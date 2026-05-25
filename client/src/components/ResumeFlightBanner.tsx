import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { ActiveFlightInfo, ResumableFlight } from "../types";
import { useConfirm } from "./ConfirmDialog";

const COUNTDOWN_SECONDS = 30;

interface Props {
  /** Already-attached active flight (e.g. restored from disk). */
  activeFlight: ActiveFlightInfo | null;
  /** Notify the dashboard when adoption succeeded. */
  onAdopted: (info: ActiveFlightInfo) => void;
  /** Notify the dashboard when the flight was cancelled. */
  onCancelled: () => void;
}

type Mode =
  /** No banner shown. */
  | { kind: "idle" }
  /**
   * A flight was just auto-resumed from disk persistence. The streamer is
   * deliberately NOT running yet — it starts when the countdown elapses
   * (or the user clicks Resume now). Cancel calls flight_cancel and aborts
   * the PIREP on phpVMS.
   */
  | {
      kind: "auto_resumed";
      flight: ActiveFlightInfo;
      secondsLeft: number;
      busy: boolean;
    }
  /**
   * phpVMS reports an in-progress PIREP but we don't have it locally. After
   * countdown / accept we adopt it (which attaches the flight + starts
   * streaming). Cancel calls flight_cancel and aborts the PIREP.
   */
  | {
      kind: "discovered";
      flight: ResumableFlight;
      secondsLeft: number;
      busy: boolean;
    };

export function ResumeFlightBanner({
  activeFlight,
  onAdopted,
  onCancelled,
}: Props) {
  const { t } = useTranslation();
  const { confirm, dialog: confirmDialog } = useConfirm();
  const [mode, setMode] = useState<Mode>({ kind: "idle" });
  const consumedRef = useRef(false);
  /**
   * Guard against doConfirm being re-entered when its setMode(busy=true)
   * triggers another useEffect run that still sees secondsLeft <= 0.
   * Without this, three streamers got spawned in the same tick.
   */
  const confirmingRef = useRef(false);
  /**
   * v0.12.1 (Stream E): true when the resumed sim position looks like a
   * glitchy crash-reload (persisted phase airborne but sim on-ground, or
   * implausible drift). While true the countdown must NOT auto-confirm —
   * the pilot has to actively press Resume. Kept in a ref so the countdown
   * effect can read it without re-subscribing to every activeFlight poll.
   */
  const positionSuspectRef = useRef(false);
  useEffect(() => {
    positionSuspectRef.current = activeFlight?.resume_position_suspect === true;
  }, [activeFlight]);

  // Disk-resume: when activeFlight first arrives with was_just_resumed=true,
  // show the auto-resumed banner.
  useEffect(() => {
    if (
      activeFlight &&
      activeFlight.was_just_resumed &&
      mode.kind === "idle" &&
      !consumedRef.current
    ) {
      consumedRef.current = true;
      setMode({
        kind: "auto_resumed",
        flight: activeFlight,
        secondsLeft: COUNTDOWN_SECONDS,
        busy: false,
      });
    }
  }, [activeFlight, mode.kind]);

  // phpVMS-discovered (no local active flight): poll discover once on mount
  // when there's nothing attached.
  useEffect(() => {
    if (activeFlight) return;
    if (mode.kind !== "idle") return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await invoke<ResumableFlight[]>(
          "flight_discover_resumable",
        );
        if (cancelled) return;
        if (list.length > 0) {
          // Block the auto_resumed path from also firing once flight_adopt
          // sets was_just_resumed=true on the backend — otherwise we'd show
          // a second banner right after this one is dismissed.
          consumedRef.current = true;
          setMode({
            kind: "discovered",
            flight: list[0]!,
            secondsLeft: COUNTDOWN_SECONDS,
            busy: false,
          });
        }
      } catch {
        // ignore — silently no banner
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeFlight, mode.kind]);

  // Countdown ticker. Guarded against re-entry: setMode(busy=true) inside
  // doConfirm causes this useEffect to re-run with the same secondsLeft<=0,
  // so without confirmingRef we'd fire doConfirm three times in a row.
  useEffect(() => {
    if (mode.kind !== "auto_resumed" && mode.kind !== "discovered") return;
    if (mode.busy) return;
    // v0.12.1 (Stream E): a suspect resume position freezes the countdown —
    // no silent auto-confirm. The pilot must press Resume themselves.
    if (mode.kind === "auto_resumed" && positionSuspectRef.current) return;
    if (mode.secondsLeft <= 0) {
      if (confirmingRef.current) return;
      confirmingRef.current = true;
      void doConfirm();
      return;
    }
    const timer = setTimeout(() => {
      setMode((prev) =>
        prev.kind === "auto_resumed" || prev.kind === "discovered"
          ? { ...prev, secondsLeft: prev.secondsLeft - 1 }
          : prev,
      );
    }, 1000);
    return () => clearTimeout(timer);
    // doConfirm is stable in scope for one render cycle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  async function doConfirm() {
    if (mode.kind === "auto_resumed") {
      setMode((prev) =>
        prev.kind === "auto_resumed" ? { ...prev, busy: true } : prev,
      );
      try {
        await invoke("flight_resume_confirm");
        setMode({ kind: "idle" });
      } catch (err) {
        const msg = errMsg(err);
        alert(`${t("resume.confirm_failed")}\n\n${msg}`);
        setMode({ kind: "idle" });
      }
      return;
    }
    if (mode.kind === "discovered") {
      const pirepId = mode.flight.pirep_id;
      setMode((prev) =>
        prev.kind === "discovered" ? { ...prev, busy: true } : prev,
      );
      try {
        const info = await invoke<ActiveFlightInfo>("flight_adopt", {
          pirepId,
        });
        await invoke("flight_resume_confirm");
        onAdopted(info);
        setMode({ kind: "idle" });
      } catch (err) {
        const msg = errMsg(err);
        alert(`${t("resume.adopt_failed")}\n\n${msg}`);
        setMode({ kind: "idle" });
      }
    }
  }

  async function doCancel() {
    if (mode.kind !== "auto_resumed" && mode.kind !== "discovered") return;
    if (
      !(await confirm({
        message: t("resume.confirm_cancel"),
        destructive: true,
      }))
    )
      return;
    setMode((prev) =>
      prev.kind === "auto_resumed" || prev.kind === "discovered"
        ? { ...prev, busy: true }
        : prev,
    );
    try {
      // For auto_resumed: ActiveFlight is already attached → flight_cancel
      // works. For discovered: nothing is attached yet, but we want to cancel
      // the discovered PIREP on phpVMS — call flight_adopt first to attach,
      // then flight_cancel.
      if (mode.kind === "discovered") {
        await invoke<ActiveFlightInfo>("flight_adopt", {
          pirepId: mode.flight.pirep_id,
        });
      }
      await invoke("flight_cancel");
      onCancelled();
      setMode({ kind: "idle" });
    } catch (err) {
      const msg = errMsg(err);
      alert(`${t("resume.cancel_failed")}\n\n${msg}`);
      setMode({ kind: "idle" });
    }
  }

  if (mode.kind === "idle") return null;

  // v0.12.1 (Stream E): suspect resume position — show a warning + no
  // countdown, the pilot must confirm manually.
  const positionSuspect =
    mode.kind === "auto_resumed" &&
    activeFlight?.resume_position_suspect === true;

  const flight =
    mode.kind === "auto_resumed"
      ? {
          // Resumed (disk) flights know the operating airline; render it as
          // part of the callsign so "DLH155" shows instead of bare "155".
          callsign: mode.flight.airline_icao
            ? `${mode.flight.airline_icao} ${mode.flight.flight_number}`
            : mode.flight.flight_number,
          dpt_airport: mode.flight.dpt_airport,
          arr_airport: mode.flight.arr_airport,
        }
      : {
          // Discovered (phpVMS) flights: PirepSummary doesn't carry the
          // airline ICAO, so we fall back to flight_number alone — the
          // dashboard's ActiveFlightPanel surfaces it correctly once
          // adoption pulls the matching bid.
          callsign: mode.flight.flight_number,
          dpt_airport: mode.flight.dpt_airport,
          arr_airport: mode.flight.arr_airport,
        };

  return (
    <section className="resume-modal" role="status" aria-live="polite">
        {confirmDialog}
        <div className="resume-modal__header">
          <span className="resume-modal__icon" aria-hidden="true">
            ✈
          </span>
          <h2 className="resume-modal__title">{t("resume.title")}</h2>
        </div>

        <div className="resume-modal__route">
          <div className="resume-modal__icao">{flight.dpt_airport}</div>
          <div className="resume-modal__arrow">→</div>
          <div className="resume-modal__icao">{flight.arr_airport}</div>
        </div>

        <div className="resume-modal__callsign">{flight.callsign}</div>

        {positionSuspect ? (
          <div
            className="resume-modal__warning"
            role="alert"
            style={{
              margin: "10px 0",
              padding: "12px 14px",
              borderRadius: 6,
              background: "rgba(239,68,68,0.15)",
              border: "2px solid rgba(239,68,68,0.65)",
              color: "#fca5a5",
              fontSize: "0.9rem",
              lineHeight: 1.5,
            }}
          >
            <strong style={{ display: "block", marginBottom: 6, fontSize: "1rem", color: "#ef4444" }}>
              ⚠ {t("resume.hard_stop_title")}
            </strong>
            {t("resume.hard_stop_body")}
          </div>
        ) : (
          <div className="resume-modal__countdown">
            <div
              className="resume-modal__countdown-bar"
              style={{
                width: `${(mode.secondsLeft / COUNTDOWN_SECONDS) * 100}%`,
              }}
            />
            <span className="resume-modal__countdown-text">
              {t("resume.countdown", { seconds: mode.secondsLeft })}
            </span>
          </div>
        )}

        {positionSuspect ? (
          // v0.13.0 Stream F (LE22-LE26): Pilot-getriggerter Re-Check.
          // Drei Optionen — Pilot MUSS aktiv entscheiden:
          //   1. PRIMARY (grün): "Position prüfen + fortsetzen" — der Pilot
          //      positioniert sich erst selbst im Sim zur gespeicherten
          //      Position, klickt dann. Wenn drift < 5nm → clean resume.
          //   2. SECONDARY (gelb, klein): "Trotzdem fortsetzen (untrusted)" —
          //      force-resume mit Risiko dass PIREP in Review-Queue landet.
          //   3. DANGER (rot): "Flug verwerfen + neu starten".
          //
          // Toleranz: bis ~10min Gap-Zeit kein Problem solange Position passt.
          <RecheckActions
            busy={mode.busy}
            onConfirm={() => {
              if (confirmingRef.current) return;
              confirmingRef.current = true;
              void doConfirm();
            }}
            onCancel={() => void doCancel()}
          />
        ) : (
          <div className="resume-modal__actions">
            <button
              type="button"
              className="button button--primary resume-modal__primary"
              onClick={() => {
                if (confirmingRef.current) return;
                confirmingRef.current = true;
                void doConfirm();
              }}
              disabled={mode.busy}
            >
              {mode.busy ? t("resume.adopting") : t("resume.adopt_now")}
            </button>
            <button
              type="button"
              className="resume-modal__danger"
              onClick={() => void doCancel()}
              disabled={mode.busy}
            >
              {t("resume.cancel_flight")}
            </button>
          </div>
        )}
    </section>
  );
}

function errMsg(err: unknown): string {
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: string }).message);
  }
  return String(err);
}

// ─── v0.13.0 Stream F: RecheckActions ────────────────────────────────────
//
// Drei-Button-Workflow für position_suspect=true:
//   1. "Position jetzt prüfen + fortsetzen" (primary, grün):
//      Pilot positioniert sich erst manuell im Sim, klickt dann diesen
//      Button. invoke("flight_resume_check_position") berechnet aktuelle
//      Drift. Wenn ok=true → was_just_resumed wird Rust-seitig gecleared,
//      Banner schließt sich beim nächsten flight_status-Poll automatisch
//      (positionSuspect→false), normaler Flug läuft weiter ohne untrusted.
//      Wenn ok=false → wir zeigen die aktuelle Drift, Pilot kann nochmal
//      positionieren und nochmal prüfen.
//   2. "Trotzdem fortsetzen (untrusted)" (secondary, klein):
//      ruft doConfirm direkt → Force-Resume, was_just_resumed=true bleibt,
//      Server flagged PIREP als untrusted.
//   3. "Flug verwerfen" (danger, rot):
//      ruft doCancel → flight_cancel auf phpVMS, Bid wird frei für neuen
//      Versuch.

interface RecheckResult {
  ok: boolean;
  drift_nm: number;
  sim_on_ground_inconsistent: boolean;
  persisted_phase: string;
  detail: string;
}

function RecheckActions({
  busy,
  onConfirm,
  onCancel,
}: {
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [checking, setChecking] = useState(false);
  const [lastResult, setLastResult] = useState<RecheckResult | null>(null);
  const [showForce, setShowForce] = useState(false);

  async function doRecheck() {
    setChecking(true);
    setLastResult(null);
    try {
      const r = await invoke<RecheckResult>("flight_resume_check_position");
      setLastResult(r);
      if (r.ok) {
        // Server hat was_just_resumed gecleared. Banner schließt sich beim
        // nächsten flight_status-Poll automatisch. Wir starten zur Sicherheit
        // den normalen Resume-Pfad damit der Stream sofort startet ohne
        // 500ms zu warten.
        onConfirm();
      }
    } catch (err) {
      setLastResult({
        ok: false,
        drift_nm: 0,
        sim_on_ground_inconsistent: false,
        persisted_phase: "?",
        detail: errMsg(err),
      });
    } finally {
      setChecking(false);
    }
  }

  return (
    <div className="resume-modal__actions" style={{ flexDirection: "column", gap: 10 }}>
      {/* Result-Feedback nach Re-Check */}
      {lastResult && !lastResult.ok && (
        <div
          role="alert"
          style={{
            padding: "10px 12px",
            borderRadius: 6,
            background: "rgba(239,68,68,0.12)",
            border: "1px solid rgba(239,68,68,0.45)",
            color: "#fca5a5",
            fontSize: "0.85rem",
            lineHeight: 1.5,
          }}
        >
          <strong style={{ display: "block", marginBottom: 4 }}>
            ⚠ Drift: {lastResult.drift_nm.toFixed(2)} nm
          </strong>
          {lastResult.detail}
        </div>
      )}

      {/* PRIMARY: Position prüfen + fortsetzen */}
      <button
        type="button"
        className="button button--primary"
        style={{
          width: "100%",
          padding: "12px",
          fontWeight: 600,
          background: "#16a34a",
          borderColor: "#15803d",
        }}
        onClick={() => void doRecheck()}
        disabled={busy || checking}
      >
        {checking
          ? t("resume.recheck_checking")
          : "🟢 " + t("resume.recheck_check_now")}
      </button>

      {/* DANGER: Flug verwerfen */}
      <button
        type="button"
        className="resume-modal__danger"
        style={{ width: "100%", padding: "10px" }}
        onClick={onCancel}
        disabled={busy || checking}
      >
        🔴 {t("resume.hard_stop_discard")}
      </button>

      {/* Toggle für force-resume (versteckt damit Piloten es nicht aus
          Versehen klicken — sie müssen erst auf "Erweitert" klicken) */}
      {!showForce ? (
        <button
          type="button"
          className="button"
          style={{
            width: "100%",
            padding: "6px",
            fontSize: "0.75rem",
            opacity: 0.6,
            background: "transparent",
            border: "1px dashed rgba(150,150,150,0.4)",
            color: "#888",
          }}
          onClick={() => setShowForce(true)}
          disabled={busy || checking}
        >
          {t("resume.recheck_show_force")}
        </button>
      ) : (
        <button
          type="button"
          className="button"
          style={{
            width: "100%",
            padding: "8px",
            fontSize: "0.85rem",
            background: "rgba(251,191,36,0.12)",
            border: "1px solid rgba(251,191,36,0.45)",
            color: "#fbbf24",
          }}
          onClick={onConfirm}
          disabled={busy || checking}
        >
          {busy ? t("resume.adopting") : "⚠ " + t("resume.hard_stop_force_resume")}
        </button>
      )}
    </div>
  );
}

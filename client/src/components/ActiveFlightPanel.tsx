import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { ActiveFlightInfo } from "../types";

interface Props {
  /** Active-flight info, owned by Dashboard. Pure display. */
  info: ActiveFlightInfo | null;
  /** Notify parent when the flight ends so it can refresh bids etc. */
  onEnded?: () => void;
}

function fmtDuration(startedIso: string, locale: string): string {
  const started = new Date(startedIso).getTime();
  const ms = Date.now() - started;
  const minutes = Math.max(0, Math.floor(ms / 60000));
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m}m`;
  return locale.startsWith("de")
    ? `${h}h ${m.toString().padStart(2, "0")}m`
    : `${h}h ${m}m`;
}

function fmtDistance(nm: number, locale: string): string {
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(
    nm,
  )} nmi`;
}

export function ActiveFlightPanel({ info, onEnded }: Props) {
  const { t, i18n } = useTranslation();
  const [busy, setBusy] = useState<"end" | "cancel" | "forget" | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Tick once a second so the elapsed-time display refreshes between polls.
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  if (!info) return null;

  async function handleEnd() {
    if (busy) return;
    setBusy("end");
    setError(null);
    try {
      await invoke("flight_end");
      onEnded?.();
    } catch (err: unknown) {
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: string }).message)
          : String(err);
      setError(msg);
    } finally {
      setBusy(null);
    }
  }

  async function handleCancel() {
    if (busy) return;
    if (!confirm(t("active_flight.confirm_cancel"))) return;
    setBusy("cancel");
    setError(null);
    try {
      await invoke("flight_cancel");
      onEnded?.();
    } catch (err: unknown) {
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: string }).message)
          : String(err);
      setError(msg);
    } finally {
      setBusy(null);
    }
  }

  /**
   * Force-discard local active-flight state without touching phpVMS. Useful
   * when the cancel call fails because the PIREP is already gone server-side
   * but our local state still thinks a flight is active.
   */
  async function handleForget() {
    if (busy) return;
    if (!confirm(t("active_flight.confirm_forget"))) return;
    setBusy("forget");
    setError(null);
    try {
      await invoke("flight_forget");
      onEnded?.();
    } catch (err: unknown) {
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: string }).message)
          : String(err);
      setError(msg);
    } finally {
      setBusy(null);
    }
  }

  return (
    <section className="active-flight">
      <header className="active-flight__header">
        <div>
          <span className="active-flight__label">
            {t("active_flight.title")}
          </span>
          <h2 className="active-flight__callsign">{info.flight_number}</h2>
          <span className="active-flight__phase">
            {t(`active_flight.phase.${info.phase}`, { defaultValue: info.phase })}
          </span>
        </div>
        <div className="active-flight__route">
          <span className="active-flight__icao">{info.dpt_airport}</span>
          <span className="active-flight__arrow">→</span>
          <span className="active-flight__icao">{info.arr_airport}</span>
        </div>
        <div className="active-flight__actions">
          <button
            type="button"
            className="button button--primary"
            onClick={handleEnd}
            disabled={busy !== null}
          >
            {busy === "end" ? t("active_flight.filing") : t("active_flight.end")}
          </button>
          <button type="button" onClick={handleCancel} disabled={busy !== null}>
            {busy === "cancel"
              ? t("active_flight.cancelling")
              : t("active_flight.cancel")}
          </button>
          <button
            type="button"
            className="active-flight__forget"
            onClick={handleForget}
            disabled={busy !== null}
            title={t("active_flight.forget_hint")}
          >
            {busy === "forget"
              ? t("active_flight.forgetting")
              : t("active_flight.forget")}
          </button>
        </div>
      </header>

      <dl className="active-flight__stats">
        <div>
          <dt>{t("active_flight.elapsed")}</dt>
          <dd>{fmtDuration(info.started_at, i18n.language)}</dd>
        </div>
        <div>
          <dt>{t("active_flight.distance")}</dt>
          <dd>{fmtDistance(info.distance_nm, i18n.language)}</dd>
        </div>
        <div>
          <dt>{t("active_flight.positions")}</dt>
          <dd>{info.position_count}</dd>
        </div>
      </dl>

      {error && (
        <p className="active-flight__error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

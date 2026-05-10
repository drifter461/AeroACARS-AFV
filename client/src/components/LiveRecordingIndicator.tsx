import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  /** ISO-8601 UTC timestamp of the most recent successful post. */
  lastPositionAt: string | null;
  /** How many positions are sitting in the in-memory outbox awaiting POST. */
  queuedCount: number;
  /** Total number of positions sent across this flight. */
  positionCount: number;
  /**
   * v0.6.2 — Connection-Health from phpVMS-Worker.
   *   - "live"    → letzter POST war Erfolg
   *   - "failing" → letzter POST scheiterte (echter Network-Loss)
   *
   * Wird zusammen mit `queuedCount` für 3 klare Status verwendet:
   *   - live    + queued=0 → „Live" (grün)
   *   - live    + queued>0 → „Sync" (blau, normaler Backlog zwischen POSTs)
   *   - failing            → „Offline" (rot, echte Verbindung weg)
   *
   * Vor v0.6.2 zeigte der Indikator „queued offline" für jeden Backlog,
   * was zwischen normalen Sync-Pausen und echten Connection-Loss nicht
   * unterscheiden konnte → Pilot dachte er sei offline obwohl alles ok.
   */
  connectionState?: "live" | "failing";
}

/**
 * Visual "this flight is being recorded" indicator for the cockpit
 * panel — like the REC dot on a video camera. Four states:
 *
 *   * Live   (grün, pulse): connection live, no backlog → alles ok.
 *   * Sync   (blau, soft pulse): connection live, backlog in der
 *     Outbox wartet auf nächsten POST-Cycle (= normal in Cruise mit
 *     30s-Cadence). Pilot muss nichts tun, wird automatisch raus.
 *   * Offline (rot, no pulse): letzter POST scheiterte. Echte
 *     Verbindungs-Probleme. Backlog wächst wenn nicht behoben.
 *   * Stale  (grau, no pulse): kein POST seit > 3 min. App vermutlich
 *     hängt oder Sim-Disconnect.
 *
 * Die "X seconds ago" Zeile tickt jede Sekunde damit der Pilot Live-
 * Feedback hat dass der Streamer nicht eingefroren ist.
 */
export function LiveRecordingIndicator({
  lastPositionAt,
  queuedCount,
  positionCount,
  connectionState,
}: Props) {
  const { t } = useTranslation();
  const [, setTick] = useState(0);

  // Local 1 Hz tick so the "X seconds ago" line stays current between
  // 2 s flight_status polls. Pure cosmetic — drives no logic.
  useEffect(() => {
    const id = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const ageSecs = lastPositionAt
    ? Math.max(0, Math.floor((Date.now() - new Date(lastPositionAt).getTime()) / 1000))
    : null;

  // v0.5.51/v0.6.0 — Stale-Threshold von 60 auf 180 sec. Vorher
  // triggerte „FEHLER" sofort wenn der phpVMS-POST > 60 sec her war.
  // Mit der v0.6.0-Architektur (Memory-Outbox + eigener phpVMS-Worker
  // mit phase-aware Cadence 4-30s) ist „60 sec Pause" absolut normal
  // im Cruise. 180 sec unterscheidet echte Connection-Probleme von
  // normalen Pausen zwischen Batches.
  const STALE_THRESHOLD_SEC = 180;

  // v0.6.2 — 3 Status statt 2 (Live / Sync / Offline / Stale).
  // Priority: Stale > Offline (failing) > Sync (queued+live) > Live.
  // - Stale wenn lange nichts: vermutlich App tot ODER Sim-Disconnect
  // - Offline wenn letzter POST gescheitert: echte Verbindungs-Probleme
  // - Sync wenn Backlog UND letzter POST Erfolg: nur Cadence-Pause
  // - Live wenn Backlog leer UND letzter POST Erfolg
  const status: "live" | "sync" | "offline" | "stale" | "idle" =
    ageSecs == null
      ? "idle"
      : ageSecs > STALE_THRESHOLD_SEC
        ? "stale"
        : connectionState === "failing"
          ? "offline"
          : queuedCount > 0
            ? "sync"
            : "live";

  const label = t(`recording.status.${status}`);
  const detail =
    ageSecs == null
      ? t("recording.no_post_yet")
      : status === "offline"
        ? t("recording.offline_pending", { count: queuedCount })
        : status === "sync"
          ? t("recording.sync_pending", { count: queuedCount })
          : t("recording.last_send_secs", { secs: ageSecs });

  // v0.5.51 — UI-Klarstellung. Vorher stand einfach nur die Zahl
  // `positionCount` ohne Label rechts in der Pille. Bei status="stale"
  // las das aus wie „FEHLER 1101" → Pilot denkt 1101 wäre ein Fehler-Code.
  // Jetzt: explizites Σ-Symbol + i18n-Tooltip + visueller Separator.
  return (
    <div
      className={`live-rec live-rec--${status}`}
      role="status"
      aria-live="polite"
      title={`${label} — ${detail} · ${t("recording.total_sent")}: ${positionCount}`}
    >
      <span className="live-rec__dot" aria-hidden="true" />
      <span className="live-rec__label">{label}</span>
      <span className="live-rec__detail">{detail}</span>
      <span className="live-rec__sep" aria-hidden="true">·</span>
      <span className="live-rec__count" title={t("recording.total_sent")}>
        Σ&nbsp;{positionCount}
      </span>
    </div>
  );
}

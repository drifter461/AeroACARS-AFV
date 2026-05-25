// v0.13.5 Tab-unabhängiger Auto-File-Watcher
//
// Befund (Torben, 2026-05-25): Wenn der Pilot vor dem PIREP-Submit zu einem
// anderen Tab als "Cockpit" wechselt (z.B. "Landung", um sich die Landung
// schon mal anzusehen), wird der PIREP nicht automatisch eingereicht. Grund:
// die Auto-File-`useEffect` saß bisher in `CockpitView.tsx` (Zeilen 73-121
// in v0.13.4), und CockpitView wird nur dann gemountet wenn `tab === "cockpit"`.
// Tab-Switch → CockpitView wird unmounted → useEffect wird aufgelöst → keine
// Auto-File-Triggerung mehr beim Phase-Wechsel auf "arrived".
//
// Fix: Auto-File-Logik in diesen headless Component auslagern. Wird in
// App.tsx IMMER gemountet wenn der User eingeloggt ist, unabhängig vom
// aktiven Tab. Component rendert nichts (keine UI) — pure Side-Effect-
// Triggerung über useEffect.
//
// Verhalten identisch zur alten CockpitView-Logik:
//   - Wenn `autoFile` Setting an UND `activeFlight.phase === "arrived"`:
//     einmal pro pirep_id `flight_end` invoken.
//   - Bei divert_hint NICHT auto-file (Pilot muss DivertBanner-Entscheidung
//     treffen).
//   - Bei Erfolg: activeFlight auf null setzen (sofort, kein Poll-Race).
//   - Bei Fehler: activity_log_add Warning + Toast — Pilot muss manuell
//     "Flug beenden" klicken.

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ActiveFlightInfo } from "../types";

interface Props {
  activeFlight: ActiveFlightInfo | null;
  autoFile: boolean;
  setActiveFlight: (f: ActiveFlightInfo | null) => void;
}

export function AutoFilePirepWatcher({
  activeFlight,
  autoFile,
  setActiveFlight,
}: Props) {
  const autoFiledRef = useRef<string | null>(null);

  useEffect(() => {
    if (!activeFlight) {
      autoFiledRef.current = null;
      return;
    }
    if (!autoFile) return;
    if (activeFlight.phase !== "arrived") return;
    // Suppress auto-file when we've detected a divert. The pilot must
    // explicitly choose "submit as divert to X" / "submit as planned"
    // / "override" via the DivertBanner — silently filing with the
    // wrong arr_airport_id would defeat the whole point.
    if (activeFlight.divert_hint) return;
    if (autoFiledRef.current === activeFlight.pirep_id) return;
    autoFiledRef.current = activeFlight.pirep_id;
    void (async () => {
      try {
        await invoke("flight_end");
        // Clear the active flight in the React tree *immediately*
        // instead of waiting for the next 2 s status poll to notice.
        // Without this, pilots reported the cockpit panel sticking
        // around after the auto-file completed; the polling-only
        // path had a race window where a stale poll could overwrite
        // a "no flight" reading and bring it back briefly.
        setActiveFlight(null);
      } catch (err: unknown) {
        // v0.7.17 (B-006): Auto-File-Failure war vorher KOMPLETT
        // stumm — catch{} schluckte alles, Pilot dachte „auto-filed"
        // aber tatsaechlich war der PIREP noch lokal nicht gefilt
        // (z.B. „not_at_arrival" weil Pilot inzwischen vom Gate weg,
        // oder „fuel" weil Block-Fuel fehlt). Pilot lief in den
        // Stale-Stream-Zustand (B-004).
        //
        // Jetzt: Activity-Log-Warning + UI-Toast damit der Pilot weiss
        // dass Auto-File scheiterte und er manuell „Flug beenden"
        // klicken muss. activeFlight bleibt erhalten damit der
        // manuelle Button weiter funktioniert. autoFiledRef bleibt
        // gesetzt → kein Retry-Loop.
        const errObj = err as { code?: string; message?: string } | undefined;
        const errCode = errObj?.code ?? "unknown";
        const errMsg = errObj?.message ?? String(err);
        void invoke("activity_log_add", {
          level: "warn",
          message: 'Auto-File fehlgeschlagen — bitte manuell „Flug beenden" klicken',
          detail: `${errCode}: ${errMsg}`,
        }).catch(() => null);
      }
    })();
  }, [activeFlight, autoFile, setActiveFlight]);

  return null;
}

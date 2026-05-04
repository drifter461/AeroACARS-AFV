import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ActiveFlightInfo } from "../types";

interface Props {
  info: ActiveFlightInfo;
}

/**
 * Live-Loadsheet (v0.3.0 — überarbeitet).
 *
 * **Sichtbarkeit:** nur in Phase = `preflight` oder `boarding`.
 * Sobald Pushback / TaxiOut beginnt → komplett weg, weil dann der
 * Cruise/Approach-Pfad relevant ist und das Loadsheet "fertig" ist.
 *
 * **Optik:** identisch zum InfoStrip darüber — gleiche `.info-strip`-
 * Klasse, gleiche `Group`/`Cell`-Struktur, gleiche Schriftgrößen
 * und Monospace-Werte. Pilot soll's als nahtlose Erweiterung der
 * MASSE/FLUG/TRIP-Zeilen sehen, nicht als eigene Box mit anderem
 * Stil.
 *
 * **Toggle:** kleiner Aufklapp-Pfeil rechts in der Group-Label-Zeile.
 * Default offen während Boarding.
 *
 * **Δ-Anzeige:** kompakt inline (z.B. `TOW 64.544 kg (+227)`) statt
 * einer eigenen Spalte. Farbcode wie sonst: <5% grün, 5-10% gelb,
 * >10% rot. Bei Overweight (IST > MAX): rote Δ + ⚠.
 */
export function LoadsheetMonitor({ info }: Props) {
  const { t } = useTranslation();
  // Hook-Reihenfolge: alle Hooks vor early returns, sonst stolpert
  // React beim Re-Render wenn die Phase wechselt und der Component
  // mal mit/ohne useState aufgerufen wird.
  const [expanded, setExpanded] = useState(true);

  // Sichtbar nur in den Boarding-Phasen — sobald TaxiOut beginnt,
  // ist das Loadsheet "abgeschlossen" und wir blenden's komplett aus.
  const visible =
    info.phase === "preflight" || info.phase === "boarding";
  if (!visible) return null;

  // Wenn weder Plan noch Live-Werte da sind, nichts rendern.
  const hasAnyPlan =
    info.planned_block_fuel_kg != null ||
    info.planned_zfw_kg != null ||
    info.planned_tow_kg != null;
  const hasAnyLive =
    info.sim_fuel_kg != null ||
    info.sim_zfw_kg != null ||
    info.sim_tow_kg != null;
  if (!hasAnyPlan && !hasAnyLive) return null;

  // Status-Hint
  const fuelDelta =
    info.sim_fuel_kg != null && info.planned_block_fuel_kg != null
      ? info.sim_fuel_kg - info.planned_block_fuel_kg
      : null;
  const zfwDelta =
    info.sim_zfw_kg != null && info.planned_zfw_kg != null
      ? info.sim_zfw_kg - info.planned_zfw_kg
      : null;
  const hint = computeHint(fuelDelta, zfwDelta, t);

  return (
    <section className="info-strip">
      {/* Header-Zeile mit Toggle-Button rechts */}
      <div className="info-strip__group loadsheet__header-row">
        <h4 className="info-strip__group-label">
          {t("cockpit.loadsheet.label")}
        </h4>
        <button
          type="button"
          className="loadsheet__toggle"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
          title={
            expanded
              ? t("cockpit.loadsheet.collapse")
              : t("cockpit.loadsheet.expand")
          }
        >
          {expanded ? "▾" : "▸"}
        </button>
      </div>

      {/* Werte-Zeile — gleicher Stil wie MASSE-Strip oben */}
      {expanded && (
        <>
          <div className="info-strip__group">
            <h4 className="info-strip__group-label">
              {t("cockpit.loadsheet.ist_label")}
            </h4>
            <div className="info-strip__cells">
              <Cell
                label={t("cockpit.loadsheet.block")}
                ist={info.sim_fuel_kg}
                soll={info.planned_block_fuel_kg}
                max={null}
              />
              <Cell
                label="ZFW"
                ist={info.sim_zfw_kg}
                soll={info.planned_zfw_kg}
                max={info.planned_max_zfw_kg}
              />
              <Cell
                label="TOW"
                ist={info.sim_tow_kg}
                soll={info.planned_tow_kg}
                max={info.planned_max_tow_kg}
              />
            </div>
          </div>
          {hint && (
            <div className="info-strip__group">
              <h4 className="info-strip__group-label">&nbsp;</h4>
              <div className="loadsheet__hint-inline">{hint}</div>
            </div>
          )}
        </>
      )}
    </section>
  );
}

/**
 * Eine Loadsheet-Cell im InfoStrip-Stil. Format: `LABEL 6.334 kg (+0)`
 * mit Δ inline und farbcodiert. Bei MAX-Wert + Overweight: ⚠-Indikator.
 */
function Cell({
  label,
  ist,
  soll,
  max,
}: {
  label: string;
  ist: number | null;
  soll: number | null;
  max: number | null;
}) {
  // Wenn weder IST noch SOLL da sind, Cell überspringen.
  if (ist == null && soll == null) return null;

  const delta = ist != null && soll != null ? ist - soll : null;
  const deltaPct =
    delta != null && soll != null && soll !== 0
      ? Math.abs(delta / soll) * 100
      : null;

  // Δ-Farbcode: <5% grün, 5-10% gelb, >10% rot. Wird auf den
  // Delta-Suffix angewendet, nicht auf den Hauptwert.
  let deltaClass = "loadsheet__delta--ok";
  if (deltaPct != null) {
    if (deltaPct >= 10) deltaClass = "loadsheet__delta--alert";
    else if (deltaPct >= 5) deltaClass = "loadsheet__delta--warn";
  }

  // Overweight: IST > MAX → ⚠ + alert-color
  const overweight = ist != null && max != null && ist > max;
  if (overweight) deltaClass = "loadsheet__delta--alert";

  const istLabel =
    ist != null ? `${Math.round(ist).toLocaleString("de-DE")} kg` : "—";

  return (
    <div className="info-strip__cell">
      <span className="info-strip__cell-label">{label}</span>
      <span className="info-strip__cell-value">{istLabel}</span>
      {delta != null && (
        <span className={`loadsheet__delta-inline ${deltaClass}`}>
          {overweight ? "⚠ " : ""}
          {delta >= 0 ? "+" : ""}
          {Math.round(delta).toLocaleString("de-DE")}
        </span>
      )}
    </div>
  );
}

function computeHint(
  fuelDelta: number | null,
  zfwDelta: number | null,
  t: (k: string, opts?: Record<string, unknown>) => string,
): string | null {
  if (fuelDelta == null && zfwDelta == null) return null;

  const fuelOk = fuelDelta == null || Math.abs(fuelDelta) < 200;
  const zfwOk = zfwDelta == null || Math.abs(zfwDelta) < 200;
  if (fuelOk && zfwOk) {
    return t("cockpit.loadsheet.hint_ready");
  }

  if (fuelDelta != null && fuelDelta < -300) {
    return t("cockpit.loadsheet.hint_fueling", {
      missing: Math.abs(Math.round(fuelDelta)).toLocaleString("de-DE"),
    });
  }
  if (zfwDelta != null && zfwDelta < -300) {
    return t("cockpit.loadsheet.hint_boarding", {
      missing: Math.abs(Math.round(zfwDelta)).toLocaleString("de-DE"),
    });
  }
  if (fuelDelta != null && fuelDelta > 500) {
    return t("cockpit.loadsheet.hint_overfueled", {
      extra: Math.round(fuelDelta).toLocaleString("de-DE"),
    });
  }

  return null;
}

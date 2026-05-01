import { useTranslation } from "react-i18next";
import type { SimSnapshot } from "../types";

interface Props {
  snapshot: SimSnapshot | null;
}

/**
 * Compact flight-info readout shown in the Cockpit tab. Mirrors the
 * "block info" panel from vmsACARS: wind, temperatures, Mach, QNH —
 * the kind of in-flight numbers a pilot wants visible at a glance
 * without diving into the debug section.
 */
export function FlightInfoPanel({ snapshot }: Props) {
  const { t, i18n } = useTranslation();
  if (!snapshot) return null;

  const wind = formatWind(snapshot, i18n.language);
  const oat = formatTemp(snapshot.outside_air_temp_c);
  const tat = formatTemp(snapshot.total_air_temp_c);
  const mach = formatMach(snapshot.mach);
  const qnh = formatQnh(snapshot.qnh_hpa, i18n.language);

  return (
    <section className="flight-info-panel">
      <h3 className="flight-info-panel__title">{t("flight_info.title")}</h3>
      <dl className="flight-info-panel__grid">
        <Cell label={t("flight_info.wind")} value={wind} />
        <Cell label={t("flight_info.qnh")} value={qnh} />
        <Cell label={t("flight_info.oat")} value={oat} />
        <Cell label={t("flight_info.tat")} value={tat} />
        <Cell label={t("flight_info.mach")} value={mach} />
      </dl>
    </section>
  );
}

/**
 * Format wind as "DDD/SS" matching aviation METAR convention. Returns
 * "—" when neither direction nor speed are known.
 */
function formatWind(snap: SimSnapshot, _locale: string): string {
  if (snap.wind_direction_deg === null && snap.wind_speed_kt === null) {
    return "—";
  }
  const dir = snap.wind_direction_deg ?? 0;
  const spd = snap.wind_speed_kt ?? 0;
  const dirNorm = ((dir % 360) + 360) % 360;
  return `${Math.round(dirNorm).toString().padStart(3, "0")}/${Math.round(spd)
    .toString()
    .padStart(2, "0")}`;
}

function formatTemp(c: number | null): string {
  if (c === null) return "—";
  const sign = c >= 0 ? "+" : "";
  return `${sign}${Math.round(c)}°C`;
}

function formatMach(m: number | null): string {
  if (m === null || m <= 0) return "—";
  return `M ${m.toFixed(2)}`;
}

function formatQnh(hpa: number | null, locale: string): string {
  if (hpa === null) return "—";
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(
    hpa,
  )} hPa`;
}

function Cell({ label, value }: { label: string; value: string }) {
  return (
    <div className="flight-info-panel__cell">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

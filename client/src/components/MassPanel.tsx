import { useTranslation } from "react-i18next";
import type { SimSnapshot } from "../types";

interface Props {
  snapshot: SimSnapshot | null;
}

/**
 * Mass readout shown in the Cockpit tab — five values that match
 * what an Airbus EFB / Boeing FMC presents on the perf page:
 *
 *   DOW    — Dry Operating Weight (empty + crew, no fuel/payload)
 *   PLD    — Payload (passengers + cargo)
 *   ZFW    — Zero Fuel Weight (DOW + PLD)
 *   FOB    — Fuel On Board
 *   GW     — Gross Weight (ZFW + FOB)
 *
 * Sources:
 *   - GW comes from `TOTAL WEIGHT` SimVar (we verified it matches
 *     the cockpit FOB/GW display on Fenix to the kg).
 *   - FOB from the fuel pipeline (FUEL TOTAL QUANTITY WEIGHT or
 *     addon-specific LVar).
 *   - ZFW = GW - FOB (computed).
 *   - DOW from `EMPTY WEIGHT` SimVar — caveat: broken on Asobo
 *     default airliners (returns ~3000 lb instead of OEW). On
 *     Fenix/FBW/PMDG the value is real.
 *   - PLD = ZFW - DOW.
 *
 * Values that come back as 0/null get rendered as "—" to make it
 * visually obvious the addon doesn't wire that field.
 */
export function MassPanel({ snapshot }: Props) {
  const { t, i18n } = useTranslation();
  if (!snapshot) return null;

  const fob = snapshot.fuel_total_kg > 0 ? snapshot.fuel_total_kg : null;
  const gw =
    snapshot.total_weight_kg !== null && snapshot.total_weight_kg > 0
      ? snapshot.total_weight_kg
      : null;
  const zfw =
    snapshot.zfw_kg !== null && snapshot.zfw_kg > 0 ? snapshot.zfw_kg : null;
  // DOW heuristic: only trust EMPTY WEIGHT if it's plausibly above
  // 1 tonne. Asobo's default airliners return values in the 1400 kg
  // range which we know are bogus.
  const dow = computeDow(snapshot);
  const payload = dow !== null && zfw !== null && zfw > dow ? zfw - dow : null;

  return (
    <section className="mass-panel">
      <h3 className="mass-panel__title">{t("mass_panel.title")}</h3>
      <dl className="mass-panel__grid">
        <Cell label={t("mass_panel.dow")} value={fmtKg(dow, i18n.language)} />
        <Cell
          label={t("mass_panel.payload")}
          value={fmtKg(payload, i18n.language)}
        />
        <Cell label={t("mass_panel.zfw")} value={fmtKg(zfw, i18n.language)} />
        <Cell label={t("mass_panel.fob")} value={fmtKg(fob, i18n.language)} />
        <Cell
          label={t("mass_panel.gross_weight")}
          value={fmtKg(gw, i18n.language)}
          modifier="mass-panel__cell--primary"
        />
      </dl>
    </section>
  );
}

/**
 * EMPTY WEIGHT is wildly wrong on Asobo's default airliners (we
 * observed ~1422 kg on the A320neo — under the OEW of a King Air).
 * Heuristic: only show DOW when the value is at least 3 t — that's
 * just above any GA aircraft and well below the smallest commercial
 * airliner's OEW, so it sorts trustworthy addon values from the
 * Asobo CFG glitches.
 */
function computeDow(snap: SimSnapshot): number | null {
  const ew = snap.empty_weight_kg;
  if (ew === null || ew < 3000) return null;
  return ew;
}

function fmtKg(kg: number | null, locale: string): string {
  if (kg === null) return "—";
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(
    kg,
  )} kg`;
}

function Cell({
  label,
  value,
  modifier,
}: {
  label: string;
  value: string;
  modifier?: string;
}) {
  return (
    <div className={`mass-panel__cell ${modifier ?? ""}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

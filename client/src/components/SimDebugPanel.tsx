import { useTranslation } from "react-i18next";
import type { SimSnapshot, SimStatus } from "../types";

/**
 * Display-only sim telemetry panel for the Settings tab's debug
 * section. State is fed in via props (polled centrally by
 * `useSimSession` at the App level) so it stays in sync with whatever
 * the cockpit / briefing tabs see, without a duplicate poll loop.
 */
interface Props {
  status: SimStatus | null;
}

function fmtNumber(value: number, fractionDigits: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(value);
}

function fmtCoord(deg: number): string {
  const sign = deg >= 0 ? "+" : "−";
  return `${sign}${Math.abs(deg).toFixed(6)}°`;
}

function fmtFt(ft: number, locale: string): string {
  return `${new Intl.NumberFormat(locale).format(Math.round(ft))} ft`;
}

function fmtKt(kt: number, locale: string): string {
  return `${new Intl.NumberFormat(locale).format(Math.round(kt))} kt`;
}

function fmtFpm(fpm: number, locale: string): string {
  const rounded = Math.round(fpm);
  const sign = rounded > 0 ? "+" : "";
  return `${sign}${new Intl.NumberFormat(locale).format(rounded)} fpm`;
}

function fmtHeading(deg: number): string {
  const norm = ((deg % 360) + 360) % 360;
  return `${Math.round(norm).toString().padStart(3, "0")}°`;
}

function fmtKg(kg: number, locale: string): string {
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(
    kg,
  )} kg`;
}

function fmtKgLb(kg: number, locale: string): string {
  // Show kg + raw lb so we can spot conversion / unit issues at a glance.
  const lb = kg / 0.45359237;
  return `${fmtKg(kg, locale)} (${new Intl.NumberFormat(locale, {
    maximumFractionDigits: 0,
  }).format(lb)} lb)`;
}

export function SimDebugPanel({ status }: Props) {
  const { t, i18n } = useTranslation();
  if (!status) {
    return <p className="sim-panel__hint">{t("sim.loading")}</p>;
  }
  const { state, kind, snapshot, last_error } = status;
  const stateLabel = t(`sim.state.${state}`);
  const isOff = kind === "off";
  const kindLabel = t(`sim.kinds.${kind}`);

  return (
    <section className={`sim-panel sim-panel--${state}`}>
      <header className="sim-panel__header">
        <div className="sim-panel__header-left">
          <h2>{t("sim.title")}</h2>
          <span className="sim-panel__kind">{kindLabel}</span>
        </div>
        <span
          className={`sim-panel__state sim-panel__state--${state}`}
          aria-live="polite"
        >
          <span className="sim-panel__dot" /> {stateLabel}
        </span>
      </header>

      {isOff && <p className="sim-panel__hint">{t("sim.off_hint")}</p>}
      {last_error && state !== "connected" && !isOff && (
        <p className="sim-panel__error" role="alert">
          {t("sim.last_error_prefix")}: <code>{last_error}</code>
        </p>
      )}
      {snapshot && state === "connected" && (
        <>
          <SnapshotGrid snap={snapshot} locale={i18n.language} />
          <MassFuelGrid snap={snapshot} locale={i18n.language} />
          <TouchdownGrid snap={snapshot} locale={i18n.language} />
        </>
      )}
    </section>
  );
}

/**
 * Live Mass & Fuel readout. Surfaces the values that get written into
 * the PIREP custom fields so the pilot can see *before* filing whether
 * the aircraft addon actually wires them. If you see "—" here, the
 * field will be dropped from the PIREP rather than written as "0 kg".
 */
function MassFuelGrid({ snap, locale }: { snap: SimSnapshot; locale: string }) {
  const { t } = useTranslation();
  return (
    <>
      <h3 className="sim-panel__section">{t("sim.sections.mass_fuel")}</h3>
      <dl className="sim-panel__grid">
        <Row label={t("sim.fields.fuel_total")}>
          {snap.fuel_total_kg > 0
            ? fmtKgLb(snap.fuel_total_kg, locale)
            : <em className="sim-panel__muted">{t("sim.fields.not_wired")}</em>}
        </Row>
        <Row label={t("sim.fields.total_weight")}>
          {snap.total_weight_kg !== null && snap.total_weight_kg > 0
            ? fmtKgLb(snap.total_weight_kg, locale)
            : <em className="sim-panel__muted">{t("sim.fields.not_wired")}</em>}
        </Row>
      </dl>
    </>
  );
}

/**
 * Last-touchdown grid. These values are populated by the simulation
 * itself the moment the gear hits the ground and stay frozen until the
 * next takeoff. If "—" is shown, no touchdown has been recorded yet
 * this session.
 */
function TouchdownGrid({ snap, locale }: { snap: SimSnapshot; locale: string }) {
  const { t } = useTranslation();
  const hasData = snap.touchdown_vs_fpm !== null;
  if (!hasData) {
    return (
      <>
        <h3 className="sim-panel__section">{t("sim.sections.touchdown")}</h3>
        <p className="sim-panel__hint">{t("sim.fields.no_touchdown_yet")}</p>
      </>
    );
  }
  return (
    <>
      <h3 className="sim-panel__section">{t("sim.sections.touchdown")}</h3>
      <dl className="sim-panel__grid">
        <Row label={t("sim.fields.touchdown_vs")}>
          {fmtFpm(snap.touchdown_vs_fpm ?? 0, locale)}
        </Row>
        <Row label={t("sim.fields.touchdown_pitch")}>
          {fmtNumber(snap.touchdown_pitch_deg ?? 0, 1, locale)}°
        </Row>
        <Row label={t("sim.fields.touchdown_bank")}>
          {fmtNumber(snap.touchdown_bank_deg ?? 0, 1, locale)}°
        </Row>
        <Row label={t("sim.fields.touchdown_heading")}>
          {fmtHeading(snap.touchdown_heading_mag_deg ?? 0)}
        </Row>
        {snap.touchdown_lat !== null && snap.touchdown_lon !== null && (
          <Row label={t("sim.fields.position")}>
            {fmtCoord(snap.touchdown_lat)} · {fmtCoord(snap.touchdown_lon)}
          </Row>
        )}
      </dl>
    </>
  );
}

function SnapshotGrid({ snap, locale }: { snap: SimSnapshot; locale: string }) {
  const { t } = useTranslation();
  return (
    <dl className="sim-panel__grid">
      <Row label={t("sim.fields.position")}>
        {fmtCoord(snap.lat)} · {fmtCoord(snap.lon)}
      </Row>
      <Row label={t("sim.fields.altitude_msl")}>
        {fmtFt(snap.altitude_msl_ft, locale)}
      </Row>
      <Row label={t("sim.fields.altitude_agl")}>
        {fmtFt(snap.altitude_agl_ft, locale)}
      </Row>
      <Row label={t("sim.fields.heading")}>
        {fmtHeading(snap.heading_deg_magnetic)} ({t("sim.fields.heading_mag")}) ·{" "}
        {fmtHeading(snap.heading_deg_true)} ({t("sim.fields.heading_true")})
      </Row>
      <Row label={t("sim.fields.groundspeed")}>
        {fmtKt(snap.groundspeed_kt, locale)}
      </Row>
      <Row label={t("sim.fields.airspeed")}>
        IAS {fmtKt(snap.indicated_airspeed_kt, locale)} · TAS{" "}
        {fmtKt(snap.true_airspeed_kt, locale)}
      </Row>
      <Row label={t("sim.fields.vertical_speed")}>
        {fmtFpm(snap.vertical_speed_fpm, locale)}
      </Row>
      <Row label={t("sim.fields.attitude")}>
        {t("sim.fields.pitch")} {fmtNumber(snap.pitch_deg, 1, locale)}° ·{" "}
        {t("sim.fields.bank")} {fmtNumber(snap.bank_deg, 1, locale)}°
      </Row>
      <Row label={t("sim.fields.g_force")}>
        {fmtNumber(snap.g_force, 2, locale)} G
      </Row>
      <Row label={t("sim.fields.on_ground")}>
        {snap.on_ground ? t("sim.yes") : t("sim.no")}
      </Row>
    </dl>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

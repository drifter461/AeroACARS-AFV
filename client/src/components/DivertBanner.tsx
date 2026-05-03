import { useEffect, useState } from "react";
import { useTranslation, Trans } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import type { ActiveFlightInfo } from "../types";

interface Props {
  activeFlight: ActiveFlightInfo;
  /** Called once a divert decision has been made and the file/cancel
   *  command has resolved. Parent uses this to clear `activeFlight`
   *  in the React tree so the cockpit collapses to the empty state. */
  onResolved: () => void;
}

/**
 * Banner shown in the cockpit when the FSM detected the aircraft
 * landed somewhere other than the planned `arr_airport`. Three
 * actions:
 *
 *   1. Submit as divert to <actual>     → flight_end({ divert_to: actual })
 *   2. Submit as planned (no override)  → flight_end()
 *   3. Override: pick another airport   → opens manual modal w/ ICAO entry
 *
 * Hidden when `activeFlight.divert_hint` is null. Hidden also when
 * the flight is still in resume-banner-pending state (was_just_resumed)
 * — we want the resume choice to settle first before piling another
 * decision on the pilot.
 */
export function DivertBanner({ activeFlight, onResolved }: Props) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showOverride, setShowOverride] = useState(false);

  const hint = activeFlight.divert_hint;
  if (!hint) return null;
  if (activeFlight.was_just_resumed) return null;

  // Skip the banner during early phases — only meaningful once the
  // FSM has actually settled at Arrived (or the universal fallback
  // promoted us). Otherwise a brief mid-flight excursion outside the
  // 2 nmi planned-arrival circle could paint a divert banner during
  // climb-out, which would be nonsensical.
  if (activeFlight.phase !== "arrived") return null;

  const titleKey =
    hint.kind === "alternate"
      ? "divert.title_alternate"
      : hint.kind === "nearest"
      ? "divert.title_nearest"
      : "divert.title_unknown";
  const bodyKey =
    hint.kind === "alternate"
      ? "divert.body_alternate"
      : hint.kind === "nearest"
      ? "divert.body_nearest"
      : "divert.body_unknown";

  const fileWith = async (divertTo: string | null) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("flight_end", divertTo ? { divertTo } : {});
      onResolved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const distanceLabel = Math.round(hint.distance_to_planned_nmi).toString();

  return (
    <>
      <section className="divert-banner" role="alert" aria-live="polite">
        <header className="divert-banner__header">
          <span className="divert-banner__icon" aria-hidden="true">
            ⚠
          </span>
          <h2 className="divert-banner__title">{t(titleKey)}</h2>
        </header>
        <p className="divert-banner__body">
          <Trans
            i18nKey={bodyKey}
            values={{
              actual: hint.actual_icao ?? "—",
              planned: hint.planned_arr_icao,
              distance: distanceLabel,
            }}
            components={{ strong: <strong /> }}
          />
        </p>
        {error && <p className="divert-banner__error">{error}</p>}
        <div className="divert-banner__actions">
          {hint.actual_icao && (
            <button
              type="button"
              className="button button--primary"
              disabled={busy}
              onClick={() => void fileWith(hint.actual_icao)}
            >
              {busy
                ? t("divert.submitting")
                : t("divert.submit_as_divert", { actual: hint.actual_icao })}
            </button>
          )}
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() => void fileWith(null)}
          >
            {t("divert.submit_as_planned", { planned: hint.planned_arr_icao })}
          </button>
          <button
            type="button"
            className="button button--ghost"
            disabled={busy}
            onClick={() => setShowOverride(true)}
          >
            {t("divert.manual_override")}
          </button>
        </div>
      </section>
      {showOverride && (
        <ManualDivertModal
          activeFlight={activeFlight}
          onClose={() => setShowOverride(false)}
          onResolved={onResolved}
        />
      )}
    </>
  );
}

interface NearestAirport {
  icao: string;
  lat: number;
  lon: number;
  distance_m: number;
  longest_runway_ft: number;
}

interface ManualProps {
  activeFlight: ActiveFlightInfo;
  onClose: () => void;
  onResolved: () => void;
}

/**
 * Modal that opens from the divert banner's "override" button.
 * Loads the 5 nearest airports from the local DB (via the new
 * `divert_nearest_airports` Tauri command, which queries
 * runway::find_nearest_airports against the touchdown coords) and
 * lets the pilot pick one — or type a custom ICAO if their actual
 * landing field isn't in the runways table.
 */
function ManualDivertModal({ activeFlight, onClose, onResolved }: ManualProps) {
  const { t } = useTranslation();
  const [nearby, setNearby] = useState<NearestAirport[] | null>(null);
  const [custom, setCustom] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const r = await invoke<NearestAirport[]>(
          "divert_nearest_airports",
          { limit: 5 },
        );
        setNearby(r);
      } catch (e) {
        setError(String(e));
        setNearby([]);
      }
    })();
  }, []);

  const file = async (icao: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("flight_end", { divertTo: icao.trim().toUpperCase() });
      onResolved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const fmtKm = (m: number): string => `${(m / 1000).toFixed(1)} km`;
  const fmtRunway = (ft: number): string =>
    ft > 0 ? `${(ft * 0.3048).toFixed(0)} m` : "—";

  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      onClick={onClose}
    >
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2 className="modal__title">{t("divert.manual_modal_title")}</h2>
        <p className="modal__hint">{t("divert.manual_modal_hint")}</p>

        {nearby === null ? (
          <p className="modal__loading">…</p>
        ) : nearby.length === 0 ? (
          <p className="modal__empty">—</p>
        ) : (
          <ul className="divert-modal__list">
            {nearby.map((a) => {
              const isPlanned = a.icao === activeFlight.arr_airport;
              return (
                <li key={a.icao}>
                  <button
                    type="button"
                    className="divert-modal__candidate"
                    disabled={busy}
                    onClick={() => void file(a.icao)}
                  >
                    <span className="divert-modal__icao">{a.icao}</span>
                    <span className="divert-modal__meta">
                      {fmtKm(a.distance_m)} · runway {fmtRunway(a.longest_runway_ft)}
                      {isPlanned && " · planned"}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}

        <div className="divert-modal__custom">
          <label htmlFor="divert-icao">
            {t("divert.manual_modal_custom_label")}
          </label>
          <input
            id="divert-icao"
            type="text"
            maxLength={4}
            placeholder={t("divert.manual_modal_custom_placeholder")}
            value={custom}
            onChange={(e) => setCustom(e.target.value)}
            disabled={busy}
          />
          <button
            type="button"
            className="button button--primary"
            disabled={busy || custom.trim().length < 3}
            onClick={() => void file(custom)}
          >
            {busy ? t("divert.submitting") : t("divert.manual_modal_submit")}
          </button>
        </div>

        {error && <p className="modal__error">{error}</p>}

        <div className="modal__footer">
          <button type="button" className="button" onClick={onClose} disabled={busy}>
            {t("divert.manual_modal_cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}

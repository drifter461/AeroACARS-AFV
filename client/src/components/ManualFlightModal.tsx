// v0.5.27 VFR/Manual-Flight-Mode-Modal.
//
// Pilot picks aircraft + enters manual flight plan when no SimBrief OFP
// is available (= small airfields, VFR flights). Two stages:
//   1. Aircraft-Picker mit Suche + Sim-Default
//   2. Manual-Plan-Form (Block-Fuel, ETA Pflicht; Rest optional)

import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { Bid, ActiveFlightInfo, UiError } from "../types";

// Local helper: konvertiert beliebigen err in UiError-Shape (analog
// zur asUiError-Funktion in BidsList.tsx).
function asUiError(err: unknown): UiError {
  if (err && typeof err === "object" && "code" in err && "message" in err) {
    const e = err as Record<string, unknown>;
    return {
      code: typeof e.code === "string" ? e.code : "unknown",
      message: typeof e.message === "string" ? e.message : String(err),
    };
  }
  return { code: "unknown", message: String(err) };
}

interface AircraftPickerEntry {
  id: number;
  registration: string;
  icao: string;
  name: string;
  airport_id: string;
  state: number;
  display: string;
}

interface ManualFlightPlan {
  aircraft_id: number;
  planned_block_fuel_kg: number;
  planned_flight_time_min: number;
  cruise_level_ft?: number;
  planned_route?: string;
  alt_airport_id?: string;
  planned_zfw_kg?: number;
  planned_burn_kg?: number;
}

interface SimContextHint {
  aircraft_icao?: string | null;
  aircraft_registration?: string | null;
  fuel_total_kg?: number | null;
}

interface Props {
  bid: Bid;
  /** Aktueller Sim-Snapshot fuer Aircraft-Default + Block-Fuel-Default. */
  simHint: SimContextHint | null;
  onClose: () => void;
  onFlightStarted: (info: ActiveFlightInfo) => void;
}

type Stage = "aircraft" | "plan" | "submitting";

export function ManualFlightModal({ bid, simHint, onClose, onFlightStarted }: Props) {
  const { t } = useTranslation();
  const [stage, setStage] = useState<Stage>("aircraft");
  const [error, setError] = useState<string | null>(null);

  // Stage 1 — Aircraft-Picker
  const [aircraftList, setAircraftList] = useState<AircraftPickerEntry[] | null>(null);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<AircraftPickerEntry | null>(null);
  const [loadingFleet, setLoadingFleet] = useState(true);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        setLoadingFleet(true);
        setError(null);
        const list = await invoke<AircraftPickerEntry[]>("fleet_list_at_airport", {
          icao: bid.flight.dpt_airport_id,
        });
        if (cancelled) return;
        setAircraftList(list);
        // Sim-Default-Auswahl: wenn der Sim ein passendes Aircraft geladen
        // hat, dieses vorauswaehlen damit Pilot nicht raten muss
        if (simHint?.aircraft_registration) {
          const match = list.find(
            (a) => a.registration.trim().toUpperCase() === simHint.aircraft_registration!.trim().toUpperCase(),
          );
          if (match) setSelected(match);
        } else if (simHint?.aircraft_icao) {
          const match = list.find(
            (a) => a.icao.trim().toUpperCase() === simHint.aircraft_icao!.trim().toUpperCase(),
          );
          if (match) setSelected(match);
        }
      } catch (err: unknown) {
        if (cancelled) return;
        const ui = asUiError(err);
        setError(`Konnte Fleet nicht laden: ${ui.message}`);
        setAircraftList([]);
      } finally {
        if (!cancelled) setLoadingFleet(false);
      }
    })();
    return () => { cancelled = true; };
  }, [bid.flight.dpt_airport_id, simHint?.aircraft_registration, simHint?.aircraft_icao]);

  const filtered = useMemo(() => {
    if (!aircraftList) return [];
    if (search.trim().length === 0) return aircraftList;
    const q = search.toLowerCase().trim();
    return aircraftList.filter((a) =>
      a.icao.toLowerCase().includes(q)
      || a.registration.toLowerCase().includes(q)
      || a.name.toLowerCase().includes(q),
    );
  }, [aircraftList, search]);

  // Stage 2 — Manual-Plan-Form
  const [blockFuelKg, setBlockFuelKg] = useState<string>(() => {
    // Sim-Default: aktueller Sim-Fuel-Wert wenn verfuegbar
    return simHint?.fuel_total_kg ? Math.round(simHint.fuel_total_kg).toString() : "";
  });
  const [flightTimeMin, setFlightTimeMin] = useState<string>("");
  const [cruiseLevel, setCruiseLevel] = useState<string>("");
  const [route, setRoute] = useState<string>("");
  const [altAirport, setAltAirport] = useState<string>("");
  const [zfwKg, setZfwKg] = useState<string>("");

  function proceedToPlan() {
    if (!selected) return;
    setError(null);
    setStage("plan");
  }

  async function submit() {
    if (!selected) return;
    const blockFuel = parseFloat(blockFuelKg);
    const ftMin = parseInt(flightTimeMin, 10);
    if (!Number.isFinite(blockFuel) || blockFuel <= 0) {
      setError("Block-Fuel muss eine positive Zahl sein");
      return;
    }
    if (!Number.isFinite(ftMin) || ftMin <= 0) {
      setError("Erwartete Flugzeit muss eine positive Zahl sein");
      return;
    }
    const plan: ManualFlightPlan = {
      aircraft_id: selected.id,
      planned_block_fuel_kg: blockFuel,
      planned_flight_time_min: ftMin,
    };
    const cl = parseInt(cruiseLevel, 10);
    if (Number.isFinite(cl) && cl > 0) plan.cruise_level_ft = cl;
    if (route.trim().length > 0) plan.planned_route = route.trim();
    if (altAirport.trim().length > 0) plan.alt_airport_id = altAirport.trim().toUpperCase();
    const zfw = parseFloat(zfwKg);
    if (Number.isFinite(zfw) && zfw > 0) plan.planned_zfw_kg = zfw;

    setStage("submitting");
    setError(null);
    try {
      const result = await invoke<ActiveFlightInfo>("flight_start_manual", {
        bidId: bid.id,
        plan,
      });
      onFlightStarted(result);
    } catch (err: unknown) {
      const ui = asUiError(err);
      setError(`${ui.code}: ${ui.message}`);
      setStage("plan");
    }
  }

  return (
    <div className="manual-modal__backdrop" onClick={() => stage !== "submitting" && onClose()}>
      <div
        className="manual-modal"
        role="dialog"
        aria-labelledby="manual-modal-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="manual-modal__head">
          <h3 id="manual-modal-title">
            🛩 VFR / Manual-Mode — {t("bids.start_flight")}
          </h3>
          <div className="manual-modal__sub">
            {bid.flight.flight_number}
            {" · "}
            {bid.flight.dpt_airport_id} → {bid.flight.arr_airport_id}
            {" · "}
            <span style={{ opacity: 0.7 }}>kein SimBrief-OFP</span>
          </div>
        </header>

        {stage === "aircraft" && (
          <div className="manual-modal__body">
            <div className="manual-modal__section-title">
              1. Aircraft auswählen
            </div>
            {loadingFleet ? (
              <div className="manual-modal__loading">Lade gesamte Fleet…</div>
            ) : aircraftList && aircraftList.length === 0 ? (
              <div className="manual-modal__empty">
                Keine Aircraft in deiner Fleet verfügbar — phpVMS-Endpoint /api/fleet nicht eingerichtet oder du hast keine Subfleet-Berechtigung. Sprich VA-Admin an.
              </div>
            ) : (
              <>
                <input
                  type="search"
                  placeholder="🔍 Suche nach ICAO / Registration / Name…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  className="manual-modal__search"
                  autoFocus
                />
                <div style={{ fontSize: "0.78rem", color: "var(--text-dim)", marginBottom: 8 }}>
                  {aircraftList?.length ?? 0} Aircraft gesamt · Aircraft am {bid.flight.dpt_airport_id} stehen oben in der Liste.
                </div>
                <div className="manual-modal__list">
                  {filtered.map((a) => {
                    const stateLabel =
                      a.state === 0 ? null :
                      a.state === 1 ? "🔒 in Use" :
                      a.state === 2 ? "✈ in Flight" :
                      "🔧 Maintenance";
                    const stateColor =
                      a.state === 0 ? undefined :
                      a.state === 1 ? "#fbbf24" :
                      a.state === 2 ? "#67e8f9" :
                      "#f87171";
                    const atDpt = a.airport_id?.toUpperCase() === bid.flight.dpt_airport_id.toUpperCase();
                    return (
                      <button
                        key={a.id}
                        type="button"
                        className={`manual-modal__list-item ${selected?.id === a.id ? "selected" : ""}`}
                        onClick={() => setSelected(a)}
                        title={a.state === 0
                          ? `Verfügbar${atDpt ? ` am ${bid.flight.dpt_airport_id}` : a.airport_id ? ` (steht in ${a.airport_id})` : ""}`
                          : `${stateLabel} — phpVMS lehnt ggf. den Prefile ab.`}
                      >
                        <span className="manual-modal__list-icao">{a.icao || "—"}</span>
                        <span className="manual-modal__list-reg">{a.registration || "—"}</span>
                        {a.airport_id && (
                          <span
                            className="manual-modal__list-name"
                            style={atDpt ? { color: "#86efac", fontWeight: 600 } : undefined}
                          >
                            @{a.airport_id}
                          </span>
                        )}
                        {a.name && a.name !== a.icao && (
                          <span className="manual-modal__list-name">{a.name}</span>
                        )}
                        {stateLabel && (
                          <span
                            className="manual-modal__list-name"
                            style={{ color: stateColor, marginLeft: "auto", fontSize: "0.8em" }}
                          >
                            {stateLabel}
                          </span>
                        )}
                      </button>
                    );
                  })}
                  {filtered.length === 0 && (
                    <div className="manual-modal__empty">
                      Kein Aircraft passt zu „{search}".
                    </div>
                  )}
                </div>
              </>
            )}
            {error && <div className="manual-modal__error">{error}</div>}
            <div className="manual-modal__actions">
              <button type="button" className="button" onClick={onClose}>
                Abbrechen
              </button>
              <button
                type="button"
                className="button button--primary"
                disabled={!selected}
                onClick={proceedToPlan}
              >
                Weiter →
              </button>
            </div>
          </div>
        )}

        {(stage === "plan" || stage === "submitting") && selected && (
          <div className="manual-modal__body">
            <div className="manual-modal__section-title">
              2. Flugplanung
            </div>
            <div style={{ marginBottom: 12, padding: "8px 10px", background: "rgba(103,232,249,0.08)", borderLeft: "3px solid #67e8f9", borderRadius: 4, fontSize: "0.85rem" }}>
              <strong>{selected.icao} {selected.registration}</strong>
              {selected.name && selected.name !== selected.icao && <> — {selected.name}</>}
            </div>

            <div className="manual-modal__form">
              <label>
                <span>Block-Fuel <strong style={{ color: "#fbbf24" }}>*</strong></span>
                <div className="manual-modal__input-with-unit">
                  <input
                    type="number"
                    min="0"
                    step="1"
                    value={blockFuelKg}
                    onChange={(e) => setBlockFuelKg(e.target.value)}
                    placeholder="z.B. 250"
                    disabled={stage === "submitting"}
                  />
                  <span>kg</span>
                </div>
                <small>Wieviel Sprit hast du getankt? Pflicht für Fuel-Score.</small>
              </label>

              <label>
                <span>Erwartete Flugzeit <strong style={{ color: "#fbbf24" }}>*</strong></span>
                <div className="manual-modal__input-with-unit">
                  <input
                    type="number"
                    min="1"
                    step="1"
                    value={flightTimeMin}
                    onChange={(e) => setFlightTimeMin(e.target.value)}
                    placeholder="z.B. 45"
                    disabled={stage === "submitting"}
                  />
                  <span>min</span>
                </div>
                <small>Geschätzte Gesamtzeit für ETA-Anzeige.</small>
              </label>

              <label>
                <span>Cruise-Level <span style={{ color: "var(--fg-dim)" }}>(optional)</span></span>
                <div className="manual-modal__input-with-unit">
                  <input
                    type="number"
                    min="0"
                    step="500"
                    value={cruiseLevel}
                    onChange={(e) => setCruiseLevel(e.target.value)}
                    placeholder="z.B. 4500"
                    disabled={stage === "submitting"}
                  />
                  <span>ft</span>
                </div>
                <small>VFR typisch 2000-9500 ft.</small>
              </label>

              <label>
                <span>Route <span style={{ color: "var(--fg-dim)" }}>(optional)</span></span>
                <input
                  type="text"
                  value={route}
                  onChange={(e) => setRoute(e.target.value)}
                  placeholder="z.B. direct, oder LMV - VFR-N"
                  disabled={stage === "submitting"}
                />
                <small>Free-text Beschreibung der geplanten Route.</small>
              </label>

              <label>
                <span>Alternate <span style={{ color: "var(--fg-dim)" }}>(optional)</span></span>
                <input
                  type="text"
                  value={altAirport}
                  onChange={(e) => setAltAirport(e.target.value.toUpperCase())}
                  placeholder="ICAO, z.B. EDDF"
                  maxLength={4}
                  disabled={stage === "submitting"}
                />
                <small>Ausweich-Flughafen.</small>
              </label>

              <label>
                <span>ZFW (Zero Fuel Weight) <span style={{ color: "var(--fg-dim)" }}>(optional)</span></span>
                <div className="manual-modal__input-with-unit">
                  <input
                    type="number"
                    min="0"
                    step="10"
                    value={zfwKg}
                    onChange={(e) => setZfwKg(e.target.value)}
                    placeholder="z.B. 800"
                    disabled={stage === "submitting"}
                  />
                  <span>kg</span>
                </div>
                <small>Aircraft + Pilot + Pax + Cargo (ohne Sprit).</small>
              </label>
            </div>

            {error && <div className="manual-modal__error">{error}</div>}

            <div className="manual-modal__actions">
              <button
                type="button"
                className="button"
                onClick={() => setStage("aircraft")}
                disabled={stage === "submitting"}
              >
                ← Zurück
              </button>
              <button
                type="button"
                className="button button--primary"
                onClick={() => void submit()}
                disabled={stage === "submitting"}
              >
                {stage === "submitting" ? "Starte…" : "🛩 Flug starten"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

# OFP-Refresh waehrend Boarding — Stand-Aufnahme + Spec

**Status:** Draft v1.1 nach Thomas-Review
**Stand:** 2026-05-11
**Trigger:** Real-Pilot-Frust beim laufenden Flug (Tab "Meine Fluege" → "Aktualisieren" tut nicht was Pilot erwartet)

> **Problem in einem Satz:** Pilot regeneriert SimBrief-OFP waehrend Boarding, klickt "Aktualisieren" im Bid-Tab — die `planned_*`-Werte im aktiven Flug bleiben aber alt, weil dieser Refresh-Pfad den aktiven Flug nicht anpackt.

---

## 1. Datenfluss heute (verifiziert im Code v0.7.6)

```
SimBrief.com                phpVMS (PAX Studio)        AeroACARS Client
─────────────               ──────────────────         ──────────────────
Pilot regeneriert  ──┐                                          
OFP                  │                                          
                     │                                          
                     └─→ User klickt "Laden von SB"             
                         in PAX Studio                          
                              │                                 
                              ▼                                 
                         Bid.simbrief.id wird auf neue          
                         OFP-ID gesetzt  (phpVMS-Bid-DB)        
                                                                
                                          ┌──── /api/user/bids ──┘
                                          │                     
                                          ▼                     
                                   Bid-Liste mit neuer
                                   simbrief.id im Client cache
                                                                
                                          │                     
                                          ▼                     
SimBrief direkt   ←─────  GET  https://www.simbrief.com/
(public-by-ID)             ofp/flightplans/xml/{id}.xml
                                          │                     
                                          ▼                     
                                   SimBriefOfp parsed →
                                   planned_block_fuel_kg
                                   planned_burn_kg
                                   planned_zfw_kg
                                   planned_tow_kg
                                   planned_ldw_kg
                                   etc.
```

**Wichtig:** `simbrief.com/ofp/flightplans/xml/{id}.xml` ist die einzige Quelle fuer die `planned_*`-Werte im Client. phpVMS speichert NICHT die OFP-Werte selbst — phpVMS speichert nur die `simbrief.id` (= Pointer zum OFP auf SimBrief-Seite). Wenn `simbrief.id` neu ist, kommt der frische Plan; wenn alt, der alte.

---

## 2. Drei Refresh-Pfade im Client (Stand v0.7.6)

| Wo | Funktion | Was wird gemacht | Sichtbar wann |
|---|---|---|---|
| **Tab "Meine Fluege"** Header-Button "⟳ Aktualisieren" | `BidsList.handleRefresh` | `phpvms_get_bids` + `sim_force_resync` + `phpvms_refresh_profile` | immer |
| **Cockpit-Tab** "OFP refreshen"-Button (kleines Action-Row) | `ActiveFlightPanel.handleRefreshOfp` → `flight_refresh_simbrief` | re-fetch bids + neuer OFP vom Bid + UEBERSCHREIBT `planned_*` im aktiven Flug | nur `preflight\|boarding\|taxi_out` (siehe v1.1-Korrektur in §6) |
| **Loadsheet-Card** Inline-Refresh-Button (v0.5.46 Adrian-Fix) | `LoadsheetMonitor.handleRefreshOfp` → `flight_refresh_simbrief` | identisch zu (2) | nur `preflight\|boarding` UND wenn OFP-Outdated-Heuristik triggert |

**Kern-Erkenntnis:** Nur (2) und (3) aktualisieren wirklich die `planned_*`-Werte im aktiven Flug. (1) — der prominente Button im Bid-Tab — tut das NICHT.

---

## 3. Real-Pilot-Workflow vs Tool-Reaktion

| Schritt | Pilot tut | AeroACARS-Reaktion | Erwartung |
|---|---|---|---|
| 1 | bookt Bid in phpVMS | — | — |
| 2 | regeneriert OFP auf simbrief.com | — | — |
| 3 | startet AeroACARS, klickt "Flug starten" | `flight_start` → `fetch_simbrief_ofp(sb.id)` → schreibt `planned_*` in FlightStats | ✓ |
| 4 | belaedt im Sim Pax/Cargo/Fuel | — | — |
| 5 | merkt: OFP-Werte passen nicht | — | — |
| 6 | aendert auf simbrief.com → neuer OFP | — | — |
| 7 | klickt **PAX Studio "Laden von SB"** auf phpVMS-Site | phpVMS-Bid bekommt neue `simbrief.id` (server-side) | ✓ |
| 8 | klickt **AeroACARS "⟳ Aktualisieren"** im Bid-Tab | `phpvms_get_bids` zieht neue Bid-Liste (mit neuer `simbrief.id`), aber **`planned_*` im aktiven Flug bleiben alt** | ❌ Pilot erwartet aktualisierten OFP |
| 9 | sieht: Loadsheet-Werte sind weiter falsch | — | (frustrierter Pilot) |
| 10 | **wenn Glueck**: findet Cockpit-Refresh-Button oder Loadsheet-Inline-Refresh-Button | `flight_refresh_simbrief` zieht neue OFP → `planned_*` ueberschrieben | ✓ |

**Ergebnis:** Der prominente Button im Tab "Meine Fluege" (= dort wo der Pilot zuerst schaut) macht NICHT was er erwartet, und der wirksame Button ist in einem anderen Tab versteckt.

---

## 4. Code-Anchors (Stand v0.7.6)

| Datei | Zeile | Was |
|---|---|---|
| `client/src/components/BidsList.tsx` | 240-258 | `handleRefresh()` — der "falsche" Button |
| `client/src/components/ActiveFlightPanel.tsx` | 138-155 | `handleRefreshOfp()` — wirksam, aber versteckt |
| `client/src/components/ActiveFlightPanel.tsx` | 249-266 | Phase-Gate `preflight\|boarding\|taxi_out` — **fehlt `pushback`** (siehe v1.1 §6) |
| `client/src/components/LoadsheetMonitor.tsx` | 102-122 | Inline-Refresh + OFP-Outdated-Heuristik |
| `client/src/components/LoadsheetMonitor.tsx` | 76-93 | Heuristik fuel-delta >= 400 kg OR >= 5% AND zfw-delta < 200 kg |
| `client/src-tauri/src/lib.rs` | 4327-4427 | `flight_refresh_simbrief` Command |
| `client/src-tauri/src/lib.rs` | 1549 | `FlightStats.flight_plan_source` (existiert) — **kein `simbrief_ofp_id` Feld** (siehe v1.1 §6 P2-Fix) |
| `client/src-tauri/src/lib.rs` | 4400 | `stats.flight_plan_source = Some("simbrief")` |
| `client/src-tauri/crates/api-client/src/lib.rs` | 1146-1177 | `fetch_simbrief_ofp` — gibt `Ok(None)` bei Netzwerk-/HTTP-Fehler (siehe v1.1 §6 Punkt 4) |
| `client/src-tauri/crates/sim-core/src/lib.rs` | 677-703 | FlightPhase enum — `Pushback` ist zwischen `Boarding` und `TaxiOut` |

---

## 5. Mutmassliche Wurzeln (priorisiert)

### W1 — UI-Discoverability (Haupt-Wurzel)
Pilot drueckt im Tab "Meine Fluege" auf "Aktualisieren" und erwartet "alles wird neu gezogen", inklusive aktivem Flug. Der Button macht aber nur Bid-Liste + Sim-Resync + Profile.

### W2 — Phase-Gate-Inkonsistenz (klar)
Cockpit-Button gated heute auf `preflight | boarding | taxi_out`. **`Pushback` fehlt** — dort verschwindet der Button obwohl die Phase noch pre-takeoff ist und der Plan noch nutzbar ueberschrieben werden koennte (Loadsheet sieht den Plan, der Touchdown-Score noch nicht). Bewusste Entscheidung notwendig.

### W3 — Cache-Layer? (unwahrscheinlich)
SimBrief antwortet auf jede ID frisch. phpVMS-Cache wuerde nur greifen wenn paxstudio das so konfiguriert hat (VA-spezifisch).

### W4 — PAX Studio "Laden von SB" updated nicht die OFP-ID am Bid? (extern verifizieren)
Wenn das PAX-Studio-Modul nur Pax/Cargo aktualisiert aber NICHT die `simbrief.id` austauscht, dann ist auch `flight_refresh_simbrief` machtlos. Diese Wurzel sitzt server-side. **Entscheidung Thomas v1.1:** nicht als Blocker fuer P1 — P2 macht das fuer den Pilot sichtbar.

**Quick-Check fuer User:** Nach "Laden von SB" einmal `https://german-sky-group.eu/api/user/bids` aufrufen (Browser, eingeloggt) und schauen ob `simbrief.id` wirklich die neue ist.

---

## 6. v1.1-Refinement nach Thomas-Review

### 6.1 P2 unterschaetzt — Persistenz-Feld noetig

`FlightStats` traegt heute kein `simbrief_ofp_id` und kein `simbrief_ofp_generated_at` — nur `flight_plan_source = Some("simbrief")` als Marker (lib.rs:1549). Fuer "OFP unveraendert"-Feedback (Toast) brauchen wir den alten Wert UND den neuen zum Vergleich.

**Erweiterung der Spec:**

```rust
// FlightStats (lib.rs ~1549)
flight_plan_source: Option<&'static str>,
// NEU:
simbrief_ofp_id: Option<String>,         // "1777622821_5F3E3B3842"
simbrief_ofp_generated_at: Option<DateTime<Utc>>, // aus OFP <times><sched_out>?
```

Plus in `storage::PersistedFlightStats` (storage/src/lib.rs) das gleiche Paar mit `#[serde(default)]` damit alte Persistenz lesbar bleibt.

`flight_start` setzt beide. `flight_refresh_simbrief` liest den alten Wert, vergleicht mit dem neuen aus dem frisch geholten OFP, und gibt das Ergebnis im Result-DTO mit zurueck:

```rust
pub struct SimBriefOfpDto {
    // bestehende Felder...
    pub previous_ofp_id: Option<String>, // None bei v0.7.5-Persistenz (alte PIREPs)
    pub current_ofp_id: String,
    pub changed: bool, // current != previous
}
```

Frontend nutzt `changed` fuer den Toast.

### 6.2 Phase-Gate inklusive `Pushback`

Heute im Cockpit-Button: `preflight | boarding | taxi_out`. Spec v1.1 entscheidet:

**Gate fuer v0.7.7: `Preflight | Boarding | Pushback | TaxiOut`**

Begruendung:
- `Pushback` ist die Phase wo der Flieger schon Cleared-Pushback hat, aber noch nicht rollt. Plan-Werte sind weiterhin nutzbar fuer Loadsheet-Vergleich und sub_scores.
- Erst `TakeoffRoll` aufwaerts soll der Plan festgenagelt sein (Score-Aggregat).
- Der heutige Gate-Vorschlag im Spec-v1.0 (ohne `Pushback`) war inkonsistent zur Begruendung "bis vor Takeoff".

Backend (`flight_refresh_simbrief`) bekommt den expliziten Gate-Check:

```rust
if !matches!(current_phase,
    FlightPhase::Preflight
        | FlightPhase::Boarding
        | FlightPhase::Pushback
        | FlightPhase::TaxiOut)
{
    return Err(UiError::new(
        "phase_locked",
        "OFP-Refresh ist nur bis vor Takeoff moeglich (Preflight bis TaxiOut)",
    ));
}
```

Frontend-Sichtbarkeit der dedizierten Buttons synchron anpassen (Cockpit + Loadsheet).

### 6.3 P3 ist Erweiterung, nicht neu

`flight_refresh_simbrief` loggt bereits "OFP refreshed" mit Plan-Werten (lib.rs:4404-4412). v1.1 erweitert um:

```rust
log_activity(
    &state,
    ActivityLevel::Info,
    if changed { "OFP refreshed" } else { "OFP unchanged" }.to_string(),
    Some(format!(
        "{} → {} ({}). Block {:.0} kg, TOW {:.0} kg, LDW {:.0} kg",
        previous_ofp_id.as_deref().unwrap_or("—"),
        current_ofp_id,
        if changed { "neu" } else { "identisch" },
        ofp.planned_block_fuel_kg, ofp.planned_tow_kg, ofp.planned_ldw_kg
    )),
);
```

Damit der JSONL-Audit-Trail im Replay sichtbar macht "ja, der OFP wurde refresht zur Zeit X, alt → neu".

### 6.4 Fehlersemantik in `fetch_simbrief_ofp` schaerfen

Heute (api-client/lib.rs:1146-1177):
- Netzwerk-Fehler → `Ok(None)`
- Non-2xx HTTP → `Ok(None)`
- Body-Read-Fehler → `Ok(None)`
- Erfolg, aber Parse fehlgeschlagen → `Ok(None)` (via `parse_simbrief_ofp`)

Caller (`flight_refresh_simbrief`) sieht nur `Ok(None)` → wirft `ofp_unusable`. Pilot kann nicht unterscheiden ob:
- SimBrief offline
- OFP-ID existiert nicht / wurde geloescht
- OFP existiert, aber XML hat unerwartetes Format

**Erweiterung:**

```rust
pub enum SimBriefFetchError {
    Network(reqwest::Error),    // → Toast: "SimBrief nicht erreichbar"
    HttpStatus(StatusCode),     // 404 → "OFP-ID nicht gefunden", 5xx → "SimBrief-Fehler"
    BodyRead,                   // selten — Netzwerk-Abbruch nach Header
    ParseFailed,                // → "OFP-Format unbekannt"
    OfpUnusable,                // Plan-Werte 0/negativ → "OFP unvollstaendig"
}

pub async fn fetch_simbrief_ofp(
    &self, ofp_id: &str,
) -> Result<SimBriefOfp, SimBriefFetchError> { ... }
```

`flight_refresh_simbrief` mappt die Variante auf passende `UiError`-Codes/Strings → Toast hilft jetzt wirklich.

**Aufwand-Hinweis:** Diese Aenderung beruehrt auch `flight_start` und `fetch_simbrief_preview` (alle drei Caller). Migration: Caller die heute `Ok(None)` toleriert haben mappen `Err(SimBriefFetchError::*)` auf `Ok(None)` (keine Regression), neuer Caller (`flight_refresh_simbrief`) nutzt die Varianten differenziert.

### 6.5 UI-Update sofort nach Refresh

`flight_status` wird vom Cockpit-Tab 2-sekuendlich gepollt → Loadsheet sieht den neuen Plan spaetestens 2s nach Refresh. Im Bid-Tab haengt das Update vom 15s-Bid-Poll (im Boarding pausiert!) ab — also potenziell ueberhaupt nicht ohne weiteres Trigger.

**Loesung:** `flight_refresh_simbrief` emit-t am Ende ein `flight-status-update`-Event (oder benutzt den bestehenden Status-Refresh-Mechanismus). Bid-Tab horcht NICHT auf flight_status — also entweder:
- (a) BidsList.handleRefresh ruft nach erfolgreichem `flight_refresh_simbrief` ein `invoke("flight_status")` und propagiert die neuen Werte (= Parent-Component aktualisiert sich)
- (b) Backend emit-t `app.emit("flight-status-changed", ...)` nach jedem `flight_refresh_simbrief` und alle Listener (Cockpit + Loadsheet + ggf. BidsList) bekommen die Aenderung.

Variante (b) ist sauberer aber groesserer Eingriff. v0.7.7 macht (a) — Bid-Tab triggert einen Status-Refresh als Teil seiner Refresh-Chain.

### 6.6 Aufwand-Korrektur

Spec v1.0 schaetzte ~30 Zeilen Frontend + ~10 Backend. Mit allen v1.1-Aenderungen realistisch:

| Punkt | LOC-Schaetzung |
|---|---|
| P1 (Bid-Tab calls flight_refresh_simbrief) | ~15 Frontend |
| Phase-Gate-Backend + Frontend-Sync | ~10 Backend + ~5 Frontend |
| Persistenz-Feld `simbrief_ofp_id` | ~5 Stats + ~5 PersistedStats + ~10 set/get-Sites |
| Result-DTO Erweiterung `previous_ofp_id` + `changed` | ~10 |
| Toast-Wording + i18n (DE+EN, evtl. IT) | ~15 |
| Activity-Log Erweiterung | ~10 |
| `fetch_simbrief_ofp` Result-Typ-Refactor | ~40 (incl. 3 Caller-Anpassungen) |
| UI-Update-Trigger nach Refresh | ~10 |
| Tests | ~50 |

**Geschaetzt: 150-200 LOC Diff** ueber 5-6 Files. "Kleiner Patch, aber nicht 40 LOC."

Falls v0.7.7-Schnitt zu gross wird: §6.4 (Result-Typ) kann auf v0.7.8 verschoben werden — der `ofp_unusable`-Fall ist heute nicht so haeufig dass die Praezisierung Tag-relevant ist.

---

## 7. Soll-Verhalten (Spec)

### Was wir wollen
1. **"Aktualisieren" im Bid-Tab macht das was es heisst** — inkl. aktiver Flug-OFP, wenn vorhanden und in refreshbarer Phase.
2. **Discoverability:** Pilot soll nicht zwischen Tabs wechseln muessen.
3. **Klarer Feedback-Loop:** wenn der OFP-Refresh KEINE Aenderung gebracht hat, soll der Pilot das wissen — damit er erkennt dass die phpVMS-Seite das Problem ist.
4. **Schnelles UI-Update:** nach erfolgreichem Refresh muss das Loadsheet binnen 1s die neuen Werte zeigen, nicht erst nach 2s `flight_status`-Poll.

### Was wir NICHT tun (in v0.7.7)
- Kein Phase-Limit-Aufweichen nach Takeoff
- Kein neuer Score-Logik-Pfad
- Kein Pax-Studio-Reverse-Engineering
- Kein Auto-Refresh-Polling (Option C — kann spaeter)
- **Kein Architektur-Wechsel zu "SimBrief-direkt-by-username"** (siehe §11 — strategische Option, eigener Schnitt)

---

## 8. Loesungs-Optionen (Detail)

### Option A1 (gewaehlt fuer v0.7.7)

`BidsList.handleRefresh` ruft `flight_refresh_simbrief` zusaetzlich auf, Phase-Gate im Backend, Result-DTO mit `previous_ofp_id`/`changed`.

```ts
async function handleRefresh() {
  if (refreshing) return;
  setRefreshing(true);
  
  const tasks: Promise<unknown>[] = [
    fetchBids(),
    invoke("sim_force_resync").catch(() => null),
    invoke<Profile | null>("phpvms_refresh_profile").catch(() => null),
  ];
  
  let ofpRefreshResult: SimBriefOfpDto | null = null;
  if (hasActiveFlight) {
    tasks.push(
      invoke<SimBriefOfpDto>("flight_refresh_simbrief")
        .then((dto) => { ofpRefreshResult = dto; return dto; })
        .catch((err: { code?: string; message?: string }) => {
          // phase_locked / no_simbrief_link → ignorieren (kein Toast)
          // ofp_fetch_failed → in detailliertere Variante mappen (§6.4)
          if (err?.code && err.code !== "phase_locked" && err.code !== "no_simbrief_link") {
            // Activity-Log oder Toast je nach Variante
          }
          return null;
        }),
    );
  }
  
  const [, , freshProfile] = await Promise.all(tasks);
  if (freshProfile && onProfileRefreshed) onProfileRefreshed(freshProfile);
  
  // v0.7.7: Toast wenn OFP refresht aber unveraendert blieb
  if (ofpRefreshResult && !ofpRefreshResult.changed) {
    showToast(t("bids.ofp_unchanged", {
      id: ofpRefreshResult.current_ofp_id,
    }));
  }
  
  // §6.5: Status-Refresh triggern damit Cockpit + Loadsheet sofort
  // die neuen Werte sehen, nicht erst nach 2s-Poll
  if (ofpRefreshResult?.changed) {
    onActiveFlightUpdated?.();
  }
  
  setTimeout(() => setRefreshing(false), 400);
}
```

Toast-Wording (v1.1 final):

```
"OFP unveraendert. phpVMS meldet weiterhin OFP-ID {id}. Bitte PAX
Studio 'Laden von SB' pruefen."
```

EN:
```
"OFP unchanged. phpVMS still reports OFP ID {id}. Check PAX Studio
'Load from SB'."
```

### Option B-erweitert (Toast wenn unveraendert) — siehe Option A1 oben, ist da integriert

### Option C (Auto-Refresh-Polling) — auf spaeter verschoben, eigener Schnitt

---

## 9. Entscheidungen aus Thomas-Review v1.1

| Punkt | Entscheidung |
|---|---|
| **W4** (PAX Studio updated `simbrief.id`?) | extern verifizieren, aber **nicht als Blocker fuer P1**. P2 macht es im Toast sichtbar. |
| **Phase-Gate** | `Preflight \| Boarding \| Pushback \| TaxiOut` (inkl. Pushback) — Begruendung "bis Takeoff" |
| **Toast-Wording** | `OFP unveraendert. phpVMS meldet weiterhin OFP-ID {id}. Bitte PAX Studio "Laden von SB" pruefen.` |
| **v0.7.7-Scope** | P1 (Bid-Tab-Refresh) + Backend-Gate + Persistenz-Feld + unveraendert-Feedback (Toast) + UI-Update-Trigger |
| **Auto-Refresh** | weiter spaeter (eigene v0.8.x-Diskussion) |
| **Result-Typ-Refactor in `fetch_simbrief_ofp`** | als Stretch-Goal v0.7.7. Kann auf v0.7.8 wenn der Schnitt zu gross wird. |

---

## 10. Test-Vorschlaege

Backend (Rust):
- `flight_refresh_simbrief_returns_phase_locked_after_takeoff`
- `flight_refresh_simbrief_marks_changed_false_when_ofp_id_identical`
- `flight_refresh_simbrief_marks_changed_true_when_ofp_id_new`
- `flight_stats_persists_simbrief_ofp_id_across_save_load`
- (falls Result-Typ-Refactor in v0.7.7): `simbrief_fetch_maps_404_to_not_found_variant`, `simbrief_fetch_maps_network_to_unreachable`

Frontend (manuell oder per Playwright-Smoke):
- Bid-Tab "Aktualisieren" im Boarding bei neuer OFP-ID → Loadsheet zeigt neue Werte ohne Tab-Wechsel
- Bid-Tab "Aktualisieren" im Boarding bei UNVERAENDERTER OFP-ID → Toast erscheint
- Bid-Tab "Aktualisieren" im Cruise → kein Crash, Bid-Liste wird trotzdem aktualisiert (`phase_locked` still ignoriert)

---

## 11. STRATEGISCHE OPTION: "SimBrief-direkt, PAX Studio raus"

Thomas-Vorschlag: *"Ich waere auch immer dafuer die frischen bzw die Daten immer von SB zu holen und Pax Studio raus zu lassen"*

### Was das heisst

Heute haengt AeroACARS an der `simbrief.id`, die phpVMS (PAX Studio) am Bid hinterlegt. Ein direkter Pfad waere:

```
Pilot regeneriert OFP auf simbrief.com
        ↓
AeroACARS fragt SimBrief direkt: "letzter OFP fuer User X?"
        ↓
SimBrief liefert latest OFP (incl. dpt/arr/callsign/etc.)
        ↓
AeroACARS verifiziert: passt dpt/arr/callsign zum aktiven AeroACARS-Flug?
        ↓ (ja)
AeroACARS uebernimmt OFP-Werte
        ↓
(phpVMS-Bid bleibt unberuehrt; nur fuer flight_number-Lookup beim Start verwendet)
```

SimBrief-API-Endpoint dafuer: `GET https://www.simbrief.com/api/xml.fetcher.php?username={username}` — gibt den **letzten** OFP fuer den User zurueck.

### Aufruf-Modell-Vergleich

| Aspekt | Heute (phpVMS-Pointer) | SimBrief-direkt |
|---|---|---|
| Abhaengigkeit von PAX Studio | hoch (Pointer-Update noetig) | gar nicht |
| Pilot-Workflow | regenerate + "Laden von SB" + AeroACARS-Refresh | nur regenerate + AeroACARS-Refresh |
| Erforderliche Pilot-Konfig | nichts (PAX Studio kennt SimBrief-User) | SimBrief-Username einmalig in AeroACARS-Settings |
| Failure-Mode | Pointer outdated → alter Plan | Pilot hat anderen OFP zwischendurch generiert (z.B. fuer einen anderen Flug) → AeroACARS muss flight-match verifizieren |
| Bid-Pax/Cargo-Zahlen | aus PAX-Studio-Subfleet via Bid | weiter aus phpVMS-Bid noetig (SimBrief OFP traegt keinen "echten" Bid-Pax-Stand) |
| VFR/Manual-Flights ohne OFP | unveraendert (kein OFP, kein Refresh) | unveraendert |

### Was waere noetig

1. **Pilot-Settings:** Feld "SimBrief-Username" (oder API-Key — SimBrief hat beides als Auth-Optionen). Speichern in der bestehenden `Settings`-Struktur.
2. **`fetch_simbrief_ofp_latest`-Command:** holt `xml.fetcher.php?username=X`, parsed gleich wie `fetch_simbrief_ofp` (mit zusaetzlichen Headern wie `<origin>`, `<destination>`, `<callsign>` fuer Verifikation).
3. **Flight-Match-Verifikation:** SimBrief-OFP.origin == AeroACARS-flight.dpt UND SimBrief-OFP.destination == AeroACARS-flight.arr UND (optional) callsign passt. Wenn Mismatch → Toast "SimBrief-OFP gehoert nicht zum aktiven Flug ({X} → {Y}), bitte regenerieren oder PAX-Studio-Fallback nutzen".
4. **Fallback-Logik:** wenn kein SimBrief-Username konfiguriert ODER kein passender OFP latest → bestehender phpVMS-Pointer-Pfad als Fallback.

### Pro / Contra

**Pro:**
- Kein PAX-Studio-Sync-Frust mehr
- Workflow fuer Pilot kuerzer
- AeroACARS waere weniger gekoppelt an phpVMS-Modul-Implementations
- Bei W4 (PAX Studio updated nicht) gar kein Symptom mehr

**Contra:**
- Pilot muss einmalig SimBrief-Username eingeben — Friction fuer Erst-User
- Flight-Match-Verifikation fragt: was wenn Pilot zwischen Bid-Start und OFP-Refresh einen OFP fuer einen anderen Flug generiert hat? Mismatch-Toast verwirrt evtl.
- Bid-Pax/Cargo (Subfleet) braucht weiter phpVMS — wir koennen PAX Studio nicht komplett rausnehmen, nur bei der OFP-ID-Quelle
- VAs ohne PAX Studio (manche andere phpVMS-Themes) bekommen heute schon den phpVMS-Pointer-Pfad — bei SimBrief-direkt muessten wir Default-Fallback dokumentieren

### Empfehlung der Spec

**v0.7.7:** P1+P2+P3 wie in §6 — adressiert den akuten Pilot-Frust ohne Architektur-Wechsel.

**v0.8.x (eigene Diskussion + Spec):** SimBrief-direkt als zusaetzlicher Pfad mit Settings-Toggle "Quelle: PAX-Studio-Pointer / SimBrief-direkt / Auto (SimBrief mit phpVMS-Fallback)". Default `Auto`, sodass:
- Pilot mit SimBrief-Username konfiguriert → SimBrief-direkt zuerst, phpVMS-Fallback bei Mismatch
- Pilot ohne SimBrief-Username → wie heute (phpVMS-Pointer)

Damit beide Workflows nebeneinander leben koennen, ohne dass irgendein VA-Setup brechen wird.

---

## 12. Versionierung dieser Spec

- **v1.0 (2026-05-11):** Initial Stand-Aufnahme + Loesungs-Optionen
- **v1.1 (2026-05-11):** Refinement nach Thomas-Review:
  - §6.1 P2 Persistenz-Feld `simbrief_ofp_id` + `_generated_at` ergaenzt (war Unter-Punkt, jetzt Erstklassig)
  - §6.2 Phase-Gate auf `Preflight | Boarding | Pushback | TaxiOut` korrigiert (Pushback war vorher implizit ausgeschlossen)
  - §6.3 P3 Audit-Trail als Erweiterung des bestehenden "OFP refreshed"-Logs umformuliert
  - §6.4 Fehlersemantik in `fetch_simbrief_ofp` — Vorschlag fuer Result-Typ-Refactor (`Ok(None)` → spezifische Error-Varianten)
  - §6.5 UI-Update-Trigger nach Refresh damit Bid-Tab-Klick nicht 2s-Status-Poll abwartet
  - §6.6 Aufwand-Korrektur: 150-200 LOC realistisch, nicht 30+10
  - §9 4 Entscheidungen aus Thomas-Review festgehalten
  - §10 Tests-Vorschlaege konkretisiert
  - **§11 NEU:** Strategische Option "SimBrief-direkt, PAX Studio raus" dokumentiert — Pro/Contra, Settings-Modell, Fallback-Logik. Empfohlen als v0.8.x mit eigener Spec.

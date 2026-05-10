# Flight-Phase State-Machine — QS-Inventur fuer Bug-Untersuchung

**Status:** v1.0 — **Draft for QS Review** (Stand AeroACARS v0.7.4)
**Zweck:** Vollstaendige Inventur aller Phase-Wechsel + Trigger + Side-Effects + Anti-Flicker-Mechaniken. Damit kann VA-Owner / QS systematisch durchgehen und potenzielle Bug-Klassen finden bevor sie als Live-Bug auftauchen.
**KEIN Implementierungs-Auftrag** — diese Spec dokumentiert NUR den Status-Quo + markiert Verdachtsstellen.

---

## 0. Warum dieses Dokument

Die Phase-State-Machine in `lib.rs::step_flight` (~600 Zeilen) ist ueber Monate gewachsen und hat zwischenzeitlich mehrere Live-Bugs produziert (PMDG-B738 53819ft AGL-Glitch, GSX-Repositioning-Trigger, MSFS-Pause-Race etc.). Jede Korrektur hat eine Anti-Flicker-Schutzschicht hinzugefuegt — aber niemand hat den Gesamt-Zustand systematisch dokumentiert.

Diese Spec ist die Antwort. Pro Transition: was triggert sie, welche Schwellen, welche Anti-Flicker-Mechaniken sind aktiv, welche Side-Effects passieren. Plus eine Verdachts-Liste (markiert mit **[VERDACHT]**) mit Stellen die im Code-Audit verdaechtig wirkten.

---

## 1. Phase-Enum (sim-core)

`crates/sim-core/src/lib.rs:677` — 16 Varianten in striktem chronologischen Default-Pfad:

```
Preflight → Boarding → Pushback → TaxiOut → TakeoffRoll → Takeoff
   → Climb → Cruise → Descent → Approach → Final → Landing
   → TaxiIn → BlocksOn → Arrived → PirepSubmitted
```

Plus eine **Holding-Variante** die laut Doc-Kommentar (sim-core:687-693) zwischen Cruise/Approach erkannt werden soll.

### 1.1 [VERDACHT] Holding-Phase im Code unbenutzt?

Code-Inventur zeigt: `FlightPhase::Holding` ist im Enum definiert + im phpVMS-Mapping vorhanden, aber **kein Transition-Pfad in `step_flight` setzt aktuell `phase = Holding`**. Der Doc-Kommentar verspricht Detection ueber sustained bank > 15° + |VS| < 200 fpm > 90s — diese Logik ist nicht implementiert.

→ Pilot-relevant: Holding wird im Web/Monitor nie sichtbar, Holding-Zeit wird nicht erfasst. Kein Show-Stopper, aber Doku<>Code-Drift.

---

## 2. Hauptfunktion `step_flight`

`lib.rs:6289` — wird vom Streamer-Tick (5-30 s je nach Phase) aufgerufen. Reihenfolge in einem Tick:

1. Anti-Flicker-State refreshen (Engines, Pushback)
2. Distance-Accounting (`distance_nm` += Haversine)
3. Position-Counter, last_lat/lon, fuel-Tracking
4. Block-Fuel-Peak-Tracker (mit Defuel-Erkennung > 200 kg sudden drop)
5. Peak-Altitude-Tracker
6. **`was_airborne`-Flag-Tracking** (3-Schicht-Verteidigung — siehe §6.1)
7. Pro aktueller `stats.phase`: passende Transition pruefen → `next_phase`
8. Wenn `next_phase != stats.phase`: Side-Effects ausloesen, `phase = next_phase`, `record_event(PhaseChanged)`

---

## 3. Transition-Tabelle

Pro Phase: aktueller Trigger + Schwellen + bekannte Anti-Flicker.

| Von | Nach | Trigger | Schwellen | Anti-Flicker |
|---|---|---|---|---|
| **Preflight** | Boarding | Auto bei flight_start (kein Sim-Check) | — | — |
| **Boarding** | Pushback | `on_ground && groundspeed > 0.5 kt && engines == 0` | 0.5 kt | — |
| **Boarding** | TaxiOut | `on_ground && groundspeed > 0.5 kt && engines > 0` | 0.5 kt | — |
| **Pushback** | TaxiOut | `tug_done (pushback_state==3) ODER powered_taxi (engines>0 && gs>3 kt)` nach DWELL | PUSHBACK_DWELL_SECS=10 | 10 s Dwell |
| **TaxiOut** | TakeoffRoll | `on_ground && gs > 30 kt && engines > 0` | 30 kt | — |
| **TakeoffRoll** | Takeoff | `was_on_ground && !on_ground` (Edge!) + setzt `takeoff_at` | on_ground-Edge | — |
| **Takeoff** | Climb | `agl > 500 ft` | 500 ft AGL | — |
| **Climb** | Cruise | `\|VS\| < 200 fpm && agl > 5000 ft` | 200 fpm + 5000 ft | — |
| **Climb** | Descent | `VS < -500 fpm` | 500 fpm | **[VERDACHT]** kein Dwell |
| **Cruise** | Descent | `VS < -500 fpm && lost_alt > 5000 ft` | 5000 ft Drop, 500 fpm | Lost-Alt-Schutz gegen Step-Down |
| **Descent** | Approach | `agl < 5000 ft && VS < 0` | 5000 ft AGL | — |
| **Approach** | Final | `agl < 700 ft` | 700 ft AGL | — |
| **Approach/Final** | Climb (Go-Around) | `agl > lowest_seen + 200 ft && VS > 500 fpm` (8s Dwell) | GO_AROUND_AGL_RECOVERY_FT=200, GO_AROUND_MIN_VS_FPM=500 | 8 s Dwell, "Lowest-AGL"-Tracker |
| **Final** | Landing | `!was_on_ground && on_ground` (Edge vom 50Hz-Sampler) + setzt `landing_at` | on_ground-Edge | Sampler-Validation (Forensik v2) |
| **Landing** | Climb (Touch-and-Go) | `agl > 100 ft && !on_ground && engines > 0` fuer 1 s | 100 ft AGL, 1 s Dwell | Reset Landing-Window |
| **Landing** | TaxiIn | `gs < 30 kt && on_ground` | 30 kt | — |
| **TaxiIn** | BlocksOn | `parking_brake && gs < 1 kt && on_ground` | 1 kt | — |
| **BlocksOn** | Arrived | `engines == 0 && parking_brake && on_ground && (now - block_on) >= 30s` | ARRIVED_DWELL=30s | 30 s Dwell |
| **(jede)** | Arrived (Universal-Fallback) | `was_airborne && on_ground && engines == 0 && stationary >= 30s` | ARRIVED_FALLBACK_DWELL=30s | `was_airborne`-Gate (siehe §6.1) |
| **Arrived** | PirepSubmitted | Manuell via `flight_file` Tauri-Command | — | — |

### 3.1 [VERDACHT] Cruise-Threshold zu permissiv

`Climb → Cruise` triggert bei `|VS| < 200 fpm`. Im Level-off mit Autopilot-Trim kann das kurz wackeln (Pilot fliegt FL340 manuell aus, AP greift wieder). Dwell-Schutz fehlt — Phase koennte zu frueh als Cruise reportet werden, bevor das Aircraft wirklich stabil ist.

**Mitigation in der Praxis:** der Trigger braucht zusaetzlich `agl > 5000 ft` — wenn der Pilot bei FL050 noch climbed verhindert das den Cruise-Switch. Bei realen Cruise-Hoehen (FL250+) ist das Wacken-Problem selten kritisch. **Trotzdem markiert** weil keine systematische Dwell-Logik wie bei anderen Transitions.

### 3.2 [VERDACHT] Climb → Descent ohne Dwell

`Climb → Descent` triggert bei `VS < -500 fpm`. Wenn ein Climb durch Turbulenz / kurzen Push-Down kurzzeitig auf -600 fpm einbricht, wird die Phase auf Descent gewechselt und kann nicht zurueck (kein "Descent → Climb"-Pfad). Pilot muesste durch Cruise zurueck.

**Mitigation in der Praxis:** -500 fpm waehrend Climb ist sehr unueblich (selbst Turbulenz kommt selten auf -500 fpm). Aber moeglich. Spec-Empfehlung: Dwell von 30-60 s einbauen.

### 3.3 [VERDACHT] Cruise → Descent Lost-Alt-Schutz nur einseitig

`Cruise → Descent` braucht `VS < -500 fpm && lost_alt > 5000 ft` — gut. **ABER**: `Descent → Cruise` existiert nicht. Wenn ein Pilot in Step-Down zwischen FL370 und FL350 cruiset, wird er einmal als Descent klassifiziert und bleibt das bis Approach. Real ist er aber im Cruise.

**Mitigation in der Praxis:** Step-Down ist 2000 ft, der Schutz greift bei 5000 ft → wir sehen erst echten Descent. **OK aktuell.** Aber wenn ein Pilot mehrere Step-Downs ueber 5000 ft Gesamtverlust macht und dann wieder cruiset, bleibt er in Descent.

---

## 4. Special Transitions: Touch-and-Go, Go-Around, Divert

### 4.1 Touch-and-Go

`lib.rs:7118-7184` — nach `Landing` Phase wird gepruft:
- `agl > 100 ft && !on_ground && engines > 0` fuer 1 s Dwell
- → Phase revertiert auf `Climb`, Landing-Window wird zurueckgesetzt
- Touchdown-Event bleibt im `touchdown_events` Vec mit `kind: TouchAndGo`

### 4.2 Go-Around

`lib.rs:1984-2005` (`check_go_around`) — Anti-Flicker-Pattern:
- `lowest_agl_seen` wird waehrend Approach/Final gemerkt
- Trigger: `agl > lowest_seen + GO_AROUND_AGL_RECOVERY_FT (200ft) && VS > GO_AROUND_MIN_VS_FPM (500fpm)` ueber `GO_AROUND_DWELL_SECS (8s)`
- → Phase auf `Climb`, `go_around_count++`

### 4.3 Divert

`lib.rs:7319-7361` — kein eigener Phase-Wechsel sondern eine "Hint":
- Wenn `!near_planned (>=2nm vom geplanten Arrival)` waehrend Landing/TaxiIn
- → `find_nearest_airports()` setzt `stats.divert_hint` mit actual+planned ICAO
- Phase laeuft normal weiter (kein dedizierter Divert-State)
- PIREP-Submit-Pfad behandelt Divert speziell (`update_pirep` mit `arr_airport_id` ueberschrieben)

### 4.4 [VERDACHT] Holding-Detection fehlt

Wie in §1.1 erwaehnt — Code hat keinen Pfad der die Holding-Phase aktiviert. Pilot der 30 Min ueber EDDF holt wird die ganze Zeit als "Cruise" oder "Approach" gefuehrt.

---

## 5. Universal Arrived-Fallback

`lib.rs:7295-7310` — Schutzschicht fuer Faelle wo der normale `BlocksOn → Arrived`-Pfad nicht durchlaeuft (z.B. Pilot vergisst Parking-Brake).

```
Trigger: was_airborne && on_ground && engines == 0 && stationary_dwell >= 30s
       && stats.block_off_at.is_some()
       && pre_block_off == false
       && already_done == false
```

**Lessons Learned:** drei Live-Bugs vor diesem Fallback noetig:
- PMDG-B738 GSX-Repositioning loeste Arrived bei FL538-Glitch aus — Fix: `agl > 30000 ft` blockt `was_airborne`
- Pilot mit kurzer Pause vor Block-Off bekam Arrived — Fix: `block_off_at.is_some()` Pflicht
- Single-Tick-Glitch poisoned `was_airborne` — Fix: 2-Tick-Dwell

---

## 6. Anti-Flicker-Mechaniken

### 6.1 `was_airborne`-Flag (3-Schicht-Verteidigung)

`lib.rs:11008-11022` — sticky Flag, einmal `true` bleibt sie. Setzen erfordert ALLE drei:
1. `agl > WAS_AIRBORNE_AGL_FT (50ft) && agl < WAS_AIRBORNE_AGL_MAX_FT (30000ft)`
2. `block_off_at.is_some()` (zeitlich plausibel)
3. Halten fuer `WAS_AIRBORNE_DWELL_TICKS (2)` Ticks

### 6.2 Engine-Anti-Flicker

`lib.rs:10917-10919` — `last_engines_running_above_zero_at` wird gestempelt jedes Mal wenn Engines > 0. Verschiedene Phase-Logiken nutzen diesen Timestamp um "Engines waren grade noch an" zu pruefen statt nur `engines == 0` (was sekundenweise flippen kann).

### 6.3 Pushback-State-Tracking

`lib.rs:10920-10924` — `saw_pushback_state_active` wird sticky-true wenn `pushback_state != 3`. Verhindert dass kurze Glitches (pushback_state schwankt 0/1/2/3) als "kein Pushback erkannt" durchgehen.

### 6.4 Bounce-Detection

`lib.rs:7070-7099` — separate AGL-Edge-Logik:
- Arm: `agl > 35 ft` nach Touchdown gesehen
- Fire: `agl < 5 ft` gesehen → `bounce_count++`
- Window: 8 s nach Touchdown (BOUNCE_WINDOW_SECS)

---

## 7. Side-Effects pro Transition

| Transition | Was wird geschrieben |
|---|---|
| Boarding → Pushback | `block_off_at = now`, MQTT `Block`-Event |
| TakeoffRoll → Takeoff | `takeoff_at = now`, `takeoff_pitch_deg/bank_deg/fuel_kg/weight_kg` |
| Final → Landing | `landing_at = actual_td_at` (vom 50Hz-Sampler), Touchdown-Window startet |
| Landing → TaxiIn | Landing-Score wird klassifiziert + `landing_score_announced = false` |
| TaxiIn → BlocksOn | `block_on_at = now`, Activity-Log "Block on" |
| BlocksOn → Arrived | Auto-Submit-Hook (wenn aktiviert) |
| Arrived → PirepSubmitted | phpVMS `/file` POST + MQTT `Pirep`-Publish + Discord-Embed + landing_history.json |

Bei jedem Wechsel: `record_event(FlightLogEvent::PhaseChanged { from, to, at })` ins JSONL.

---

## 8. Resume / Pause / Restart

### 8.1 Persistence

`lib.rs` `save_active_flight()` schreibt nach jedem Phase-Wechsel + alle 30 s:
- `<app_data_dir>/active_flight.json` mit `PersistedFlightStats` (alle Felder von FlightStats die Snapshot-relevant sind)
- Inkl. `phase`, `block_off_at`, `takeoff_at`, `bounce_count`, `landing_score`, `forensics_version`, etc.

### 8.2 Restore

`lib.rs:10117-10152` — beim AeroACARS-Start:
- Wenn `active_flight.json` existiert: `PersistedFlightStats.apply_to(&mut FlightStats)` 
- **Phase wird 1:1 restored** — wenn der Pilot z.B. in Cruise war als die App geschlossen wurde, ist sie nach Restart in Cruise

### 8.3 [VERDACHT] Phase-Restore + Sim-Disconnect

Wenn der Pilot AeroACARS schliesst waehrend er in Final ist, dann den Sim schliesst und 30 Min spaeter beides wieder oeffnet — die Phase ist `Final`, aber der Sim ist auf einem ganz anderen Flughafen. Der naechste `step_flight`-Tick wird die Phase aufgrund der neuen Snapshot-Werte normal weiter ausfuehren. Der Wechsel `Final → Landing` setzt einen Timestamp `landing_at` mit dem Sim-Snapshot-Timestamp — aber wenn der Sim "gerade wieder live ist" und der Pilot rein zufaellig auf einer Bahn rollt, wird das eventuell als Landing-Edge erkannt obwohl es nur Sim-Reload ist.

**In der Praxis:** dieser Edge-Case ist selten weil der Pilot meistens `flight_cancel` macht statt einen Final-Flug 30 Min spaeter neu zu starten. **Aber wenn es passiert:** kein expliziter Schutz. Spec-Empfehlung: nach Restore sollte ein "Restore-Tick" erkannt werden und der erste Sim-Snapshot als "ground-truth" geloggt werden bevor weitere Phase-Wechsel erlaubt sind.

---

## 9. phpVMS-Status-Code-Mapping

`lib.rs:8231-8249` — pro Phase ein 3-letter Code laut phpVMS-Doku:

| Phase | Code | Phase | Code |
|---|---|---|---|
| Preflight | INI | Approach | APR |
| Boarding | BST | Final | FIN |
| Pushback | PBT | Landing | LDG |
| TaxiOut | TXI | TaxiIn | TXI |
| TakeoffRoll | TOF | BlocksOn | ONB |
| Takeoff | TKO | Arrived | ARR |
| Climb | ICL | PirepSubmitted | (filed) |
| Cruise/Descent | ENR | | |

### 9.1 [VERDACHT] Cruise und Descent haben den gleichen Code

Beide → "ENR". phpVMS unterscheidet die nicht. Pilot der die Hoehe mehrfach wechselt ist nicht im Web sichtbar verschieden. **Aktuell bewusst** (phpVMS-Doku hat keinen Descent-Code), aber dokumentiert hier als Hinweis.

---

## 10. Bekannte Bug-Klassen — was zu pruefen ist

Aus dem Code-Audit + Doku-Review fallen die folgenden Verdachts-Klassen auf, die QS systematisch durchgehen sollte:

### 10.1 Phase-Race-Conditions

**Verdacht:** Streamer-Tick (5-30s) + Touchdown-Sampler (50Hz) lesen beide aus `flight.stats.lock()`. Wenn der Sampler ein `landing_at` setzt und der Streamer parallel die Phase auf `Final → Landing` switchen will, koennte der Streamer mit `landing_at = None` checken und seinen eigenen `landing_at` schreiben.

**Wo nachschauen:** `lib.rs:6716-6717` (Final → Landing setzt `landing_at`) vs Sampler-Validation-Block (`lib.rs:9051+`). Pruefen ob beide gleichen Wert schreiben oder ob race moeglich ist.

### 10.2 Pause-Resume-Drift

**Verdacht:** §8.3 — Phase wird restored, aber kein Sim-Snapshot-Validation. Pilot der die App nach Final restartet ohne den Sim-Flug zu beenden koennte einen Phantom-Touchdown bekommen.

**Wo nachschauen:** `lib.rs:10117-10152` (apply_to) — gibts irgendwo einen "first-tick-after-restore"-Flag der weitere Phase-Wechsel kurz blockt?

### 10.3 Holding-Detection nicht implementiert

**Verdacht:** §1.1 + §4.4 — Doku verspricht, Code liefert nicht. Pilot der echtes Holding fliegt sieht "Cruise" + Phantom-Distance.

**Wo nachschauen:** Ist die Distance-Akkumulation `distance_nm += haversine` in §2 Schritt 2 ein Problem im Holding (kreist um einen Fix)? Wenn ja, wird die PIREP-Distanz ueberhoeht.

### 10.4 Transition-Nicht-Reversibel

**Verdacht:** §3.2 + §3.3 — `Climb → Descent` und `Descent → Cruise` haben kein Reverse. Pilot der durch Turbulenz kurz negative VS hat wird permanent als Descent klassifiziert.

**Wo nachschauen:** Gibts einen `Descent → Cruise` oder `Descent → Climb`-Pfad? Wenn nein, sollte er hinzugefuegt werden (mit Dwell).

### 10.5 was_airborne Edge-Cases

**Verdacht:** Sticky-Flag bedeutet: einmal `true`, bleibt `true` bis Flight-Ende. Wenn die 3-Schicht-Verteidigung (§6.1) doch durchbricht (z.B. neuer MSFS-Bug der konsistent FL40000 fuer mehrere Sekunden meldet), ist der Schutz weg fuer den Rest des Fluges.

**Wo nachschauen:** Gibt es einen "was_airborne reset" wenn die Bedingungen nicht mehr halten? Per `lib.rs:11008-11022` — wenn `airborne_now == false`, wird `airborne_dwell_ticks = 0` gesetzt aber `was_airborne` selbst bleibt `true`. Das ist die Sticky-Eigenschaft. Bug oder Feature?

### 10.6 Phase-Skip bei sehr schnellen Sim-Snapshots

**Verdacht:** Wenn der Pilot mit Slew-Mode oder Sim-Time-Speed-Up springt, koennte ein Tick gleichzeitig `agl: 30000 → 50` sehen. Phase-Wechsel sind aber linear (Climb → Cruise → Descent → Approach → Final). Bei Sprung wuerde mehrere Ticks dauern bis FSM aufgeholt hat, und Side-Effects (block_off_at etc.) waeren leer.

**Wo nachschauen:** Gibt es einen "phase-skip-detector" der mehrere Wechsel pro Tick erlaubt? Vermutlich nein — Pilot in Slew-Mode bekommt vermutlich einen Phantom-PIREP.

---

## 11. Empfohlene QS-Tests

Aus den Verdachten in §10 leiten sich systematische Tests ab. Wenn QS einen davon als ECHT befindet → ein Mini-Hotfix-Spec analog `aircraft-type-match.md` Maintenance-Workflow:

| # | Was testen | Wie |
|---|---|---|
| T1 | Phase-Race Sampler vs Streamer | Mock-Test: Sampler setzt `landing_at`, Streamer-Tick checkt + schreibt Phase. Verifizieren beide den gleichen Timestamp. |
| T2 | Pause-Resume-Drift | Replay-Test: aktiven Final-Flug speichern, Sim-Snapshot wechseln (anderer Flughafen), App-Restart, ersten Tick laufen lassen. Erwartung: kein Phantom-Touchdown. |
| T3 | Holding-Detection | Test ob `phase = Holding` jemals gesetzt werden kann. Wenn nein → Spec aktualisieren oder Code implementieren. |
| T4 | Climb→Descent Reversibel | Mock-Test: Phase=Climb, kurze VS=-600 fpm (1 Tick), dann VS=+1500 fpm. Erwartung: bleibt Climb (Dwell). Aktuell vermutlich Bug. |
| T5 | was_airborne Reset | Mock-Test: was_airborne=true, dann 30 s `agl<50ft && on_ground=true`. Erwartung: was_airborne=false. Aktuell vermutlich Sticky. |
| T6 | Phase-Skip im Slew-Mode | Mock-Test: Phase=Boarding, naechster Snapshot agl=30000ft, on_ground=false, gs=300. Erwartung: irgendein vernuenftiges Verhalten (entweder schnell durch FSM oder Block + Warning). |

---

## 12. Glossar

- **Phase:** Wert von `FlightPhase` enum. Aktuell aktive Position im Flight-Lifecycle.
- **Transition:** Wechsel von einer Phase zur naechsten in `step_flight`.
- **Tick:** Ein Aufruf von `step_flight` ausgeloest vom Streamer-Worker (5-30 s je nach Phase).
- **Sampler-Tick:** Ein Aufruf vom Touchdown-Sampler (50 Hz waehrend Approach/Final/Landing).
- **Anti-Flicker:** Mechanik die verhindert dass kurze SimVar-Glitches Phase-Wechsel ausloesen (Dwell, Edge-Detection, Sticky-Flag).
- **Side-Effect:** Was waehrend einer Transition zusaetzlich passiert (Timestamp setzen, Event loggen, MQTT publishen).
- **Universal Arrived-Fallback:** Schutzschicht damit der Flight auch dann auf Arrived kommt wenn der normale BlocksOn-Pfad nicht durchlaeuft.
- **VERDACHT:** Markierung in dieser Spec fuer Code-Stellen die im Audit verdaechtig wirkten — keine bewiesene Bug aber sollte QS systematisch nachgehen.

---

**Ende der Spec v1.0 — bitte QS-Review. Ziel: VERDACHTS-Liste in §10 systematisch durchgehen, jeden Punkt entweder als "kein Bug, dokumentieren" oder "echter Bug, Hotfix-Spec" klassifizieren.**

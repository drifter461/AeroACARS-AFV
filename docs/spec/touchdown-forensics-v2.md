# Touchdown-Forensik v2 — Architektur-Spec für v0.7.0

**Status:** Draft / Review — to be approved by VA-Owner before implementation
**Author:** Refactor based on real-flight data analysis from 6 test-flights (10.05.26)
**Cutoff:** Forward-only — gilt nur für Flüge `started_at >= 2026-05-10T02:44:00Z` (= post-v0.6.0-Refactor mit garantiertem 50Hz Buffer)

---

## 0. Warum dieses Dokument

Das aktuelle Touchdown-Forensik-System (v0.5.x → v0.6.2) hat **9 zusammenhängende Bugs** gleicher architektonischer Wurzel:
- Single-shot TD-Detection (= „first edge wins")
- Sim-Engine-edge wird als „echter" TD verwendet (X-Plane edge-trigger-happy bei Float)
- vs_at_edge unconditional-override ohne Plausibilitäts-Prüfung
- Keine Multi-TD-Unterstützung (T&G/Go-Around/Bounce)
- Keine Confidence-Tagging

**Beweis aus echten Flügen** (alle 6 test-flights mit voll-funktionierendem 50Hz Buffer post v0.6.0):

| Flug | Sim | Gebracht angezeigt | Was richtig wäre | Δ |
|---|---|---|---|---|
| PTO 105 GA | MSFS | -55 fpm / 100 | -55 fpm / 100 | 0 ✓ |
| **PTO 705 T&G** | MSFS | -182 fpm vom 200ms-Streifschuss | echter zweiter TD nicht bewertet | unbekannt ❌ |
| DLH 304 | MSFS | -357 fpm / 80 | -357 fpm / 80 | 0 ✓ |
| CFG 785 | MSFS | -142 fpm / 100 | -142 fpm / 100 | 0 ✓ |
| DLH 742 | MSFS | -191 fpm / 100 | -191 fpm / 100 | 0 ✓ |
| **DAH 3181** | **X-Plane** | **+104 fpm / 80** | **-334 fpm vom echten TD-Frame** | **-438** ❌ |

**Befund:** MSFS-Pfad ist algorithmisch korrekt (4/4 Flüge unverändert). X-Plane-Pfad ist algorithmisch broken (Float wird als TD gewertet). T&G/Go-Around-Pfad ist broken sim-übergreifend (zweiter TD wird ignoriert).

**Daten-Befund:** Alle nötigen Daten sind in den 50Hz Sampler-Buffern + Streamer-Stream vorhanden. Was fehlt ist nicht Daten-Sampling, sondern Algorithmus-Logik.

---

## 1. Daten-Inventar (was wir HABEN, was wir NICHT haben)

### 1.1 Pro 50Hz Sampler-Sample (heute, im JSONL `touchdown_window`-Event)

```
at, vs_fpm, g_force, on_ground, agl_ft, heading_true_deg,
groundspeed_kt, indicated_airspeed_kt, lat, lon, pitch_deg, bank_deg
```

### 1.2 Pro Streamer-Position-Snapshot (1-3s cadence, im JSONL `position`-Events)

```
ALLES vom Sim — über 80 Felder inklusive:
gear_normal_force_n (X-Plane only — Sim-Limit MSFS)
fuel_total_kg, fuel_flow_kg_per_h
engines_running, gear_position, flaps_position
spoilers_handle_position, spoilers_armed
autopilot_*, autobrake, parking_brake
weather (wind, qnh, oat, mach)
... etc
```

### 1.3 Was im 50Hz Sampler-Buffer FEHLT (Datenlücke)

`gear_normal_force_n` ist im Streamer-Stream vorhanden, aber NICHT im `touchdown_window` Sample-Buffer. Das ist die **kritische Datenlücke** für X-Plane TD-Validation.

### 1.4 Was wir BEWUSST NICHT nutzen werden (addon-unzuverlässig)

Aircraft-Addons reportieren diese Felder unzuverlässig (Pilot-Bestätigung):

- `spoilers_armed` / `spoilers_handle_position` — manche Addons setzen nie
- `autopilot_*` — addon-spezifische Behandlung
- `autobrake` — nicht in allen Addons
- `engines_running` Throttle/N1 trace — addon-spezifisch
- Per-gear contact points (CONTACT POINT IS ON GROUND:0/1/2 in MSFS) — addon-spezifisch

**→ Validation-Logik darf NUR auf zuverlässige Sim-native Daten verlassen** (siehe 1.1 + 1.2 ohne addon-spezifische Felder).

### 1.5 Sim-spezifische Datenverfügbarkeit

| Daten | MSFS | X-Plane |
|---|---|---|
| `on_ground` (zuverlässig) | ✅ konservativ | ⚠️ trigger-happy |
| `vs_fpm` | ✅ | ✅ |
| `altitude_agl_ft` | ✅ | ✅ |
| `g_force` | ✅ | ✅ |
| `pitch_deg`, `bank_deg` | ✅ | ✅ |
| `groundspeed_kt`, `indicated_airspeed_kt` | ✅ | ✅ |
| `gear_normal_force_n` | ❌ Sim-Limit | ✅ via DataRef |
| `PLANE TOUCHDOWN NORMAL VELOCITY` SimVar (latched) | ⚠️ addon-abhängig | ❌ |

**→ Sim-Trennung ist STRUKTURELL unvermeidbar** weil X-Plane das wichtigste Validation-Signal hat (`gear_normal_force_n`) und MSFS nicht.

---

## 2. Architektur-Übersicht (3 Layer)

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: TD-Candidate Detection (sim-spezifisch)            │
│  - sammelt potenzielle TD-Edges aus 50Hz-Stream             │
│  - kein Filter, nur Detection                               │
└────────────────────┬────────────────────────────────────────┘
                     │ candidate stream
┌────────────────────▼────────────────────────────────────────┐
│ Layer 2: TD-Validation (sim-spezifisch — 4 Tests)           │
│  - jede Candidate wird gegen 4 Tests geprüft                │
│  - PASS → gilt als „validierter TD"                         │
│  - FAIL → markiert als „false-edge" (Streifschuss/Float)    │
└────────────────────┬────────────────────────────────────────┘
                     │ validated TDs
┌────────────────────▼────────────────────────────────────────┐
│ Layer 3: VS-Calculation + Score (sim-agnostic)              │
│  - berechnet VS am echten TD-Frame                          │
│  - Cross-Validation zwischen Quellen                        │
│  - HARD GUARD gegen positive Werte                          │
│  - emittiert touchdown_complete pro validiertem TD          │
└────────────────────┬────────────────────────────────────────┘
                     │ touchdown_complete events
┌────────────────────▼────────────────────────────────────────┐
│ Layer 4: Final-Landing-Selection (PIREP-Filing-time)        │
│  - bei mehreren validated TDs: welcher gilt als „Landing"?  │
│  - basierend auf Phase-Sequenz + Aircraft-State             │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Layer 1: TD-Candidate Detection

### 3.1 Sim-spezifische Detection

**X-Plane:**
```
candidate_edge wenn:
  prev.in_air == True  AND
  (current.on_ground == True  OR  current.gear_force_n > 0)
```

**MSFS:**
```
candidate_edge wenn:
  prev.in_air == True  AND
  current.on_ground == True
```

`prev.in_air` = `!prev.on_ground && (prev.gear_force_n.unwrap_or(0) <= EPS)`

### 3.2 Was zu speichern ist (pro Candidate)

```rust
struct TdCandidate {
    edge_sample_index: usize,
    edge_at: DateTime<Utc>,
    edge_agl_ft: f32,
    edge_vs_fpm: f32,
    edge_gear_force_n: Option<f32>,  // X-Plane only
    edge_g_force: f32,
}
```

### 3.3 Multi-Edge-Tracking

Sampler erfasst **alle** Candidates einer Flight-Session, nicht nur den ersten. `sampler_touchdown_at: Option<DateTime>` wird **abgeschafft** und ersetzt durch `Vec<TdCandidate>`.

---

## 4. Layer 2: TD-Validation (4 Tests pro Sim)

Jede Candidate wird gegen 4 Tests geprüft. **Mindestens 3 von 4** müssen PASS sein für „validierter TD".

### 4.1 X-Plane Tests

| Test | Bedingung | Schwellwert |
|---|---|---|
| **T1: gear_force-impact** | peak `gear_force_n` im Window [edge, edge+500ms] > threshold | `> 0` für `> 200ms` continuous |
| **T2: sustained_ground_contact** | `on_ground == True` für mindestens N continuous samples | `>= 25 samples` (= 500ms @ 50Hz) |
| **T3: low_agl_persistence** | `agl < 5 ft` für mindestens N samples nach edge | `>= 50 samples` (= 1000ms) |
| **T4: vs_negative_at_edge_or_smoothed** | entweder `vs_at_edge < +50 fpm` ODER `vs_smoothed_500ms < -10 fpm` | siehe |

### 4.2 MSFS Tests (kein gear_force)

| Test | Bedingung | Schwellwert |
|---|---|---|
| **T1: g_force-spike** | `peak g_force` im Window [edge, edge+500ms] > threshold | `> 1.05` (lockere threshold) |
| **T2: sustained_ground_contact** | `on_ground == True` für mindestens N continuous samples | `>= 25 samples` (= 500ms) |
| **T3: low_agl_persistence** | `agl < 5 ft` für mindestens N samples nach edge | `>= 50 samples` (= 1000ms) |
| **T4: vs_negative_at_edge_or_smoothed** | entweder `vs_at_edge < +50 fpm` ODER `vs_smoothed_500ms < -10 fpm` | siehe |

### 4.3 Validierung gegen DAH 3181 (X-Plane Float-Streifschuss)

| Test | Wert | Result |
|---|---|---|
| T1 (gear_force) | edge gear_force = 0 N (Streifschuss) | **FAIL** |
| T2 (sustained) | on_ground für ~700ms (≈ 35 samples) | PASS |
| T3 (agl<5ft) | agl steigt auf 5.7ft nach 700ms, auf 7.9ft nach 1.2s | **FAIL** |
| T4 (vs negative) | vs_at_edge=+104, vs_smoothed_500ms=-57 | PASS (smoothed negative) |

**2/4 PASS → FAIL Validation → false-edge → weiter beobachten**

Beim echten TD bei sample um 07:54:02 (4 sec später):
- gear_force=827171 N → T1 PASS
- on_ground bleibt True für > 1 sec → T2 PASS
- agl bleibt < 1 ft → T3 PASS
- vs am echten TD = -334 fpm → T4 PASS
- **4/4 PASS → validierter TD**

### 4.4 Validierung gegen MSFS-Flüge (alle 4 mit landing_analysis)

Beispiel CFG 785:
| Test | Wert | Result |
|---|---|---|
| T1 (g_force) | g_force-spike = 1.18 | PASS |
| T2 (sustained) | on_ground bleibt True | PASS |
| T3 (agl<5ft) | bleibt unter 1ft (rollout) | PASS |
| T4 (vs negative) | vs_at_edge = -142, vs_smoothed_500ms = -130 | PASS |

**4/4 → validierter TD → vs_at_edge = -142 (= unverändert zur aktuellen Logik)**

---

## 5. Layer 3: VS-Calculation am validierten TD-Frame

### 5.1 TD-Frame-Bestimmung (sim-spezifisch)

**X-Plane:** TD-Frame = sample mit peak `gear_force_n` im Window [edge, edge+500ms]
- Bei DAH 3181: peak ist um 07:54:02.4 mit 827kN → TD-Frame ist ~Stream-sample 198 (im 50Hz Buffer)

**MSFS:** TD-Frame = edge-sample selbst (wenn validation passed)
- Funktioniert weil MSFS-edge konservativ ist

### 5.2 VS-Berechnung am TD-Frame (sim-agnostic Cascade)

```rust
fn compute_landing_vs(td_frame_idx, samples, sim) -> LandingRateResult {
    let vs_at_td = samples[td_frame_idx].vs_fpm;
    let vs_smoothed_500_at_td = avg(samples[td_frame_idx-25..=td_frame_idx]);
    let vs_smoothed_1000_at_td = avg(samples[td_frame_idx-50..=td_frame_idx]);
    let pre_flare_peak = min(samples[td_frame_idx-100..=td_frame_idx-25].vs_fpm);
    
    // Cascade: bevorzugt vs_at_td, fällt zurück bei Plausibilitäts-Versagen
    let chosen = if vs_at_td < -10.0 {
        (vs_at_td, "vs_at_td_frame", Confidence::High)
    } else if vs_smoothed_500_at_td < -10.0 {
        (vs_smoothed_500_at_td, "vs_smoothed_500ms", Confidence::Medium)
    } else if vs_smoothed_1000_at_td < -10.0 {
        (vs_smoothed_1000_at_td, "vs_smoothed_1000ms", Confidence::Low)
    } else if pre_flare_peak < 0.0 {
        (pre_flare_peak, "pre_flare_peak", Confidence::VeryLow)
    } else {
        return Err(LandingRateError::AllSourcesPositive);  // HARD REJECT
    };
    
    // Cross-Validation
    let cross_check = compute_cross_validation(samples, td_frame_idx, chosen);
    
    Ok(LandingRateResult {
        vs_fpm: chosen.0,
        source: chosen.1,
        confidence: chosen.2,
        cross_check_spread_fpm: cross_check.spread,
        ...
    })
}
```

### 5.3 HARD GUARDS (strukturell)

```rust
fn finalize_vs(candidate_fpm: f32) -> Result<f32, RejectionReason> {
    if candidate_fpm > 0.0 {
        // PHYSIKALISCH UNMÖGLICH bei Touchdown
        return Err(RejectionReason::PositiveVs);
    }
    if candidate_fpm < -3000.0 {
        // GIB-Sample / Sim-Glitch — Real-world max um -1500 fpm
        return Err(RejectionReason::ImplausiblyHigh);
    }
    Ok(candidate_fpm)
}
```

**Bei `Err(...)`:** Kein Score finalisiert, Cockpit zeigt Banner „Touchdown forensics inconclusive — please review manually". Pilot kann manuell PIREP filen.

---

## 6. Layer 4: Final-Landing-Selection bei Multi-TDs

### 6.1 TD-Lifecycle pro Flight-Session

```
Per Flug entsteht ein Vec<ValidatedTd>:
  td[0] = erster validierter TD
  td[1] = zweiter validierter TD (nach climb-out > 100ft AGL > 30s)
  td[2] = ...
  td[N] = letzter validierter TD vor PIREP-Filing
```

### 6.2 „Final Landing" Bestimmung (zum PIREP-Filing-Zeitpunkt)

```
LAST validierter TD wo:
  - aircraft danach für mindestens 30 sec UNTER 50ft AGL bleibt
  - UND groundspeed sinkt UNTER 30kt
  - UND Phase-FSM in [Landing, TaxiIn, BlocksOn, Arrived] ist
```

→ **PIREP-Score wird VOM final-Landing-TD genommen**, nicht vom ersten.

### 6.3 Beispiel PTO 705 (Touch-and-Go)

```
td[0]: 07:54:30  vs ~ -180 fpm  (200ms ground contact, dann climb-out auf 1560ft AGL)
       → AFTER td[0]: aircraft stieg auf 1560ft AGL → MARK as „T&G/Go-Around"
       → NICHT als final landing

td[1]: 08:01:29  vs ~ -111 fpm  (sustained ground contact, rollout, BlocksOn)
       → AFTER td[1]: aircraft bleibt am Boden, gs sinkt
       → MARK as „final landing"
       → PIREP-Score = vs ~ -111 fpm
```

### 6.4 Beispiel DAH 3181 (Float-Streifschuss + echter TD)

```
candidate[0]: 07:53:58.46  Float-Streifschuss
              → Validation FAIL → markiert als „false-edge", NICHT in td[]

td[0]: 07:54:02.4  echter TD mit gear_force=827kN  vs=-334 fpm
       → AFTER td[0]: rollout, Phase=Landing
       → MARK as „final landing"
       → PIREP-Score = vs ~ -334 fpm, smooth
```

### 6.5 Beispiel Bounce (Hard-TD + Bounce + Re-TD)

```
td[0]: TD mit vs=-600 fpm  (HARD)
       → AFTER td[0]: kurzer Bounce auf 3ft AGL (kein climb-out > 50ft)
       → KEIN „final landing" mark — bounce noch nicht abgeschlossen

td[1]: Re-TD mit vs=-200 fpm  (Bounce-Touch)
       → AFTER td[1]: rollout, gs sinkt, Phase=Landing
       → MARK as „final landing"

→ PIREP-Score Frage: welche VS gilt?
```

**Entscheidung (Bounce-Sequenz):** Wenn td[0] ein HARD-TD war (vs < -300) UND innerhalb 2 sec kommt td[1] mit vs > -300 → das ist ein Bounce-Pattern. **Score = vs_max(td[0..N])** (= härtester impact zählt). PLUS `bounce_count = N`.

Bei DAH 3181: td[0] (peak gear_force) hatte vs=-334. Bounce-Detection markiert es als bounce_count=1. Score = -334.

### 6.6 Wichtig: Re-Score während des Flugs

- Pilot sieht im Cockpit nach jedem validated TD den **vorläufigen** Score („preliminary")
- Bei Climb-out > 100ft AGL > 30s: Cockpit zeigt „last TD: T&G/Go-Around — waiting for final"
- Bei „final landing" mark: Cockpit zeigt finalen Score
- Beim PIREP-Filing: nimm finalen Score (= aus dem als „final landing" markierten TD)

---

## 7. Schema-Änderungen

### 7.1 `TouchdownWindowSample` (50Hz Sampler-Buffer)

```rust
// VORHER (heute)
pub struct TouchdownWindowSample {
    pub at: DateTime<Utc>,
    pub vs_fpm: f32,
    pub g_force: f32,
    pub on_ground: bool,
    pub agl_ft: f32,
    pub heading_true_deg: f32,
    pub groundspeed_kt: f32,
    pub indicated_airspeed_kt: f32,
    pub lat: f64,
    pub lon: f64,
    pub pitch_deg: f32,
    pub bank_deg: f32,
}

// NACHHER (v0.7.0)
pub struct TouchdownWindowSample {
    // ... bestehende Felder ...
    
    // NEU: X-Plane TD-validation anchor
    pub gear_normal_force_n: Option<f32>,  // None bei MSFS, Some bei X-Plane
}
```

**Backward-Compat:** alte JSONLs ohne das Feld deserialisieren mit `None` (serde-default). Kein Migration nötig.

### 7.2 `LandingRateResult` (neu)

```rust
pub struct LandingRateResult {
    pub vs_fpm: f32,
    pub source: &'static str,  // "vs_at_td_frame" | "vs_smoothed_500ms" | ...
    pub confidence: Confidence,  // High | Medium | Low | VeryLow
    pub td_frame_index: usize,
    pub cross_check_spread_fpm: f32,
    pub validation_score: u8,  // 0-4 (wie viele Tests passed)
    pub td_index: u8,  // 0 = erster, 1 = zweiter, ... bei Multi-TD
    pub is_final_landing: bool,
}

pub enum Confidence { High, Medium, Low, VeryLow }
```

### 7.3 `touchdown_complete` Event (erweitert)

```rust
// Pro validiertem TD ein Event (nicht nur einer pro Flug)
pub struct TouchdownCompleteEvent {
    pub timestamp: DateTime<Utc>,
    pub td_index: u8,
    pub is_final: bool,  // erst beim PIREP-Filing endgültig gesetzt
    pub validation: ValidationDetail,
    pub landing_rate: LandingRateResult,
    // ... rest wie bisher (airport, runway, weather etc.)
}
```

---

## 8. Stop-Bedingungen / Edge-Cases

### 8.1 Kein TD detected nach 30s in Phase=Landing

→ Synthetic-TD basierend auf:
- `vs_min` im low-AGL window
- AGL-Trend (= aircraft hat sich auf < 5 ft niedergelassen)
- Confidence = `VeryLow`
- Activity-Log: `WARN sampler did not detect a validated TD — falling back to synthetic`

### 8.2 Pilot quittete App vor finalem TD

→ Beim Resume: `Vec<ValidatedTd>` wird aus persistierter active_flight.json restored. Sampler beobachtet weiter, sammelt zusätzliche TDs, „final landing" wird erst beim Filing entschieden.

### 8.3 PIREP-Filing OHNE jegliche validated TD

→ Cockpit zeigt Banner „No validated touchdown detected — manual review required".
→ Pilot kann trotzdem filen (manual PIREP), Score-Felder bleiben null/empty.
→ JSONL hat trotzdem alle samples für Forensik.

### 8.4 SimVar `PLANE TOUCHDOWN NORMAL VELOCITY` (MSFS) als optionale Cross-Check

Wenn gesetzt UND innerhalb ±100 fpm vom validated VS → erhöht Confidence auf High. Wenn divergent → log warning, nimm validated. Niemals SimVar als primary (addon-unzuverlässig).

---

## 9. Migration & Rollout

### 9.1 Cutoff

- Neue Logik gilt für Flüge mit `started_at >= 2026-05-10T02:44:00Z`
- Ältere PIREPs bleiben mit alter Logik gescored (kein Re-Score)
- Forensik-Tools können später optional alte JSONLs gegen die neue Logik laufen lassen (= Konsistenz-Check, kein PIREP-update)

### 9.2 Schema-Backward-Compat

- TouchdownWindowSample.gear_normal_force_n optional → alte JSONLs deserialisieren weiter
- aeroacars-live recorder akzeptiert beide Schemas
- Frontend (Cockpit + Live-Map) nutzt confidence-tag wenn vorhanden, ignoriert sonst

### 9.3 Release-Pfad

- v0.7.0 als Major-Bump (semantische Touchdown-Score-Änderung)
- Pilot-Schutz: erst Prerelease, dann Test-Flight, dann Latest
- Bilingual Notes mit Vorher/Nachher Beispielen aus den 6 Test-Flügen
- Discord-Ankündigung mit explizitem Hinweis: „Score-Logik fundamental anders, vor allem bei X-Plane Float-Landings und Touch-and-Go"

---

## 10. Akzeptanz-Tests (gegen die 6 echten JSONLs)

| Flug | Sim | Erwartung neue Logik | Heute |
|---|---|---|---|
| PTO 105 GA | MSFS | -55 fpm, score 100, conf=High, td_count=1 | -55 ✓ identisch |
| **PTO 705 T&G** | MSFS | **td[0]=-182 (T&G), td[1]=echter TD vs ~-111, final=td[1]** | -182 vom Streifschuss ❌ |
| DLH 304 | MSFS | -357 fpm, score 80, conf=High, td_count=1 | -357 ✓ identisch |
| CFG 785 | MSFS | -142 fpm, score 100, conf=High, td_count=1 | -142 ✓ identisch |
| DLH 742 | MSFS | -191 fpm, score 100, conf=High, td_count=1 | -191 ✓ identisch |
| **DAH 3181** | **X-Plane** | **vs ~-334 fpm, score=smooth, conf=High, td_count=1, false-edge=1** | **+104 Float ❌** |

**Implementation gilt als erfolgreich wenn:**
- Alle 4 MSFS-Flüge bit-identisch zu heute (= keine Regression)
- DAH 3181 zeigt negative VS aus echtem TD-Frame (statt +104)
- PTO 705 zeigt 2 validierte TDs, Score vom echten zweiten TD

---

## 11. Implementation-Plan (mit Zeit-Schätzung)

| Phase | Was | Zeit |
|---|---|---|
| A | Sample-Schema erweitern (`gear_normal_force_n`) | 30 min |
| B | TD-Candidate-Detection (Layer 1) — sim-spezifisch | 1.5 h |
| C | TD-Validation (Layer 2) — 4 Tests pro Sim | 2 h |
| D | VS-Cascade + HARD GUARDS (Layer 3) | 1.5 h |
| E | Multi-TD Lifecycle + Final-Landing-Selection (Layer 4) | 2 h |
| F | Sampler-Refactor (multi-edge tracking) | 2 h |
| G | Frontend (Cockpit-Banner + Confidence-Badge) | 1.5 h |
| H | Acceptance-Tests gegen die 6 JSONLs | 1 h |
| I | Bilingual Release-Notes + Discord-Ankündigung | 30 min |
| J | Build + Deploy + Pilot-Test | 1 h |

**Gesamt:** ~13 Stunden

---

## 12. Was BEWUSST NICHT in v0.7.0 ist

- **Re-Score alter PIREPs** — Forward-only, alte bleiben wie sie sind
- **Per-gear contact points** — addon-unzuverlässig
- **Throttle/N1/Spoilers/Autobrake** — addon-unzuverlässig
- **Sim-spezifische TD-FRAME-Detection in MSFS** — nutzt nur edge + g_force-spike (kein gear_force in MSFS)
- **„Auto-Forensik" für Pre-v0.7.0 PIREPs** — könnte v0.7.1 sein

---

## 13. Risiken

| Risiko | Mitigation |
|---|---|
| Validation-Tests zu strict → echter TD wird false-edge | Akzeptanz-Tests gegen 6 JSONLs decken Edge-Cases ab. Plus Synthetic-Fallback (8.1). |
| Validation-Tests zu loose → Streifschuss als TD durch | Test 1 (gear_force/g_force-spike) ist primary discriminator |
| Bounce-Lifecycle falsch klassifiziert | Bounce-Detection (6.5) explizit dokumentiert |
| MSFS ohne gear_force schwächere Validation | g_force-spike + sustained + low_agl als Triple-Anchor; in 4/4 Test-Flügen bestätigt |
| Schema-Änderung breaks alte JSONLs | Optional field, serde-default, getestet |
| Pilot verwirrt durch Multi-TD-Anzeige | UI: nur „final landing"-Score prominent, T&G/Bounce als Sekundär-Info |

---

## 14. Open Questions für VA-Owner-Review

1. **Bounce-Score:** härtester impact ODER letzter sustained TD? (Sektion 6.5)
2. **„Final landing" Definition:** 30 sec unter 50ft AGL + gs<30kt — passt das? Oder strenger/lockerer?
3. **Synthetic-TD Fallback:** akzeptabel als VeryLow-Confidence-Score, oder lieber gar kein Score?
4. **gear_force-Threshold in T1 (X-Plane):** fix `> 0 für > 200ms` ODER aircraft-mass-aware (`> 0.3 × static_weight`)?
5. **MSFS-SimVar Cross-Check:** sollte Confidence boosten wenn vorhanden + plausibel (8.4)?

---

## 15. Bekannte Bugs die durch diese Spec gefixt werden

| # | Bug | Wie gefixt |
|---|---|---|
| 1 | vs_at_edge unconditional override → positive Landerate | HARD GUARD (5.3) |
| 2 | vs_estimate_xp/msfs nicht negative_only | Cascade hat `< -10` filter (5.2) |
| 3 | Sampler is_none()-Guard verhindert zweiten TD | Multi-TD-Tracking (3.3) |
| 4 | touchdown_complete fehlt beim zweiten Touch | Pro validated TD ein Event (7.3) |
| 5 | landing_estimate_window_ms unvollständig gesetzt | Wird im neuen Schema sauber gesetzt |
| 6 | bounce_count Inkonsistenz analysis vs scored | Single source of truth (analysis only) |
| 7 | flare_detected Heuristik unzuverlässig | Wird nicht mehr für Selection genutzt — nur informativ |
| 8 | X-Plane on_ground edge-trigger-happy bei Float | Validation Test T1 (gear_force-impact) — strukturell zu |
| 9 | T&G/Go-Around: erster Streifschuss als Score | Final-Landing-Selection (Layer 4) |

---

**Ende Spec.** Bitte prüfen + Open Questions in Sektion 14 beantworten, dann starte ich die Implementation.

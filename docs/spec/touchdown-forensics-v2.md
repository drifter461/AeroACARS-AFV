# Touchdown-Forensik v2 — Architektur-Spec für v0.7.0

**Status:** Draft v2.1 (post VA-Owner Review) — to be approved before implementation
**Cutoff:** Forward-only — gilt für Flüge mit `forensics_version: 2` Marker (rolled out ab dem v0.7.0-Release).

---

## Changelog gegenüber v2.0

VA-Owner-Review brachte 6 berechtigte Einwände und 5 beantwortete Open Questions. Alle eingearbeitet:

| Änderung | War (v2.0) | Ist (v2.1) | Sektion |
|---|---|---|---|
| **DAH 3181 on_ground-Dauer** | „~700ms / 35 samples" (FALSCH) | 44ms / 2 samples (verifiziert aus JSONL) | 0, 4.3 |
| **Schwellwerte zeit- statt sample-basiert** | „25 samples = 500ms" | Timestamps aus `at`-Feld, kein fixer Sample-Count | 4 |
| **gear_force bei X-Plane** | als 1 von 4 Tests (3-of-4 voting) | **Must-Pass Anchor**, nicht Voting | 4.1 |
| **gear_force-Threshold** | „> 0 für 200ms" | aircraft-mass-aware: `>= max(1000N, 0.03 × total_weight_kg × 9.80665)` plus Confirmation-Window | 4.1, 4.4 |
| **TD-Frame-Definition** | „peak gear_force frame" | **3 Frames** unterschieden: contact / impact / load_peak | 5.1 |
| **VS-Quelle** | „vs am peak gear_force frame" | **vs am impact_frame** (= min VS in [contact-250ms, contact+100ms]) | 5.2 |
| **DAH 3181 erwarteter Score** | „smooth" | **acceptable / firm** abhängig von G-Last (-334 fpm ist nicht smooth) | 10 |
| **Datenmodell** | `Vec<ValidatedTd>` | `LandingEpisode` mit nested touches (false_edge / contact / bounce / settle) | 6 |
| **Event-Naming** | `touchdown_complete` (mit voreiligem `is_final`) | `touchdown_detected` (pro TD) + `landing_finalized` (am Filing-Zeitpunkt) | 7.3 |
| **Cutoff** | nur Datum | `forensics_version: 2` Field in Events/PIREP-payload (Datum als Rollout-Hinweis) | 9.1 |
| **Acceptance** | „bit-identisch" | `±5 fpm`, gleicher Score-Bucket, gleicher td_count | 10 |
| **Synthetic-TD-Fallback** | „akzeptabel als VeryLow Score" | Kein Auto-Score, nur Review-Banner | 8.1 |
| **MSFS SimVar** | „optional cross-check" | Confidence-Boost bei Plausibilität, Warnung bei Divergenz, niemals primary | 8.4 |

---

## 0. Warum dieses Dokument

Das aktuelle Touchdown-Forensik-System (v0.5.x → v0.6.2) hat **9 zusammenhängende Bugs** gleicher architektonischer Wurzel:
- Single-shot TD-Detection (= „first edge wins")
- Sim-Engine-edge wird als „echter" TD verwendet (X-Plane edge-trigger-happy bei Float)
- vs_at_edge unconditional-override ohne Plausibilitäts-Prüfung
- Keine Multi-TD-Unterstützung (T&G/Go-Around/Bounce)
- Keine Confidence-Tagging

**Beweis aus echten Flügen** (alle 6 test-flights mit voll-funktionierendem 50Hz Buffer post v0.6.0):

| Flug | Sim | Heute angezeigt | Was richtig wäre | Δ |
|---|---|---|---|---|
| PTO 105 GA | MSFS | -55 fpm / 100 | -55 fpm / 100 | 0 ✓ |
| **PTO 705 T&G** | MSFS | -182 fpm vom 200ms-Streifschuss | echter zweiter TD nicht bewertet | unbekannt ❌ |
| DLH 304 | MSFS | -357 fpm / 80 | -357 fpm / 80 | 0 ✓ |
| CFG 785 | MSFS | -142 fpm / 100 | -142 fpm / 100 | 0 ✓ |
| DLH 742 | MSFS | -191 fpm / 100 | -191 fpm / 100 | 0 ✓ |
| **DAH 3181** | **X-Plane** | **+104 fpm / 80** | **VS am impact_frame ≈ -334 fpm** | **-438** ❌ |

**Befund:** MSFS-Pfad ist algorithmisch korrekt (4/4 Flüge unverändert). X-Plane-Pfad ist algorithmisch broken (Float wird als TD gewertet). T&G/Go-Around-Pfad ist broken sim-übergreifend (zweiter TD wird ignoriert).

**Daten-Befund:** Alle nötigen Daten sind in den 50Hz Sampler-Buffern + Streamer-Stream vorhanden. Was fehlt ist nicht Daten-Sampling, sondern Algorithmus-Logik.

**Verifiziertes DAH 3181 Sample-Trace:**

```
Sample 124 (07:53:58.43)  on_ground=False  agl=2.79  vs= -44   ← in Luft
Sample 125 (07:53:58.46)  on_ground=True   agl=2.60  vs=+104   ← Edge 1 (Float-Streifschuss)
Sample 126 (07:53:58.51)  on_ground=True   agl=2.62  vs=+162
Sample 127 (07:53:58.54)  on_ground=False  agl=2.79  vs=+207   ← schon wieder in Luft (Dauer: 44ms)
…
Sample 198 (07:54:02.31)  on_ground=True   agl=2.46  vs=-401   ← Edge 2 (echter contact_frame)
Sample 226 (07:54:03.41)  on_ground=False  agl=2.38  vs=+142   ← (1104ms sustained, Bounce)
Sample 246 (07:54:04.10)  on_ground=True   agl=2.25  vs=-49    ← Edge 3 (Settle/Rollout)
```

→ Drei TD-candidate-edges, nur **Edge 2 ist der echte first contact**.

---

## 1. Daten-Inventar (was wir HABEN, was wir NICHT haben)

### 1.1 Pro 50Hz Sampler-Sample (heute, im JSONL `touchdown_window`-Event)

```
at, vs_fpm, g_force, on_ground, agl_ft, heading_true_deg,
groundspeed_kt, indicated_airspeed_kt, lat, lon, pitch_deg, bank_deg
```

### 1.2 Pro Streamer-Position-Snapshot (1-3s cadence, im JSONL `position`-Events)

ALLES vom Sim — über 80 Felder inklusive `gear_normal_force_n` (X-Plane only — Sim-Limit MSFS).

### 1.3 Was im 50Hz Sampler-Buffer FEHLT (Datenlücke)

`gear_normal_force_n` ist im Streamer-Stream vorhanden, aber NICHT im `touchdown_window` Sample-Buffer. Das ist die **kritische Datenlücke** für X-Plane TD-Validation.

### 1.4 Was wir BEWUSST NICHT nutzen werden (addon-unzuverlässig)

Aircraft-Addons reportieren diese Felder unzuverlässig (Pilot-Bestätigung):

- `spoilers_armed` / `spoilers_handle_position`
- `autopilot_*`, `autobrake`
- `engines_running` / Throttle / N1
- Per-gear contact points (`CONTACT POINT IS ON GROUND:0/1/2` in MSFS)
- `total_weight_kg` ist meist ok, aber falls null/zero → Fallback-Pfad nötig (siehe 4.4)

**→ Validation-Logik darf NUR auf zuverlässige Sim-native Daten verlassen.**

### 1.5 Sim-spezifische Datenverfügbarkeit

| Daten | MSFS | X-Plane |
|---|---|---|
| `on_ground` | ✅ konservativ | ⚠️ trigger-happy (Float-edge in 40-50ms möglich) |
| `vs_fpm` | ✅ | ✅ |
| `altitude_agl_ft` | ✅ | ✅ |
| `g_force` | ✅ | ✅ |
| `pitch_deg`, `bank_deg` | ✅ | ✅ |
| `gear_normal_force_n` | ❌ Sim-Limit | ✅ via DataRef |
| `PLANE TOUCHDOWN NORMAL VELOCITY` SimVar (latched) | ⚠️ addon-abhängig | ❌ |
| `total_weight_kg` (für mass-aware threshold) | ✅ | ✅ (meist) |

→ **Sim-Trennung ist STRUKTURELL unvermeidbar** weil X-Plane das wichtigste Validation-Signal hat (`gear_normal_force_n`) und MSFS nicht.

---

## 2. Architektur-Übersicht (4 Layer)

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: TD-Candidate Detection (sim-spezifisch)            │
│  - sammelt potenzielle TD-Edges aus 50Hz-Stream             │
│  - Multi-Edge-Tracking (kein single-shot)                   │
│  - kein Filter, nur Detection                               │
└────────────────────┬────────────────────────────────────────┘
                     │ candidate stream
┌────────────────────▼────────────────────────────────────────┐
│ Layer 2: TD-Validation (sim-spezifisch)                     │
│  X-Plane: gear_force ist MUST-PASS anchor                   │
│           plus Plausibilitäts-Tests                         │
│  MSFS:    weiches Voting (g_force-spike + AGL + sustained)  │
│  PASS → contact_frame identifiziert (nicht peak-frame!)     │
│  FAIL → markiert als „false-edge", weiter beobachten        │
└────────────────────┬────────────────────────────────────────┘
                     │ contact_frames
┌────────────────────▼────────────────────────────────────────┐
│ Layer 3: VS-Calculation am IMPACT-Frame (sim-agnostic)      │
│  - impact_frame = min VS in [contact-250ms, contact+100ms]  │
│  - load_peak_frame = max gear_force/G (nur Forensik)        │
│  - VS-Cascade mit HARD GUARDS gegen positive Werte          │
│  - Cross-Validation, Confidence-Tag                         │
└────────────────────┬────────────────────────────────────────┘
                     │ touchdown_detected events
┌────────────────────▼────────────────────────────────────────┐
│ Layer 4: LandingEpisode-Aggregation + Final-Selection       │
│  - bündelt false_edges, contact, bounces, settle, load_peak │
│  - bei mehreren Episoden: erste = T&G, letzte = final       │
│  - landing_finalized event erst beim Filing                 │
│  - Score = härtester Impact innerhalb final-Episode         │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Layer 1: TD-Candidate Detection

### 3.1 Sim-spezifische Detection

**X-Plane:**
```
candidate_edge wenn:
  prev.in_air == True  AND
  (current.on_ground == True  OR  current.gear_normal_force_n > epsilon)
```

**MSFS:**
```
candidate_edge wenn:
  prev.in_air == True  AND
  current.on_ground == True
```

`prev.in_air` = `!prev.on_ground && (prev.gear_normal_force_n.unwrap_or(0) <= epsilon)`
`epsilon = 1.0 N` (Noise-Floor für gear_force-Sensor)

### 3.2 Was zu speichern ist (pro Candidate)

```rust
struct TdCandidate {
    edge_sample_index: usize,
    edge_at: DateTime<Utc>,  // primäre Zeit-Referenz, nicht sample_count
    edge_agl_ft: f32,
    edge_vs_fpm: f32,
    edge_gear_force_n: Option<f32>,  // X-Plane only
    edge_g_force: f32,
}
```

### 3.3 Multi-Edge-Tracking

Sampler erfasst **alle** Candidates einer Flight-Session, nicht nur den ersten. `sampler_touchdown_at: Option<DateTime>` wird **abgeschafft** und ersetzt durch `Vec<LandingEpisode>` (siehe Layer 4).

---

## 4. Layer 2: TD-Validation — sim-spezifische Tests

**Grundprinzip:** Schwellwerte werden via `at`-Timestamp gemessen, nicht via Sample-Count. Sampler ist nicht garantiert genau 50 Hz.

### 4.1 X-Plane: gear_force ist MUST-PASS Anchor

**Required (alle 3 müssen PASS):**

| Test | Bedingung |
|---|---|
| **A1: gear_force-impact (MUST)** | peak `gear_normal_force_n` im Window `[edge_at, edge_at + 500ms]` >= dynamic_threshold (siehe 4.4) UND mindestens 3 consecutive samples (Confirmation-Window 60-100ms) über threshold |
| **A2: low_agl_persistence** | `agl_ft < 5` für `>= 1000ms` ab edge_at (gemessen via Timestamps) |
| **A3: vs_negative_at_impact** | `vs_at_impact_frame < -10 fpm` (siehe 5.1 für impact_frame Definition) |

**Wenn A1 FAIL → Validation FAIL** (Streifschuss, kein Energie-Transfer).

### 4.2 MSFS: weiches Voting (kein gear_force verfügbar)

**4 Tests, mind. 3 müssen PASS:**

| Test | Bedingung |
|---|---|
| **B1: g_force-spike** | `peak g_force` im Window `[edge_at, edge_at + 500ms]` > 1.05 |
| **B2: sustained_ground_contact** | `on_ground == True` für `>= 500ms` continuous (Timestamps) |
| **B3: low_agl_persistence** | `agl_ft < 5` für `>= 1000ms` ab edge_at |
| **B4: vs_negative_at_impact** | `vs_at_impact_frame < -10 fpm` |

### 4.3 Verifizierte Validierung gegen DAH 3181 (X-Plane Float-Streifschuss)

**Edge 1 (Sample 125, 07:53:58.463):**

| Test | Wert | Result |
|---|---|---|
| A1 (gear_force) | gear_force im Window = 0 N (Float, kein Energie-Transfer) | **FAIL** |
| A2 (agl<5ft 1000ms) | agl steigt auf 5.7ft bei sample 130 (746ms später) — danach > 5 | **FAIL** |
| A3 (vs negative at impact) | impact_frame würde mit vs ~ +104 berechnet | **FAIL** |

→ **A1 FAIL → MUST-PASS verfehlt → Edge 1 = false-edge** ✓

**Edge 2 (Sample 198, 07:54:02.310):**

| Test | Wert | Result |
|---|---|---|
| A1 (gear_force) | Streamer-stream zeigt gear_force = 827171 N kurz nach edge | **PASS** |
| A2 (agl<5ft 1000ms) | agl bleibt unter 5ft für > 1 sec | **PASS** |
| A3 (vs negative at impact) | impact_frame vs ≈ -334 fpm (siehe 5.1) | **PASS** |

→ **3/3 PASS → Edge 2 = validierter contact_frame** ✓

**Edge 3 (Sample 246, 07:54:04.098):**

Bounce-touch, kommt 700ms nach Edge 2 mit vs=-49 fpm. Wird als Bounce der gleichen Episode behandelt (Layer 4), nicht als neuer TD.

### 4.4 gear_force-Threshold (mass-aware)

```rust
fn gear_force_threshold_n(total_weight_kg: Option<f32>) -> f32 {
    let abs_floor = 1000.0;  // Newton, hartes Minimum
    let mass_ratio = 0.03;   // = 3% des statischen Gewichts
    let dynamic = total_weight_kg
        .filter(|w| *w > 100.0)  // Plausibilität
        .map(|w| w * 9.80665 * mass_ratio)
        .unwrap_or(abs_floor);
    dynamic.max(abs_floor)
}
```

**Beispiele:**
| Aircraft | total_weight_kg | dynamic | final threshold |
|---|---|---|---|
| Cessna 152 | 757 | 222 N | **1000 N** (floor wins) |
| A320 | 73000 | 21478 N | **21478 N** |
| A330 (DAH 3181) | 250000 | 73550 N | **73550 N** |
| B747 | 333400 | 98099 N | **98099 N** |
| Sim ohne weight | None | — | **1000 N** (floor) |

DAH 3181 hatte gear_force=827171 N → klar über 73550 N → PASS A1.

**Confirmation-Window:** mind. 3 consecutive samples über threshold (≈ 60-100ms) damit Single-Sample-Spikes (Sim-Engine-Glitches) keine false-positive triggern.

### 4.5 Verifizierte Validierung gegen MSFS-Flüge (alle 4 mit landing_analysis)

Beispiel CFG 785:

| Test | Wert | Result |
|---|---|---|
| B1 (g_force-spike) | peak g_force = 1.18 | PASS |
| B2 (sustained 500ms) | on_ground bleibt True (rollout) | PASS |
| B3 (agl<5ft 1000ms) | bleibt unter 1ft | PASS |
| B4 (vs negative at impact) | vs_at_impact_frame ≈ -142 | PASS |

→ **4/4 PASS → validierter contact_frame → vs_at_impact_frame = -142** (= unverändert zu heute)

---

## 5. Layer 3: VS-Calculation am IMPACT-Frame

### 5.1 Drei separate Frames im Window nach contact

Nach VA-Owner-Punkt 5: peak gear_force kommt **nach** dem first impact (Suspension-Compression-Lag). VS dort ist gedämpft oder rebound-kontaminiert.

```
contact_frame:    erste Force-Threshold-Überschreitung (X-Plane)
                  ODER  erste on_ground=True die A1 (oder B-Voting) bestanden hat
impact_frame:     min(vs_fpm) im Window [contact_frame - 250ms, contact_frame + 100ms]
                  → das ist die echte „Sink-Rate beim Aufsetzen"
load_peak_frame:  max(gear_force_n bei X-Plane, g_force bei MSFS) im Window
                  [contact_frame, contact_frame + 500ms]
                  → nur für G-Forensik & Bounce-Detection, NICHT für VS
```

### 5.2 VS-Berechnung am impact_frame (sim-agnostic Cascade)

```rust
fn compute_landing_vs(
    contact_frame_idx: usize,
    impact_frame_idx: usize,  // pre-computed
    samples: &[Sample],
) -> Result<LandingRateResult, RejectionReason> {
    let vs_at_impact = samples[impact_frame_idx].vs_fpm;
    
    // Smoothed Werte AM IMPACT-FRAME (nicht am contact-edge)
    let vs_smoothed_500_at_impact = avg_over_time_window(samples, impact_frame_idx, -500ms, 0);
    let vs_smoothed_1000_at_impact = avg_over_time_window(samples, impact_frame_idx, -1000ms, 0);
    let pre_flare_peak = min_over_time_window(samples, impact_frame_idx, -3000ms, -500ms);
    
    let chosen = if vs_at_impact < -10.0 {
        (vs_at_impact, "vs_at_impact_frame", Confidence::High)
    } else if vs_smoothed_500_at_impact < -10.0 {
        (vs_smoothed_500_at_impact, "vs_smoothed_500ms_at_impact", Confidence::Medium)
    } else if vs_smoothed_1000_at_impact < -10.0 {
        (vs_smoothed_1000_at_impact, "vs_smoothed_1000ms_at_impact", Confidence::Low)
    } else if pre_flare_peak < 0.0 {
        (pre_flare_peak, "pre_flare_peak", Confidence::VeryLow)
    } else {
        return Err(RejectionReason::AllSourcesPositive);  // HARD REJECT
    };
    
    // HARD GUARD
    finalize_vs(chosen.0)?;
    
    // Cross-Validation
    let cross_check = compute_cross_validation_spread(samples, impact_frame_idx);
    
    Ok(LandingRateResult {
        vs_fpm: chosen.0,
        source: chosen.1,
        confidence: chosen.2,
        contact_frame_idx,
        impact_frame_idx,
        load_peak_frame_idx,
        cross_check_spread_fpm: cross_check.spread,
        validation_score: ...,
    })
}
```

### 5.3 HARD GUARDS (strukturell)

```rust
fn finalize_vs(candidate_fpm: f32) -> Result<f32, RejectionReason> {
    if candidate_fpm > 0.0 {
        return Err(RejectionReason::PositiveVs);
    }
    if candidate_fpm < -3000.0 {
        return Err(RejectionReason::ImplausiblyHigh);
    }
    Ok(candidate_fpm)
}
```

**Bei `Err(...)`:** Kein Score finalisiert, Cockpit zeigt Banner „Touchdown forensics inconclusive — please review manually". Pilot kann manuell PIREP filen.

### 5.4 Verifizierte Berechnung gegen DAH 3181

```
contact_frame:      Sample 198 (07:54:02.310, vs=-401, agl=2.46)
impact_frame:       min vs in [197.9, 198.1] - 250ms vor / +100ms nach
                    → Sample mit vs ≈ -334 fpm (= echter Sink-Moment)
load_peak_frame:    Streamer-stream zeigt 1635 kN bei 07:54:08
                    → Forensik: peak G ≈ 1.4

vs_at_impact = -334 fpm
→ < -10 → Confidence::High → vs_fpm = -334 fpm ✓
```

---

## 6. Layer 4: LandingEpisode

VA-Owner-Punkt: nicht „liste validierter TDs", sondern **strukturierte Episoden** die alle Touches eines Landing-Versuchs bündeln.

### 6.1 Datenmodell

```rust
struct LandingEpisode {
    /// Index 0 = erste Episode, 1 = nach Climb-out, etc.
    episode_index: u8,

    /// false-edges die zu dieser Episode gehören
    /// (Streifschuss VOR dem echten contact)
    false_edges: Vec<FalseEdge>,

    /// echter erster Bodenkontakt (validiert)
    contact: ContactDetail,

    /// nachfolgende Bounce-Touches innerhalb derselben Episode
    /// (= aircraft bleibt unter 50ft AGL, kein climb-out > 100ft)
    bounces: Vec<BounceTouch>,

    /// finaler Settle-Frame (Räder bleiben auf, gs sinkt)
    settle: Option<SettleDetail>,

    /// load-peak (Forensik)
    load_peak: LoadPeakDetail,

    /// härtester Impact (kann contact ODER bounce sein)
    /// = der VS der für das Scoring zählt (Bounce-aware)
    hardest_impact_vs_fpm: f32,
    hardest_impact_source: HardestImpactSource,  // Contact | Bounce(idx)

    /// Klassifizierung dieser Episode
    classification: EpisodeClass,  // FinalLanding | TouchAndGo | GoAround
}

enum EpisodeClass {
    /// aircraft blieb am Boden, gs sinkt — Pilot ist gelandet
    FinalLanding,
    /// aircraft hob nach Touch wieder ab und stieg auf < 1000ft AGL,
    /// kam danach wieder runter (Pattern-Flug, T&G)
    TouchAndGo,
    /// aircraft stieg > 1000ft AGL nach dem Touch (Go-Around)
    GoAround,
    /// noch nicht klassifiziert (Episode läuft noch)
    Pending,
}
```

### 6.2 „Final Landing" Episode-Finalisierung (zum PIREP-Filing-Zeitpunkt)

Eine Episode wird `EpisodeClass::FinalLanding` wenn:
- aircraft bleibt für mindestens 30 sec UNTER 50ft AGL nach contact
- UND groundspeed sinkt UNTER 30kt
- UND keine climbout-Sequenz > 100ft AGL nach contact

**Beim PIREP-Filing** wird die Episode mit `classification == FinalLanding` als „die Landung" gewählt. Wenn mehrere FinalLanding existieren → nimm letzte (sollte semantisch nur eine geben).

### 6.3 Beispiel PTO 705 (Touch-and-Go)

```
Episode 0:
  false_edges: []  (erster on_ground edge war direkt valid)
  contact: 07:54:30.020, vs=-182 fpm, sustained ground 200ms
  bounces: []
  settle: None  (aircraft hob wieder ab)
  load_peak: ...
  hardest_impact_vs_fpm: -182
  classification: TouchAndGo  (nach Climb-out auf 1560ft AGL)

Episode 1:
  false_edges: []
  contact: 08:01:29.820, vs=-111 fpm, sustained > 30sec
  bounces: []
  settle: 08:01:42, gs<30kt
  load_peak: ...
  hardest_impact_vs_fpm: -111
  classification: FinalLanding

→ PIREP-Score: vom Episode 1 (-111 fpm)
→ PIREP-Notes: „Touch-and-Go detected (Episode 0, vs=-182). Final landing Episode 1 (vs=-111)."
```

### 6.4 Beispiel DAH 3181 (Float + echter TD + Bounce)

```
Episode 0:
  false_edges: [Edge 1 @ 07:53:58.463 (44ms Streifschuss, A1 FAIL)]
  contact: 07:54:02.310, vs_at_impact=-334 fpm, gear_force=827kN
  bounces: [Edge 3 @ 07:54:04.098 (kurzer Wieder-touch 700ms später, vs=-49)]
  settle: 07:54:30+, rollout
  load_peak: 1635kN @ 07:54:08
  hardest_impact_vs_fpm: -334  (contact war härter als Bounce)
  classification: FinalLanding

→ PIREP-Score: vs=-334 fpm
→ Confidence: High (gear_force, contact_frame, vs all consistent)
```

### 6.5 Bounce-Score (VA-Owner-Antwort 1)

`hardest_impact_vs_fpm = min(contact.vs, bounces.iter().map(|b| b.vs))`

→ Wenn contact = -600 fpm und Bounce = -200 fpm → **PIREP-Score = -600** (= härtester Impact). Bounce wird als Penalty/Note dokumentiert, aber nicht als „die Landung" gewertet.

### 6.6 Während des Flugs: Cockpit-UX

- Per validated TD wird `touchdown_detected` event emittiert
- Cockpit zeigt **vorläufigen** Score („preliminary, episode N")
- Bei Climb-out > 100ft AGL > 30s: Banner „last touch was T&G/Go-Around, waiting for final"
- Beim PIREP-Filing: `landing_finalized` event mit `final_episode_index`
- Cockpit zeigt finalen Score

---

## 7. Schema-Änderungen

### 7.1 `TouchdownWindowSample` (50Hz Sampler-Buffer)

```rust
pub struct TouchdownWindowSample {
    // bestehende Felder ...
    pub gear_normal_force_n: Option<f32>,  // NEU: X-Plane Some, MSFS None
}
```

**Backward-Compat:** alte JSONLs ohne das Feld deserialisieren mit `None`.

### 7.2 Event-Naming (VA-Owner-Punkt)

| Alt (v0.6.x) | Neu (v0.7.0) |
|---|---|
| `touchdown_complete` (mit voreiligem `is_final`) | `touchdown_detected` (pro validated contact_frame) |
| — | `landing_finalized` (am PIREP-Filing, mit `final_episode_index`) |

Gründe:
- `touchdown_detected` kann pro Episode/contact mehrfach feuern, ohne Bedeutungs-Konflikt
- `landing_finalized` wird genau einmal pro Flug emittiert, beim Filing

### 7.3 `forensics_version` Feld (Cutoff via Version)

In allen TD-relevanten Events + im PIREP-payload:

```rust
struct TouchdownDetectedEvent {
    forensics_version: u8,  // = 2 ab v0.7.0
    episode_index: u8,
    contact_frame_index: usize,
    impact_frame_index: usize,
    landing_rate: LandingRateResult,
    // ...
}
```

Recorder/aeroacars-live identifiziert via `forensics_version` welche Auswertungs-Logik zu verwenden ist (alte v1-Events bleiben mit alter Logik gescored).

---

## 8. Stop-Bedingungen / Edge-Cases

### 8.1 Kein TD detected nach 30s in Phase=Landing

→ Kein automatischer Score (VA-Owner-Punkt). Cockpit zeigt Banner:
> „Touchdown forensics inconclusive — please review and file PIREP manually if landing was successful."

JSONL hat trotzdem alle Samples für spätere Forensik.

### 8.2 Pilot quittete App vor finalem TD

→ Beim Resume: `Vec<LandingEpisode>` wird aus persistierter active_flight.json restored. Sampler beobachtet weiter.

### 8.3 PIREP-Filing OHNE jegliche validierte Episode

→ `landing_finalized` Event mit `final_episode_index: None`, Score-Felder bleiben null. Banner wie 8.1.

### 8.4 SimVar `PLANE TOUCHDOWN NORMAL VELOCITY` (MSFS) als Confidence-Boost

VA-Owner-Antwort 5:
- Wenn gesetzt UND `|simvar_vs - vs_at_impact| < 50 fpm` → `Confidence::High` (auch wenn primary nur Medium war)
- Wenn divergent (> 50 fpm Spread) → log warning mit beiden Werten, Score bleibt vom impact_frame
- **Niemals** SimVar als primary (addon-unzuverlässig)

---

## 9. Migration & Rollout

### 9.1 Cutoff via Version, nicht nur Datum

- Alle Events ab v0.7.0 tragen `forensics_version: 2`
- Recorder akzeptiert beide (v1 + v2), wertet via Version aus
- Datum (10.05.26 02:44) als Rollout-Hinweis im UI: „Flüge davor mit Legacy-Forensik"

### 9.2 Schema-Backward-Compat

- TouchdownWindowSample.gear_normal_force_n optional → alte JSONLs deserialisieren weiter
- Frontend nutzt confidence-tag wenn vorhanden, ignoriert sonst

### 9.3 Release-Pfad

- v0.7.0 als Major-Bump (semantische Touchdown-Score-Änderung)
- Pilot-Schutz: erst Prerelease, dann Test-Flight, dann Latest
- Bilingual Notes mit Vorher/Nachher Beispielen
- Discord-Ankündigung mit explizitem Hinweis: „Score-Logik fundamental anders, vor allem bei X-Plane Float-Landings und Touch-and-Go"

---

## 10. Akzeptanz-Tests

VA-Owner-Punkt: **`±5 fpm` Toleranz, gleicher Score-Bucket, gleicher td_count** — nicht bit-identisch (Refactor mit Timestamp-Logik kann minimal andere Werte liefern).

| Flug | Sim | Erwartung neue Logik | Heute | Toleranz |
|---|---|---|---|---|
| PTO 105 GA | MSFS | vs ∈ [-60, -50] fpm, score=smooth, td_count=1 | -55 fpm/100 | ±5 fpm, gleicher Bucket |
| **PTO 705 T&G** | MSFS | **2 Episoden**: Ep 0 = T&G mit vs ∈ [-187, -177], Ep 1 = FinalLanding mit eigenem Score | -182 vom Streifschuss ❌ | bestehen wenn 2 Episoden + Final = Ep 1 |
| DLH 304 | MSFS | vs ∈ [-362, -352] fpm, score=acceptable/firm | -357/80 | ±5 fpm |
| CFG 785 | MSFS | vs ∈ [-147, -137], score=smooth | -142/100 | ±5 fpm |
| DLH 742 | MSFS | vs ∈ [-196, -186], score=smooth | -191/100 | ±5 fpm |
| **DAH 3181** | **X-Plane** | **vs am impact_frame ∈ [-340, -325]**, score=acceptable oder firm (NICHT smooth!), td_count=1, false_edges=1, bounces=1 | +104/80 ❌ | bestehen wenn vs negativ + Float als false-edge erkannt |

**Score-Buckets (zur Klarstellung — bestehende Logik):**
- smooth: `0 > vs > -200`
- acceptable: `-200 >= vs > -400`
- firm: `-400 >= vs > -600`
- hard: `vs <= -600`

→ **DAH 3181 mit -334 fpm = `acceptable` Score-Bucket**, nicht smooth (VA-Owner-Punkt 6 korrigiert).

---

## 11. Implementation-Plan (mit Zeit-Schätzung)

| Phase | Was | Zeit |
|---|---|---|
| A | Sample-Schema erweitern (`gear_normal_force_n`) + serde-default für backward-compat | 30 min |
| B | TD-Candidate-Detection (Layer 1) — sim-spezifisch + multi-edge-tracking | 1.5 h |
| C | TD-Validation (Layer 2) — A-Tests X-Plane + B-Tests MSFS, time-based | 2 h |
| D | impact_frame / contact_frame / load_peak_frame Berechnung (Layer 3) | 1 h |
| E | VS-Cascade + HARD GUARDS + Cross-Validation | 1.5 h |
| F | LandingEpisode Datenmodell + Aggregation | 2 h |
| G | Final-Landing-Selection + Bounce-Score + Episode-Klassifizierung | 1.5 h |
| H | Sampler-Refactor (Multi-Edge, Episoden-State-Machine) | 2.5 h |
| I | Event-Renaming (touchdown_detected + landing_finalized) | 1 h |
| J | forensics_version Marker in Events + PIREP-payload | 30 min |
| K | Frontend (Cockpit-Banner + Confidence-Badge + Episode-Anzeige) | 2 h |
| L | Acceptance-Tests gegen die 6 JSONLs | 1.5 h |
| M | Bilingual Release-Notes + Discord-Ankündigung | 30 min |
| N | Build + Deploy + Pilot-Test | 1 h |

**Gesamt:** ~18 Stunden

---

## 12. Was BEWUSST NICHT in v0.7.0 ist

- **Re-Score alter PIREPs** — Forward-only, alte bleiben wie sie sind (kein Re-Compute via forensics_version=1 → 2)
- **Per-gear contact points** — addon-unzuverlässig
- **Throttle/N1/Spoilers/Autobrake** — addon-unzuverlässig
- **Synthetic-TD Auto-Score** (Sektion 8.1)
- **Re-Score-Tool für Pre-v0.7.0 PIREPs** — könnte v0.7.1 sein als optional-batch-tool

---

## 13. Risiken

| Risiko | Mitigation |
|---|---|
| gear_force-Threshold zu strict → echte leichte TDs werden false-edge | abs_floor=1000N als hartes Minimum, mass_ratio dynamisch nur darüber |
| gear_force-Threshold zu loose → Streifschuss als TD durch | Confirmation-Window 60-100ms (3 consecutive samples) |
| MSFS-Voting zu loose ohne gear_force | Cross-Validation Spread (5.2) liefert Confidence-Hinweis bei divergenten Quellen |
| impact_frame-Window zu eng → echter sink-min außerhalb | [-250ms, +100ms] empirisch gewählt aus DAH 3181 + 4 MSFS-Flügen, kann via Logs nachkalibriert werden |
| Episode-Klassifizierung falsch (T&G vs Bounce) | Schwellwert 100ft AGL klar dokumentiert, anpassbar |
| Schema-Änderung breaks alte JSONLs | Optional field, serde-default, Acceptance-Test gegen pre-v0.7.0 JSONLs |
| Pilot verwirrt durch Multi-Episode-Anzeige | UI: nur Final-Episode-Score prominent, andere als kollabierte Sekundär-Info |

---

## 14. Open Questions — beantwortet (VA-Owner-Review)

1. **Bounce-Score:** ✅ **härtester Impact innerhalb der Episode** (siehe 6.5)
2. **Final Landing Definition:** ✅ Episode-Finalisierung via 30sec/50ft/30kt + keine climbout (siehe 6.2)
3. **Synthetic-TD Fallback:** ✅ **kein Auto-Score, nur Review-Banner** (siehe 8.1)
4. **gear_force-Threshold:** ✅ **mass-aware mit absolute floor** (siehe 4.4)
5. **MSFS SimVar Cross-Check:** ✅ **Confidence-Boost bei Plausibilität, Warnung bei Divergenz** (siehe 8.4)

---

## 15. Bekannte Bugs die durch diese Spec gefixt werden

| # | Bug | Wie gefixt |
|---|---|---|
| 1 | vs_at_edge unconditional override → positive Landerate | HARD GUARD (5.3) + Cascade auf impact_frame (5.1) |
| 2 | vs_estimate_xp/msfs nicht negative_only | Cascade hat `< -10` filter (5.2) |
| 3 | Sampler is_none()-Guard verhindert zweiten TD | Multi-Edge-Tracking + LandingEpisodes (3.3, 6.1) |
| 4 | touchdown_complete fehlt beim zweiten Touch | Pro contact_frame ein `touchdown_detected` event (7.2) |
| 5 | landing_estimate_window_ms unvollständig gesetzt | Wird im neuen Schema sauber gesetzt |
| 6 | bounce_count Inkonsistenz analysis vs scored | Bounce als Teil der Episode (6.1), single source of truth |
| 7 | flare_detected Heuristik unzuverlässig | Wird nicht mehr für Selection genutzt — nur informativ |
| 8 | X-Plane on_ground edge-trigger-happy bei Float | A1 (gear_force) ist MUST-PASS Anchor (4.1) — strukturell zu |
| 9 | T&G/Go-Around: erster Streifschuss als Score | Episode-Aggregation + classification (6.2, 6.3) |

---

## 16. Was sich gegenüber v2.0 strukturell geändert hat (Architektur-Diff)

| Komponente | v2.0 | v2.1 |
|---|---|---|
| Validation-Modell | „3 von 4 Tests" (gleichberechtigt) | X-Plane: gear_force MUST-PASS + 2 Plausibilitäts-Tests; MSFS: weiches 3-of-4-Voting |
| TD-Frame | „peak gear_force frame" | 3 separate Frames: contact / impact / load_peak |
| VS-Quelle | vs_at_edge (= contact-edge) | vs_at_impact (= min vs in [contact-250ms, contact+100ms]) |
| Schwellwerte | sample-count basiert | timestamp-basiert |
| gear_force-Threshold | „> 0 für 200ms" | aircraft-mass-aware (1000N floor + 0.03 × static weight) |
| Datenmodell | flacher Vec<ValidatedTd> | strukturierte LandingEpisode (false_edges + contact + bounces + settle) |
| Event-Naming | touchdown_complete (mit is_final) | touchdown_detected + landing_finalized (zwei Events) |
| Cutoff | nur Datum | forensics_version + Datum als Rollout-Hinweis |
| Acceptance | bit-identisch | ±5 fpm, gleicher Score-Bucket |
| Synthetic-Fallback | als VeryLow Score erlaubt | nur Review-Banner, kein Auto-Score |
| MSFS-SimVar | optional cross-check | Confidence-Boost mit Divergenz-Warnung |
| Bounce-Score | „letzter sustained TD" | härtester Impact innerhalb Episode |
| DAH 3181 Erwartung | „smooth" (FALSCH) | acceptable/firm (-334 fpm) |

---

**Ende Spec v2.1.** Bitte freigeben oder weitere Einwände nennen.

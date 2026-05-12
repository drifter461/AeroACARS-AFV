# Sim-Pause Handling — Master Spec (Diskussion)

**Status:** DRAFT v0.3 — **DEFERRED** (Bewertung verschoben, aktuelles Verhalten by-design — nicht zur Implementation freigegeben)
**Stand:** 2026-05-12
**Trigger:** Real-Pilot-Incident AUA 323 LOWW→ESGG am 2026-05-11 (PIREP `J2VoaZmoD6LQGpMg`) + Pilot-Owner-Feedback zur v0.2: „Toleranzen sind viel zu hart, der Pilot muss die Position auch wieder finden können — Autosave ist meist 10 min alt, Flug kann am Startpunkt-Flughafen geladen werden"
**Vorgaenger:** v0.1 Initial Draft, v0.2 Sim-Pause-Detection-Erweiterung — **beide obsolet durch BREAKING CHANGE in v0.3**
**Paket-Goal in einem Satz:**
Pausen sollen die Aufzeichnung nie still verfälschen und nie unnötig blockieren — `pirep_id` entscheidet ob es derselbe Flug ist, Position/Höhe/Sprit-Drift informieren den Piloten aber blockieren ihn nie, und der Pilot hat immer einen Weg den PIREP zu canceln wenn das System falsch zugeordnet hat.

---

## Changelog v0.2 → v0.3 (BREAKING CHANGE)

| # | Aenderung | Sektion |
|---|---|---|
| **C3-A** | **Toleranz-Ansatz komplett verworfen.** v0.2 hatte 1 NM / 500 ft / 50 kg / 2h als Blocker fuer Auto-Resume. Pilot-Owner-Feedback: real-world Sim-Restart laedt Autosave (~10 min alt, 80 NM Drift bei Cruise-Speeds) oder spawnt am Departure-Airport (= komplette Flugstrecke Drift). Strikte Toleranzen wuerden Auto-Resume in genau den haeufigsten Real-Cases verhindern. **NEUER ANSATZ: Auto-Resume IMMER, Drift nur als UI-Information** (still/Toast/Warning). Pilot kann jederzeit PIREP canceln wenn falsch zugeordnet. | §1, §4.1-4.3, §5 F1-F2 |
| **C3-B** | **`pirep_id` ist der einzige Identitaets-Anker**, nicht Position/Hoehe/Sprit. Position ist Sache des Piloten (Re-Spawn, Slew, Reload). System trifft Auto-Resume-Entscheidung NUR an `pirep_id`-Match. | §1.1, §4.1 |
| **C3-C** | **Hard-Limit auf Pause-Dauer gestrichen.** v0.2 hatte `AUTO_RESUME_MAX_PAUSE = 2h`. Mit `pirep_id`-Identitaet ist kein Zeit-Limit noetig — Heartbeat haelt PIREP, Pilot kommt zurueck wann er kommt. Server-`RESUME_WINDOW_MS` bleibt nur als Fallback (falls Client `pirep_id` nicht mitschickt). | §4.4, §5 F3 |
| **C3-D** | **Szenario-Matrix neu** (§3) mit ~29 dokumentierten Pause-/Resume-Faellen. Pro Szenario: Detection, State-Uebergang, UI-Reaktion, Akkumulator-Verhalten, Server-Verhalten. | §3 (neu) |
| **C3-E** | **Pause-State-Machine explizit definiert** (§2) — vorher implizit ueber `paused_since`-Field. ACTIVE / PAUSED / RESUMING mit dokumentierten Transitions. | §2 (neu) |
| **C3-F** | **F6 (NEU) X-Plane-Plugin-Protokoll-Extension:** Plugin soll einen „Paused"-Heartbeat schicken statt zu schweigen, damit `paused_reason = SimPause` auch fuer X-Plane gesetzt werden kann (gleiche Semantik wie MSFS F5). | §5 F6 |
| **C3-G** | **F7 (NEU) Aircraft-Change-Detection:** wenn Sim-Snapshot mid-flight ein anderes `aircraft_icao` meldet als FlightStats kennt → starker Hinweis „falsches Bid geladen". Zeigt nicht-blockierendes Banner mit Cancel-Option. | §5 F7 |
| **C3-H** | **F8 (NEU) Bid-Change-Detection waehrend Pause:** Pilot kann waehrend laufender Pause das phpVMS-Bid wechseln. Beim Resume erkennt ACARS dass `callsign`/`dep`/`arr` nicht mehr passen → analog F7. | §5 F8 |
| **C3-I** | **UI-Stufen** (§6.3) explizit definiert: `Drift::Quiet` (<1 NM), `Drift::Toast` (1-50 NM), `Drift::Warning` (50-200 NM), `Drift::ExtremeJump` (>200 NM, Banner mit Cancel-Option). Keine ist je ein Blocker fuer Auto-Resume — nur unterschiedliche UI-Lautstaerke. | §6.3 |
| **C3-J** | **Estimate angepasst:** v0.2 hatte ~18h. Mit 3 neuen Features (F6-F8) und neuer Szenario-Matrix-Testing: **~30h Code + 3 Wochen Pilot-QS**. | §10 |
| **C3-K** | Spec-Titel angepasst von „Sim-Disconnect Auto-Resume" auf „Sim-Pause Handling — Master Spec" — adressiert nicht mehr nur Disconnect sondern alle Pause-Formen einheitlich. | Header |

---

## 0. Warum dieses Dokument

### 0.1 Incident AUA 323 (2026-05-11)

Konkrete Beobachtung im Client-Log `J2VoaZmoD6LQGpMg.jsonl.gz` (2193 Zeilen, Pilot Thomas K):

| Zeit (UTC) | Event |
|---|---|
| 11:02:58 | `flight_started` AUA 323 LOWW→ESGG |
| 11:44:03 | Climb → Cruise @ FL350 |
| 12:35:07 | Cruise → Descent @ FL301 |
| **12:38:34** | Letzte Position vor Freeze (noch im Descent) |
| — | **Sim-Freeze + Pause 23 min 40 s — keine Position-Events** |
| **13:02:14** | Erste Position nach manuellem „Flug fortsetzen"-Klick (Flieger steht bereits auf Boden in ESGG) |
| 13:04:26 | `pirep_filed` |

Resultat: zwei Sessions im Server (Session A mit 2062 Ticks ohne PIREP, Session B mit 124 Ticks ARRIVED+PIREP). Pilot bemerkt die Pause erst nach Landung.

### 0.2 v0.2-Befund: Toleranz-Ansatz war falsch

v0.2 versuchte das Problem mit harten Drift-Schwellen zu loesen (1 NM / 500 ft / 50 kg). Pilot-Owner-Feedback hat aufgezeigt:

**Real-World Sim-Restart-Recovery-Pfade:**

| Pfad | Position-Drift gegen pre-Pause |
|---|---|
| Hiccup, Sim erholt sich von selbst | 0 NM |
| MSFS-CTD + Continue Flight + **Autosave-Reload** (typ. 10 min alt) | 0-80 NM (bei Cruise-Speed) |
| MSFS-CTD + **Pilot laedt Bid neu** → spawnt am Departure-Gate | **= komplette Flugstrecke** (4000+ NM bei JFK→HKG) |
| Pilot slewt manuell zur Cruise-Position zurueck | beliebig, aber legitim |
| Pilot pickt versehentlich falsches Bid | komplett andere Welt |

Eine 1-NM-Schwelle wuerde Auto-Resume in 3 von 5 dieser Faelle blockieren — genau die haeufigsten Real-Cases. v0.2 war auf „kurzer Freeze" optimiert, nicht auf echte Sim-Restart-Recovery.

**Konsequenz:** Toleranzen als Blocker sind die falsche Loesung. Position ist **kein Identitaets-Merkmal** eines Fluges.

### 0.3 Neue Design-Philosophie (v0.3)

**„Informieren statt blockieren — `pirep_id` entscheidet, Pilot entscheidet alles andere."**

```
ACARS sieht Resume:
  → ist active_flight.json + pirep_id noch da?
       JA  → AUTO-RESUME (immer, egal welche Drift)
            → Drift nur als Info ins Activity-Log/Toast
            → bei Aircraft-Change oder Bid-Change-Mismatch: 
              nicht-blockierende Warning mit Cancel-Option
       NEIN → bereits gehandhabt (ResumeFlightBanner discover-resumable path)
```

Vier orthogonale Prinzipien:

| # | Prinzip | Konsequenz |
|---|---|---|
| **P1** | `pirep_id` ist der einzige Identitaets-Anker | Auto-Resume entscheidet sich an `pirep_id`-Match, nicht an Position |
| **P2** | Drift informiert, blockt nie | UI-Stufen (Quiet/Toast/Warning/ExtremeJump-Banner), nie als Resume-Verweigerung |
| **P3** | Pilot hat immer einen Ausweg | PIREP-Cancel-UI bleibt zugaenglich, Aircraft-Change/Bid-Change zeigen sie aktiv an |
| **P4** | Heartbeat haelt PIREP unbegrenzt am Leben | Kein Hard-Limit auf Pause-Dauer, weder client- noch server-seitig |

### 0.4 Drei-Tier-Pause-Szenarien (bleibt aus v0.2)

| Szenario | Beispiel | Wer haelt den Flug | Realistische Max-Dauer |
|---|---|---|---|
| **A) Kurzer Sim-Hiccup, ACARS laeuft weiter** | Stutter, kurzes Hang, FPS-Drop, Wetter-Download | Streamer-Loop pausiert sich selbst, Heartbeat laeuft weiter | Minuten bis 1-2h |
| **B) Laengere Unterbrechung, ACARS laeuft noch** | MSFS-CTD + Neustart, Pilot-Pause fuer Essen, MSFS Esc-Pause | Streamer pausiert, Heartbeat haelt phpVMS-PIREP am Leben | **unbegrenzt** (solange App offen) |
| **C) Save and continue tomorrow (Long-Haul)** | 12h-Flug, Pilot schliesst App nach 6h, macht morgen weiter | Disk-Persistence + `ResumeFlightBanner kind=auto_resumed` | unbegrenzt (bis phpVMS-Side-Cron raeumt, Heartbeat aus) |

---

## 1. Identitaets-Modell

### 1.1 `pirep_id` als einziger verlaesslicher Anker

Wenn ACARS aus einer Pause kommt und entscheiden muss „ist das derselbe Flug wie vor der Pause", gibt es exakt **einen** Anker der nie luegt: die `pirep_id`. Sie wird von phpVMS einmalig vergeben wenn der Flug angefangen wird, und ACARS speichert sie in:

- **Tauri-Backend Memory** (`flight: Arc<ActiveFlight>` Struktur)
- **`active_flight.json` auf Disk** (ueberlebt App-Restart)
- **phpVMS-Server-DB** (`pireps`-Tabelle)
- **aeroacars-live `flight_sessions.pirep_id`** (sobald gesetzt — siehe F3)

Solange diese vier Quellen dieselbe `pirep_id` zeigen: **es ist derselbe Flug**, egal wo das Flugzeug physisch steht.

### 1.2 Andere Signale: Information, nicht Identitaet

| Signal | Aussagekraft | Verwendung in v0.3 |
|---|---|---|
| Position-Drift | beliebig (Autosave / Reload / Slew) | UI-Stufe (Quiet/Toast/Warning/ExtremeJump) |
| Hoehen-Drift | beliebig (Climb-Phase-Snapshot) | reines Logging |
| Sprit-Drift | beliebig (Autosave hat mehr Sprit als pre-Crash) | reines Logging |
| on_ground-Wechsel | Information ueber Flug-Phase | reines Logging |
| `aircraft_icao`-Aenderung | **starkes Signal** „falsches Bid geladen" (F7) | Warning-Banner mit Cancel-Option |
| `callsign/dep/arr`-Aenderung | **starkes Signal** „Pilot hat Bid gewechselt" (F8) | Warning-Banner mit Cancel-Option |

### 1.3 Verantwortungs-Trennung: Pilot vs. System

| Verantwortung | Wer | Wie |
|---|---|---|
| Flug-Identitaet (`pirep_id`) | System (phpVMS + AeroACARS) | Vergeben einmalig, persistiert auf Disk + Server |
| Position des Flugzeugs | **Pilot** | Sim, Slew, Reload, Autosave — System mischt sich nicht ein |
| „Ist das noch der richtige Flug?" | **Pilot** | UI gibt Hinweise (Drift, Aircraft-Change), Pilot entscheidet |
| Block-/Flight-Time-Korrektur | System | Pause-Akkumulator zieht Zeiten automatisch ab |
| Session-Zusammenhang Server | System | `pirep_id`-Join im Server (F3) |

---

## 2. Pause-State-Machine

### 2.1 States

```
                  ┌──────────────────────────────────────┐
                  │                                      │
                  ▼                                      │
              ┌────────┐    pause-trigger    ┌────────┐ │
              │ ACTIVE │ ──────────────────▶ │ PAUSED │ │
              └────────┘                     └────────┘ │
                  ▲                              │      │
                  │                              │      │
                  │     resume-detected          │      │
                  │       (always)               │      │
                  │                              ▼      │
                  │                          ┌──────────┴─┐
                  └────────────────────────  │  RESUMING  │
                                             └────────────┘
```

**ACTIVE** — Normaler Streaming-Betrieb. Position-Posts laufen, Phase-FSM tickt, Heartbeat regelmaessig.

**PAUSED** — Aufzeichnung steht still. Heartbeat laeuft weiter (haelt phpVMS-PIREP), aber keine Position-Posts und kein Phase-FSM-Step. `paused_since: Some(t)` und `paused_reason: Some(SimDisconnect | SimPause | NetworkLoss | UserPause)` gesetzt.

**RESUMING** — Uebergangs-State (existiert nur fuer einen Tick). Pause-Block schliessen, `pause_total_duration` updaten, `pause_segments` Liste anhaengen, Drift gegen `paused_last_known` ausmessen, UI-Reaktion auswaehlen, dann zurueck zu ACTIVE.

### 2.2 Transitions

| Von | Nach | Trigger | Auto/Manuell |
|---|---|---|---|
| ACTIVE | PAUSED | SimConnect-Snapshot=None > 30s | Auto (Disconnect-Detection) |
| ACTIVE | PAUSED | SimConnect-Event `Paused`/`Pause_EX1` | Auto (F5) |
| ACTIVE | PAUSED | X-Plane-Plugin sendet `Paused`-Heartbeat | Auto (F6) |
| ACTIVE | PAUSED | (zukuenftig) User klickt Pause-Button im UI | Manuell — out of scope v0.3 |
| PAUSED | RESUMING | SimConnect-Snapshot ist wieder Some(snap) | Auto |
| PAUSED | RESUMING | SimConnect-Event `Unpaused` | Auto (F5) |
| PAUSED | RESUMING | X-Plane-Plugin sendet `Resumed`-Heartbeat | Auto (F6) |
| PAUSED | RESUMING | User klickt „Flug fortsetzen" im Banner | Manuell |
| PAUSED | (PIREP-Cancel) | User cancelt PIREP im phpVMS-UI oder via App | Manuell |
| RESUMING | ACTIVE | Immer (RESUMING ist nur 1-Tick-Zwischen-State) | Auto |

**Wichtig:** Es gibt **keinen** Pfad PAUSED → bleibt-PAUSED-wegen-Drift. Auto-Resume passiert immer wenn der Trigger feuert. Drift ist nur UI-relevant.

### 2.3 Persistenz

Alle State-Felder muessen `save_active_flight` ueberleben:

```rust
struct FlightStatsPauseFields {
    paused_since: Option<DateTime<Utc>>,
    paused_last_known: Option<PausedSnapshot>,
    paused_reason: Option<PausedReason>,
    pause_total_duration: chrono::Duration,
    pause_segments: Vec<PauseSegment>,
}
```

Alle Felder `#[serde(default)]` → Pre-v0.3 `active_flight.json` weiter ladbar.

Bei App-Restart waehrend PAUSED:
1. Disk-Resume laed `paused_since` etc. → State bleibt PAUSED
2. ResumeFlightBanner zeigt 30s-Countdown wie bisher
3. Bei Confirm: ACARS startet Streamer, der nimmt PAUSED-State auf und wartet auf Resume-Trigger

---

## 3. Szenario-Matrix

Pro Szenario:
- **D** = Detection (welcher Trigger)
- **T** = State-Transition
- **U** = UI-Reaktion
- **A** = Akkumulator
- **S** = Server-Verhalten
- **P** = Pilot-Aktion erforderlich

### 3.1 Sim-Disconnect-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.1.1** | MSFS-Hiccup (Sim erholt sich in 30-90s) | snapshot=None >30s, dann Some | ACTIVE→PAUSED→RESUMING→ACTIVE | Quiet (Activity-Log nur) | += pause_duration | nichts (gleiche Session) | keine |
| **3.1.2** | MSFS-CTD + Continue Flight + Autosave (~10 min alt) | snapshot=None >30s, dann Some mit 0-80 NM Drift | ACTIVE→PAUSED→RESUMING→ACTIVE | Toast (1-50 NM) | += pause_duration | gleiche Session (`pirep_id`-Join F3) | keine |
| **3.1.3** | MSFS-CTD + Reload Bid bei Departure (Riesendrift) | wie 3.1.2, aber Drift = Flugstrecke | ACTIVE→PAUSED→RESUMING→ACTIVE | Warning (50-200 NM) oder ExtremeJump-Banner (>200 NM) | += pause_duration | gleiche Session | keine, aber Cancel-Option sichtbar |
| **3.1.4** | X-Plane-Crash (Plugin tot) | Plugin-UDP-Socket-Timeout | wie 3.1.1 | Quiet/Toast je nach Drift | += pause_duration | gleiche Session | keine |
| **3.1.5** | SimConnect-Network-Glitch (Sim laeuft, ACARS verliert Verbindung) | snapshot=None >30s, kurzfristig | ACTIVE→PAUSED→RESUMING→ACTIVE | Quiet | += pause_duration | gleiche Session | keine |

### 3.2 Sim-Pause-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.2.1** | MSFS Esc-Pause (60s) | SimConnect `Paused`-Event (F5) | ACTIVE→PAUSED→RESUMING→ACTIVE | Quiet (Drift=0 garantiert) | += pause_duration | gleiche Session | keine |
| **3.2.2** | MSFS Active-Pause (`P` Key) | SimConnect `Paused`-Event mit Flag (F5) | wie 3.2.1 | Quiet | += pause_duration | gleiche Session | keine |
| **3.2.3** | MSFS Pause-on-Task | SimConnect `Paused`-Event | wie 3.2.1 | += pause_duration | gleiche Session | keine |
| **3.2.4** | X-Plane `sim/time/paused=1` | X-Plane-Plugin `Paused`-Heartbeat (F6) — heute: Plugin schweigt → 3.1.4-Pfad | wie 3.2.1 (mit F6) bzw. 3.1.4 (ohne F6) | Quiet (mit F6) bzw. wie 3.1.4 | += pause_duration | gleiche Session | keine |
| **3.2.5** | X-Plane Replay-Modus | wie 3.2.4 | wie 3.2.4 | Activity-Log: „Replay-Modus erkannt" | += pause_duration | gleiche Session | keine |
| **3.2.6** | Overnight Esc-Pause (8h, Long-Haul) | wie 3.2.1, lange Dauer | wie 3.2.1 | Quiet (kein Hard-Limit mehr) | += pause_duration (8h) | gleiche Session via `pirep_id`-Join | keine |

### 3.3 Pilot-Aktions-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.3.1** | Manuelle Slew waehrend aktiver Stream (KEINE Pause) | Position-Sprung in zwei aufeinanderfolgenden ACTIVE-Snapshots | ACTIVE bleibt | Toast oder Warning je nach Drift | unveraendert | gleiche Session | keine, aber Drift wird gemeldet |
| **3.3.2** | Falsches Bid geladen (Pilot wechselt versehentlich) | `aircraft_icao` aus Snapshot != `flight.aircraft_icao` (F7) ODER `callsign`/`dep`/`arr`-Aenderung (F8) | ACTIVE bleibt, aber AC-Mismatch-Flag | **Warning-Banner mit zwei Knoepfen:** [Weiter, Bid ignorieren] [PIREP canceln + neues Bid pickn] | unveraendert | gleiche Session bis Cancel | **JA — Pilot muss entscheiden** |
| **3.3.3** | Aircraft-Wechsel mid-flight (Pilot laedt anderes Profil) | wie 3.3.2 nur `aircraft_icao`-Wechsel | wie 3.3.2 | wie 3.3.2 | unveraendert | gleiche Session | JA |
| **3.3.4** | Pilot cancelt PIREP waehrend Pause via phpVMS | phpVMS-API-Poll meldet PIREP cancelled | PAUSED → (Cleanup) | Activity-Log: „PIREP cancelled" | abgeschlossen | Session beendet | keine |

### 3.4 App-Lifecycle-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.4.1** | App killen mid-flight (ACTIVE) | App-Exit, Streamer kill | ACTIVE → (Disk-Persistence) | bei Neustart: ResumeFlightBanner kind=auto_resumed | unveraendert | Session bleibt ACTIVE bis Janitor finalisiert (30 min) | beim Restart: Banner-Confirm |
| **3.4.2** | App killen mid-pause (PAUSED) | wie 3.4.1, im PAUSED-State | PAUSED → (Disk-Persistence) | bei Neustart: ResumeFlightBanner | unveraendert (Akkumulator persistiert!) | Session bleibt | beim Restart: Banner-Confirm, dann Pause-State wieder geladen, Resume-Logik wie bei Online-Resume |
| **3.4.3** | App-Update mid-flight | wie 3.4.1, neuer Binary | wie 3.4.1 | wie 3.4.1 | unveraendert | wie 3.4.1 | wie 3.4.1 |
| **3.4.4** | Windows-Reboot mid-flight | OS killt App, Disk evtl. nicht synced | ResumeFlightBanner kind=discovered (phpVMS-Pfad, weil active_flight.json ggf. weg) | Discover-Adopt-Pfad wie heute | reset auf 0 | Session laeuft weiter via Heartbeat-State | beim Restart: Discover-Banner-Adopt |

### 3.5 Netzwerk-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.5.1** | MQTT-Broker-Disconnect (live.kant.ovh down) | MQTT-Client-Reconnect-Logik | ACTIVE bleibt, MQTT-Posts in Queue | Activity-Log: „Live-Map temporaer offline" | unveraendert | Session pausiert evtl. (Server-Sicht) — `pirep_id`-Join greift bei Reconnect | keine |
| **3.5.2** | phpVMS-Cron killt PIREP trotz Heartbeat | API-Response `flight not found` beim naechsten Post | je nach Stage: Cancel-Flow oder Discover-Adopt | Banner: „PIREP wurde server-seitig beendet — neuer Flug?" | abgeschlossen | nicht mehr verfuegbar | JA — neuen Flug starten oder fortsetzen |
| **3.5.3** | VPN-Switch mid-flight | kurzer Network-Drop | wie 3.5.1 | wie 3.5.1 | unveraendert | wie 3.5.1 | keine |

### 3.6 Long-Haul / Multi-Session-Szenarien

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.6.1** | Multi-Tag-Pause (Flug Freitag, Continue Sonntag) | App-Close + 2 Tage spaeter Open | ResumeFlightBanner | wie 3.4.2 | Akkumulator persistiert | Server-Session evtl. expired (4h `RESUME_WINDOW_MS`) — `pirep_id`-Join (F3) rettet | beim Restart Banner-Confirm |
| **3.6.2** | Wechsel zwischen MSFS Save-Slots mid-flight | Position-Sprung bei naechstem Snapshot | wie 3.3.1 | unveraendert | gleiche Session | keine, aber Drift gemeldet |
| **3.6.3** | Pilot aendert dep/arr in phpVMS (Divert) waehrend Pause | Bid-Polling erkennt geaendertes `arr_airport` | wie 3.3.2 (Bid-Change) | analog F8 | unveraendert | analog F8 | JA — bestaetigen ob Divert oder Bid-Wechsel |

### 3.7 Phase-/Timing-Edge-Cases

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.7.1** | Pause direkt nach Takeoff (Sim-Crash kurz nach Rotation) | wie 3.1.2 | wie 3.1.2 | wie 3.1.2 | += pause_duration | gleiche Session | keine, aber Phase-FSM-Konsistenz pruefen |
| **3.7.2** | Pause vor Takeoff waehrend Boarding | wie 3.1.1 | wie 3.1.1 | wie 3.1.1 | += pause_duration aber Block-Off-Stempel evtl. noch nicht gesetzt → kein Abzug auf flight_time, nur auf block_time | gleiche Session | keine |
| **3.7.3** | Touch-and-Go mit Pause dazwischen | Sim-Pause zwischen Landing und Takeoff | wie 3.2.1 | Quiet | += pause_duration | gleiche Session — kein Phase-Regression-Split, weil LANDING nicht in END_PHASES | keine |
| **3.7.4** | Mehrere Pausen kurz hintereinander (Glitch) | 5x Pause/Resume in 60s | wie 3.1.1 jeweils | letzte unterdrueckt 4 vorige (Toggle-Debounce) | += sum (alle >1s) | gleiche Session | keine |
| **3.7.5** | Pause WAEHREND Disconnect-Resume-Toggle | SimConnect liefert kurz Some, dann wieder None | wie 3.1.1 bleibt PAUSED | Quiet | += pause_duration | gleiche Session | keine |

### 3.8 Mass / Identitaets-Kollisions-Edge-Cases

| # | Szenario | D | T | U | A | S | P |
|---|---|---|---|---|---|---|---|
| **3.8.1** | Pilot startet zweiten ACARS-Client parallel (zwei Tauri-Instanzen) | Disk-Lock-Mechanismus muss greifen | n/a — out of scope, defensive Sperre | Error-Dialog: „AeroACARS laeuft bereits" | n/a | n/a | App schliessen |
| **3.8.2** | Zwei Piloten teilen sich phpVMS-Account, fliegen gleichzeitig | Server-seitige `(va, pilot_id)`-Kollision | n/a — out of scope (Account-Setup-Issue) | n/a | n/a | Sessions kollidieren | n/a |

---

## 4. Architektur-Entscheidungen

### 4.1 `pirep_id` IS THE IDENTITY (BREAKING CHANGE vs. v0.2)

**Entscheidung.** Auto-Resume entscheidet sich ausschliesslich am `pirep_id`-Match. Position, Hoehe, Sprit, on_ground spielen **keine Rolle** fuer die Resume-Entscheidung.

**Begruendung.** Real-World Sim-Restart-Szenarien (Autosave 10 min alt, Spawn am Departure-Airport) produzieren beliebig grosse Position-Drift bei dem es trotzdem derselbe Flug ist. Eine position-basierte Identitaets-Heuristik wuerde diese Faelle blockieren.

**Implikation.** Wenn der Pilot beim Resume merkt „falscher Flug" — z.B. weil er versehentlich ein falsches Bid geladen hat — muss er den PIREP **manuell canceln**. Das System hilft ihm dabei indem es Aircraft-Change (F7) und Bid-Change (F8) **aktiv anzeigt**, aber zwingt keinen Cancel an.

### 4.2 Drift INFORMS, never BLOCKS

**Entscheidung.** Position-/Hoehen-/Sprit-Drift wird gemessen und in UI-Stufen (Quiet/Toast/Warning/ExtremeJump-Banner) angezeigt. Aber kein einziger UI-Pfad blockiert das Auto-Resume.

**UI-Stufen (siehe §6.3):**

| Drift | Reaktion |
|---|---|
| < 1 NM | `Quiet` — nur Activity-Log-Eintrag mit Drift-Wert |
| 1-50 NM | `Toast` — 5s Toast „Position um X NM nachgeladen", klickbar weg |
| 50-200 NM | `Warning` — nicht-blockierender Banner im Cockpit, klickbar weg |
| > 200 NM | `ExtremeJump` — Banner mit Cancel-PIREP-Knopf (aber Auto-Resume hat bereits stattgefunden) |

### 4.3 Pilot HAS ALWAYS A WAY OUT

**Entscheidung.** PIREP-Cancel-UI ist in jedem State zugaenglich. Bei `Warning`/`ExtremeJump` wird der Cancel-Knopf prominent angezeigt. Bei Aircraft-Change (F7) und Bid-Change (F8) wird er aktiv vorgeschlagen.

**Begruendung.** Wenn das System eine falsche Auto-Resume-Entscheidung trifft (oder der Pilot sich anders entscheidet), muss er innerhalb von Sekunden den Weg zurueck haben. Heutiger PIREP-Cancel funktioniert ueber Bid-Tab → out-of-flow.

### 4.4 Heartbeat haelt PIREP unbegrenzt am Leben — kein Hard-Limit auf Pause-Dauer

**Entscheidung.** Kein `AUTO_RESUME_MAX_PAUSE`-Limit. Heartbeat-Codepfad (`lib.rs:10901+`) postet `POST /pireps/{id}/update` waehrend Pause weiter → phpVMS-Cron `RemoveExpiredLiveFlights` greift nie. Server-seitig: `RESUME_WINDOW_MS` bleibt als Fallback nur fuer Edge-Cases wo der Client `pirep_id` nicht mitschickt.

**Konsequenz.** Auch nach 24h Pause (z.B. Multi-Tag-Long-Haul) ist Auto-Resume moeglich, sobald der Pilot zurueckkommt und Daten reinkommen.

**Wichtig.** Der Heartbeat MUSS waehrend Pause weiter feuern. Wenn die Pause-Logik in F1/F5/F6 das versehentlich deaktiviert, wuerde phpVMS den PIREP doch killen → Implementation muss explizit testen dass der Heartbeat-Pfad nicht gebrochen wird (Acceptance A20).

### 4.5 Pause-Akkumulator fuer Block-Time-Korrektur

**Entscheidung.** Bei jeder Pause-Ende-Transition wird die Pause-Dauer in `stats.pause_total_duration: chrono::Duration` akkumuliert. Block-/Flight-Time-Berechner in `lib.rs:14070-14079` zieht diesen Akkumulator vom Roh-Wert ab.

**Persistenz.** `pause_total_duration` UND `pause_segments` (Liste der einzelnen `[start, end]`-Paare) muessen in `save_active_flight` mit serialisiert werden. Mit `#[serde(default)]` rueckwaertskompatibel.

**Alle Pausen werden akkumuliert** — egal ob Sim-Disconnect (F1), MSFS-Pause-Event (F5), X-Plane-Plugin-Heartbeat (F6), oder kuenftiger User-Pause-Button.

### 4.6 SimConnect-Pause-Event fuer MSFS

**Entscheidung.** Neue SimConnect-System-Event-Subscriptions fuer `Paused` + `Unpaused` (analog zu bestehender `SimStart`-Subscription bei `adapter.rs:1495`). Aktualisiert `paused_since` direkt, **ohne** die 30s-Disconnect-Wartezeit.

### 4.7 X-Plane-Plugin-Pause-Heartbeat

**Entscheidung.** X-Plane-Plugin-Protokoll-Extension: statt waehrend `sim/time/paused=1` zu schweigen, sendet das Plugin einen `Paused`-Heartbeat (~1 Hz). Client setzt `paused_reason = SimPause` statt `SimDisconnect`.

**Begruendung.** Heute fallen X-Plane-Pausen in den Disconnect-Pfad und werden erst nach 30s erkannt. Mit `Paused`-Heartbeat: sofortige Pause-Erkennung, sauberere Semantik. Plus: Plugin kann signalisieren wenn der Pilot in den **Replay-Modus** wechselt (eigener Heartbeat-Flag) — eigener Pause-Reason.

**Plugin-Protokoll-Version-Bump.** Neue Plugin-Version macht alle ACARS-Clients <= aktueller Stand abwaerts-kompatibel (Client ignoriert unbekannte Heartbeat-Typen). Forward-only fuer den Plugin-Pause-Pfad — alte Plugin-Versionen fallen weiter in den Disconnect-Pfad.

### 4.8 Aircraft-Change ist das starke „falscher Flug"-Signal (F7)

**Entscheidung.** Wenn ein Snapshot ein anderes `aircraft_icao` meldet als FlightStats kennt → starke Indikation „falsches Bid geladen". Banner zeigt im Cockpit (nicht-blockierend) mit zwei Optionen: [Weiter, ignorieren] oder [PIREP canceln + neues Bid].

**Toleranz.** Wechsel zwischen verwandten Variant-Codes (z.B. `B738` ↔ `B38M` ↔ `B738-MAX`) sollte konfigurierbar sein — manche Pilot-Workflows nutzen verschiedene Aircraft-Profile fuer denselben Flug. Default: striktes Match. Pilot-Owner-Entscheidung in OF11.

### 4.9 Bid-Change-Detection waehrend Pause (F8)

**Entscheidung.** Analog F7, aber fuer `callsign`/`dep`/`arr`. Wenn der Pilot waehrend Pause das phpVMS-Bid wechselt (z.B. neue Route gepickt) und ACARS beim Resume andere Werte sieht: Banner wie F7.

---

## 5. Feature-Pakete

### F1 — Auto-Resume IMMER (BREAKING vs. v0.2)

**Problem.** v0.1/v0.2 hatten harte Toleranz-Schwellen die in Real-World Sim-Restart-Szenarien Auto-Resume verhinderten.

**Loesung.** Auto-Resume passiert **immer** wenn Resume-Trigger feuert. Keine Drift-, Hoehen-, Sprit-, Pause-Dauer-Checks als Blocker. Drift wird gemessen und steuert die UI-Stufe (F2).

**Pseudo-Code:**

```rust
// In streamer-Loop, ersetzt Z. 10955-10968:
let (is_paused, paused_since, paused_last_known_snap, paused_reason) = {
    let stats = flight.stats.lock().expect("flight stats");
    (
        stats.paused_since.is_some(),
        stats.paused_since,
        stats.paused_last_known.clone(),
        stats.paused_reason.clone(),
    )
};

if is_paused {
    let recovery_snapshot = current_snapshot(&app);
    if let Some(snap) = recovery_snapshot {
        let pause_duration = paused_since.map(|t| Utc::now() - t).unwrap_or_default();

        // BREAKING CHANGE v0.3: KEIN Toleranz-Check mehr.
        // Auto-Resume passiert immer. Drift wird gemessen fuer UI.
        let drift = compute_drift(&snap, paused_last_known_snap.as_ref());
        let ui_level = drift.classify();  // Quiet/Toast/Warning/ExtremeJump

        let mut stats = flight.stats.lock().expect("flight stats");
        stats.pause_total_duration = stats.pause_total_duration + pause_duration;
        if let Some(start) = stats.paused_since {
            stats.pause_segments.push(PauseSegment {
                started_at: start,
                ended_at: Utc::now(),
                reason: paused_reason.clone().unwrap_or(PausedReason::SimDisconnect),
            });
        }
        stats.paused_since = None;
        stats.paused_last_known = None;
        stats.paused_reason = None;
        drop(stats);

        // UI-Reaktion an Level (F2)
        emit_resume_ui(&app, ui_level, drift, pause_duration);

        save_active_flight(&app, &flight);
        // fall-through zum normalen Tick mit recovery_snapshot
    } else {
        tokio::time::sleep(Duration::from_secs(5)).await;
        continue;
    }
}
```

**Files affected:**
- `client/src-tauri/src/lib.rs` (Z. 10948-10967 Block, neue `compute_drift`-Funktion, Konstanten)

### F2 — UI-Stufen (Quiet/Toast/Warning/ExtremeJump-Banner)

**Problem.** Aktuell: blockierender Banner bei jeder Pause. v0.2: Toast bei Auto-Resume. v0.3: differenzierte UI-Stufen nach Drift.

**Loesung.** Vier UI-Stufen, gesteuert von `compute_drift().classify()`:

| Stufe | Trigger | Komponente | Lebensdauer | Pilot-Aktion |
|---|---|---|---|---|
| **Quiet** | Drift < 1 NM | Activity-Log-Eintrag, kein UI-Element | persistent im Log | keine |
| **Toast** | 1-50 NM | `ResumeToast`-Component, 5s Anzeige | 5s, dann auto-dismiss | optional Klick (dismiss) |
| **Warning** | 50-200 NM | `ResumeWarningBanner`-Component, persistent bis Klick | persistent | „Verstanden"-Klick |
| **ExtremeJump** | > 200 NM | `ResumeExtremeBanner`-Component, persistent | persistent | „Weiter mit PIREP" oder „PIREP canceln" |

**ExtremeJump-Banner-Text-Vorschlag:**
> „Sehr grosser Positions-Sprung erkannt (X NM, Y Pause-Dauer). Falls dies ein anderer Flug oder ein Fehler ist, kannst du den PIREP jetzt canceln.
> [Weiter mit PIREP] [PIREP canceln + neuen Flug starten]"

Wichtig: Auto-Resume hat **bereits stattgefunden**, der Banner ist nicht-blockierend. Streaming laeuft weiter waehrend der Banner angezeigt wird.

**Files affected:**
- `client/src/components/ResumeToast.tsx` (neu)
- `client/src/components/ResumeWarningBanner.tsx` (neu)
- `client/src/components/ResumeExtremeBanner.tsx` (neu)
- `client/src/locales/{de,en,it}/common.json` — neue i18n-Keys
- `client/src-tauri/src/lib.rs` — neuer Tauri-Event `resume-ui-feedback` mit Level + Drift-Details

### F3 — Server `ensureSession`: `pirep_id` als zusaetzlicher Join-Key + Konstanten relaxen

(Im wesentlichen unveraendert gegenueber v0.2 — siehe v0.2-Doku fuer Details. Hier nur Kurz-Fassung.)

**Aenderungen:**
- Neuer `findSessionByPirepId(va, pilot_id, pirep_id)`-Helper in `recorder/src/db.ts`
- `ensureSession` in `recorder/src/mqttSubscriber.ts` prueft `pirep_id`-Match vor Standard-Pfad
- `RESUME_WINDOW_MS`: 20 min → **4h**
- `LINGERING_TIMEOUT_MS`: 15 min → **30 min**
- Session-Spalte `pirep_id` wird frueh gesetzt (beim ersten Position-Event mit `pirep_id` im Payload), nicht erst beim PIREP-File

**Files affected:**
- `aeroacars-live/recorder/src/mqttSubscriber.ts`
- `aeroacars-live/recorder/src/db.ts`
- `aeroacars-live/recorder/src/index.ts`

### F4 — Pause-Akkumulator + Block-Time-Korrektur

**Problem.** Bei MSFS-Esc-Pause / X-Plane-Pause laeuft die Wall-Clock-Berechnung von `flight_time_secs = now - takeoff_at` weiter — PIREP zeigt Pause-Zeit als geflogen.

**Loesung.** `FlightStats` bekommt `pause_total_duration: chrono::Duration` + `pause_segments: Vec<PauseSegment>`. Block-Time-Berechner zieht ab:

```rust
// HEUTE (lib.rs:14070):
let flight_time_secs = match (stats.takeoff_at, stats.landing_at) {
    (Some(t), Some(l)) if l > t => (l - t).num_seconds().max(0) as i32,
    (Some(t), None) => (now - t).num_seconds().max(0) as i32,
    _ => stats.block_off_at.map(|b| (now - b).num_seconds().max(0) as i32).unwrap_or(0),
};

// MIT F4:
let raw_secs = /* wie heute */;
let pause_secs = stats.pause_total_duration.num_seconds().max(0) as i32;
let flight_time_secs = (raw_secs - pause_secs).max(0);
```

**Persistenz.** `pause_total_duration` + `pause_segments` mit `#[serde(default)]` in `save_active_flight`.

**Files affected:**
- `client/src-tauri/src/lib.rs` (FlightStats-Felder, Block-Time-Berechnung Z. 14070, Persistenz)

### F5 — MSFS-Pause-Detection via SimConnect-Event

**Problem.** MSFS Esc-Pause / Active-Pause werden heute nicht erkannt — SimConnect liefert Frozen-Snapshots.

**Loesung.** Neue SimConnect-System-Event-Subscriptions in `crates/sim-msfs/src/adapter.rs`:

```rust
const PAUSE_EVENT_ID: u32 = 301;
const UNPAUSE_EVENT_ID: u32 = 302;

// adapter.rs:1495 Pattern:
unsafe { sys::SimConnect_SubscribeToSystemEvent(self.handle, PAUSE_EVENT_ID, c"Paused".as_ptr()) };
unsafe { sys::SimConnect_SubscribeToSystemEvent(self.handle, UNPAUSE_EVENT_ID, c"Unpaused".as_ptr()) };

// adapter.rs:1026 Dispatch-arm:
Ok(Some(DispatchMsg::SystemEvent { event_id })) => {
    if event_id == PAUSE_EVENT_ID {
        emit SimPauseEvent::Paused;
    } else if event_id == UNPAUSE_EVENT_ID {
        emit SimPauseEvent::Resumed;
    }
}
```

Im Client-Streamer-Loop:
- `Paused`-Event → `stats.paused_since = Some(now)`, `paused_reason = Some(SimPause)`
- `Unpaused`-Event → Standard-Resume-Pfad (F1) — Drift sollte 0 sein, Auto-Resume Quiet

**Edge-cases:**
- **Disconnect WAEHREND SimPause:** `paused_reason` updated von `SimPause` zu `SimDisconnect`, `paused_since` unveraendert.
- **Toggle-Debounce:** Pause-Dauer < 1s wird ignoriert (kein Akkumulator-Update).
- **Event-Verlust:** Fallback ueber Disconnect-Pfad (Frozen-Snapshots werden eh als „nichts neues" gewertet — aber Block-Time-Drift bleibt unkorrigiert).

**Files affected:**
- `client/src-tauri/crates/sim-msfs/src/adapter.rs`
- `client/src-tauri/crates/sim-msfs/src/lib.rs`
- `client/src-tauri/crates/sim-core/src/lib.rs` (ggf. `SimPauseEvent`-enum)
- `client/src-tauri/src/lib.rs`

### F6 — X-Plane-Plugin-Protokoll-Extension (NEU v0.3)

**Problem.** X-Plane-Plugin schweigt waehrend Pause / Replay → ACARS sieht Disconnect → 30s-Wartezeit + Auto-Resume mit allen Disconnect-Edge-Cases.

**Loesung.** Plugin-Protokoll-Extension: ein zusaetzlicher Heartbeat-Frame waehrend `sim/time/paused=1` oder `sim/time/sim_in_replay=1`. Format:

```c
// xplane-plugin/src/plugin.cpp:347 — vor dem return-Statement:
if (read_int(g_drefs.sim_paused) != 0 || read_int(g_drefs.sim_in_replay) != 0) {
    // v0.3 NEU: Paused-Heartbeat senden statt nur zu schweigen
    static int last_paused_send_s = 0;
    int now_s = static_cast<int>(XPLMGetElapsedTime());
    if (now_s - last_paused_send_s >= 1) {
        send_paused_heartbeat(
            read_int(g_drefs.sim_in_replay) != 0 ? "replay" : "paused"
        );
        last_paused_send_s = now_s;
    }
    return FLIGHT_LOOP_BASE_INTERVAL_S;
}
```

Im X-Plane-Adapter-Client (`crates/sim-xplane/src/adapter.rs`):
- Neuer Packet-Type `PausedHeartbeat { reason: "paused" | "replay" }`
- Beim Empfang: emit `SimPauseEvent::Paused { reason: SimPause | Replay }`
- Bei naechstem normalen Position-Frame: emit `SimPauseEvent::Resumed`

**Plugin-Version-Bump.** Alte Plugin-Versionen (vor v0.3-Plugin) senden keinen `PausedHeartbeat` → Client faellt in den 3.1.4-Pfad (Disconnect-Detection nach 30s). Forward-only, kein Breaking Change.

**Files affected:**
- `xplane-plugin/src/plugin.cpp`
- `xplane-plugin/CMakeLists.txt` (Version-Bump)
- `client/src-tauri/crates/sim-xplane/src/adapter.rs`
- `client/src-tauri/crates/sim-xplane/src/dataref.rs` (entfernt hardcoded `paused: false`)

### F7 — Aircraft-Change-Detection (NEU v0.3)

**Problem.** Pilot laedt versehentlich anderes Flugzeug-Profil. Heute bleibt `flight.aircraft_icao` in FlightStats unveraendert, Sim-Snapshot meldet aber neues ICAO. Fuel-Burn-Rate, MTOW etc. werden falsch berechnet.

**Loesung.** Snapshot-zu-FlightStats-Diff bei jedem Tick. Wenn `snap.aircraft_icao != flight.aircraft_icao` (case-sensitive, mit konfigurierbarer Variant-Toleranz) → `WarningBanner` analog F2:

```
„Anderes Flugzeug erkannt — Bid sagt B738, geladen ist A320.
Falls du das Bid wechseln willst, cancel den aktuellen PIREP.
[B738 ist richtig — A320 ignorieren] [PIREP canceln]"
```

**Variant-Toleranz (OF11):**
- Default: striktes Match.
- Optional: Familie-Match (`B737`-Familie: B736/B737/B738/B739/B38M etc.) — Pilot-Owner-Entscheidung.

**Files affected:**
- `client/src-tauri/src/lib.rs` — `aircraft_icao`-Check im Streamer-Loop, neuer Tauri-Event
- `client/src/components/AircraftChangeBanner.tsx` (neu)

### F8 — Bid-Change-Detection waehrend Pause (NEU v0.3)

**Problem.** Pilot wechselt waehrend laufender Pause das phpVMS-Bid (z.B. neue Route ausgewaehlt). Beim Resume sieht ACARS andere `callsign`/`dep`/`arr` als pre-Pause.

**Loesung.** Bid-Polling-Refresh im Streamer-Loop. Bei Resume aus PAUSED-State zusaetzlich ein Bid-Refresh ausloesen. Wenn die Werte sich geaendert haben → analog F7-Banner:

```
„Bid hat sich geaendert — vorher: AUA 323 LOWW→ESGG, jetzt: AUA 451 LOWW→EDDF.
Falls du den neuen Flug starten willst, cancel den aktuellen PIREP.
[Alten Bid behalten] [PIREP canceln + neuen Flug starten]"
```

**Files affected:**
- `client/src-tauri/src/lib.rs` — Bid-Refresh-Call im Resume-Pfad
- `client/src/components/BidChangeBanner.tsx` (neu)

---

## 6. Datenmodell + Konstanten

### 6.1 FlightStats-Erweiterung

```rust
pub struct FlightStats {
    // ... bestehende Felder ...
    pub paused_since: Option<DateTime<Utc>>,           // bestehend
    pub paused_last_known: Option<PausedSnapshot>,     // bestehend (erweitert §6.2)
    pub paused_reason: Option<PausedReason>,           // NEU v0.2
    pub pause_total_duration: chrono::Duration,        // NEU v0.2 — Default::default()
    pub pause_segments: Vec<PauseSegment>,             // NEU v0.2
}

pub enum PausedReason {
    SimDisconnect,
    SimPause,
    Replay,        // NEU v0.3 (X-Plane Replay-Modus, via F6)
    NetworkLoss,   // NEU v0.3 (MQTT-Broker-Disconnect, via F1 als Sonderfall)
}

pub struct PauseSegment {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub reason: PausedReason,
    pub drift_at_resume: Option<DriftSummary>,  // NEU v0.3 — fuers PIREP-Audit
}

pub struct DriftSummary {
    pub position_drift_nm: f64,
    pub altitude_drift_ft: f64,
    pub fuel_drift_kg: f64,
    pub on_ground_changed: bool,
    pub aircraft_changed: bool,  // NEU v0.3 — F7-Anker
    pub bid_changed: bool,       // NEU v0.3 — F8-Anker
}
```

**Serde:** alle neuen Felder mit `#[serde(default)]`.

### 6.2 PausedSnapshot-Erweiterung

```rust
pub struct PausedSnapshot {
    pub lat: f64,
    pub lon: f64,
    pub heading_deg: f64,
    pub altitude_ft: f64,
    pub fuel_total_kg: f64,
    pub zfw_kg: Option<f64>,
    // NEU v0.2:
    pub on_ground: bool,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    // NEU v0.3:
    pub aircraft_icao: Option<String>,    // fuer F7
    pub callsign: Option<String>,         // fuer F8
    pub dep_airport: Option<String>,      // fuer F8
    pub arr_airport: Option<String>,      // fuer F8
}
```

Alle neuen Felder `#[serde(default)]`.

### 6.3 Drift-Klassifikation (UI-Stufen)

```rust
pub enum DriftLevel {
    /// Drift < 1 NM, < 200 ft, < 100 kg, on_ground identisch
    /// UI: Activity-Log-Eintrag (still)
    Quiet,
    /// Drift 1-50 NM ODER 200-2000 ft ODER 100-500 kg
    /// UI: 5s Toast, auto-dismiss
    Toast,
    /// Drift 50-200 NM ODER 2000-10000 ft ODER 500-2000 kg
    /// UI: persistenter Warning-Banner mit „Verstanden"-Klick
    Warning,
    /// Drift > 200 NM ODER > 10000 ft ODER > 2000 kg ODER on_ground gewechselt mit Position-Drift > 5 NM
    /// UI: persistenter Banner mit Cancel-PIREP-Option
    ExtremeJump,
}

pub fn classify_drift(d: &DriftSummary) -> DriftLevel {
    // ExtremeJump dominiert
    if d.position_drift_nm > 200.0
        || d.altitude_drift_ft > 10000.0
        || d.fuel_drift_kg > 2000.0
        || (d.on_ground_changed && d.position_drift_nm > 5.0)
    {
        return DriftLevel::ExtremeJump;
    }
    if d.position_drift_nm > 50.0
        || d.altitude_drift_ft > 2000.0
        || d.fuel_drift_kg > 500.0
    {
        return DriftLevel::Warning;
    }
    if d.position_drift_nm > 1.0
        || d.altitude_drift_ft > 200.0
        || d.fuel_drift_kg > 100.0
    {
        return DriftLevel::Toast;
    }
    DriftLevel::Quiet
}
```

**Begruendung der Schwellen:**

| Drift-Achse | Quiet | Toast | Warning | ExtremeJump |
|---|---|---|---|---|
| Position | 1 NM | 50 NM | 200 NM | >200 NM |
| Hoehe | 200 ft | 2000 ft | 10000 ft | >10000 ft |
| Sprit | 100 kg | 500 kg | 2000 kg | >2000 kg |

- **Position:** 1 NM = SimConnect-Glitches/AP-Drift. 50 NM = Autosave 6 min alt bei 500 kt. 200 NM = Autosave 25 min alt ODER signifikanter Slew.
- **Hoehe:** 200 ft = Cruise-Oszillation + AP-Recovery. 2000 ft = Phase-Wechsel (Climb-Snapshot). 10000 ft = Climb→Cruise oder Descent→Climb-Sprung.
- **Sprit:** 100 kg = SimConnect-Glitch + Addon-Init. 500 kg = ~3 min Cruise-Burn-Drift. 2000 kg = Profil-Wechsel.

Alle Schwellen sind **Diskussions-Werte** — Pilot-Owner-Justierung in OF12.

### 6.4 SimConnect-Event-Konstanten

```rust
// adapter.rs:55 — analog SIM_START_EVENT_ID:
const PAUSE_EVENT_ID: u32 = 301;
const UNPAUSE_EVENT_ID: u32 = 302;
// Event-Strings: "Paused" + "Unpaused" (siehe MSFS SDK Doku, OF6)
```

### 6.5 X-Plane-Plugin-Protokoll-Erweiterung

Plugin sendet zusaetzlich zum normalen Telemetrie-Frame einen optional `PausedHeartbeat`-Frame waehrend Pause/Replay:

```c
// Format (Vorschlag — Implementor-Detail im plugin.cpp):
struct PausedHeartbeatPacket {
    uint32_t magic;       // 0xAAACAR03 (v0.3)
    uint8_t  packet_type; // 0x10 = PausedHeartbeat
    uint8_t  reason;      // 0x01 = "paused", 0x02 = "replay"
    uint32_t timestamp_ms;
};
```

Frequency: 1 Hz waehrend Pause. Plugin-Version-Bump auf `0xAAACAR03` (heute `0xAAACAR02` o. ae.).

### 6.6 Konstanten (Server)

```typescript
// recorder/src/mqttSubscriber.ts:127
const RESUME_WINDOW_MS = 4 * 60 * 60 * 1000;  // 4h (war 20 min)
// recorder/src/index.ts:179
const LINGERING_TIMEOUT_MS = 30 * 60 * 1000;  // 30 min (war 15 min)
```

---

## 7. Acceptance-Kriterien

### F1 — Auto-Resume IMMER

| # | Kriterium |
|---|---|
| A1 | Snapshot bei Resume mit Drift 0 NM → Auto-Resume, UI-Level `Quiet`, kein Banner |
| A2 | Snapshot mit 30 NM Drift → Auto-Resume, UI-Level `Toast`, 5s Toast erscheint |
| A3 | Snapshot mit 100 NM Drift → Auto-Resume, UI-Level `Warning`, persistenter Banner bis Klick |
| A4 | Snapshot mit 5000 NM Drift (Reload bei Departure) → Auto-Resume, UI-Level `ExtremeJump`, Banner mit Cancel-Option |
| A5 | Pause-Dauer 12h, Drift 0 NM (Esc-Pause) → Auto-Resume, kein Hard-Limit greift |
| A6 | Pause-Dauer 12h, Drift 500 NM (Cancel + Reload) → Auto-Resume `ExtremeJump`, Pilot kann canceln |
| A7 | `paused_last_known` fehlt (Pause durch frueh-Disconnect) → Auto-Resume `Quiet`, kein Drift-Eintrag (nur Pause-Dauer geloggt) |

### F2 — UI-Stufen

| # | Kriterium |
|---|---|
| A8 | `Quiet`-Event triggert keinen UI-Render, nur Activity-Log-Eintrag |
| A9 | `Toast`-Event rendert `ResumeToast`, dismissed nach 5s |
| A10 | `Warning`-Event rendert persistenten Banner, bleibt bis Klick |
| A11 | `ExtremeJump`-Event rendert Banner mit zwei Knoepfen: „Weiter mit PIREP" + „PIREP canceln" |
| A12 | Cancel-PIREP-Klick im ExtremeJump-Banner ruft `flight_cancel`-Tauri-Command auf |

### F3 — Server pirep_id-Join + Konstanten

| # | Kriterium |
|---|---|
| A13 | Zwei Sessions mit gleicher `pirep_id`, Gap >4h → gemerged in eine Session |
| A14 | `RESUME_WINDOW_MS = 4h` aktiv, ENDED-Session 3h alt mit gleichem callsign/dep/arr aber **anderer** `pirep_id` → trotzdem getrennte Sessions (Phase-Regression schuetzt) |
| A15 | `LINGERING_TIMEOUT_MS = 30 min` aktiv, Session 25 min idle bleibt ACTIVE (kein vorzeitiger Janitor-Run) |

### F4 — Pause-Akkumulator + Block-Time

| # | Kriterium |
|---|---|
| A16 | Single Pause 60s, Resume → `pause_total_duration == 60s`, `pause_segments.len() == 1` |
| A17 | Drei Pausen je 20s → Akkumulator = 60s, `pause_segments.len() == 3` |
| A18 | Pause < 1s wird ignoriert (Toggle-Debounce) |
| A19 | Heartbeat-Payload `flight_time` = (now - takeoff_at) - `pause_total_duration` |
| A20 | Heartbeat-Pfad bleibt waehrend Pause aktiv (phpVMS-Cron killt PIREP nicht) |

### F5 — MSFS-Pause-Event

| # | Kriterium |
|---|---|
| A21 | SimConnect `Paused`-Event setzt `paused_since` + `paused_reason=SimPause` |
| A22 | SimConnect `Unpaused`-Event triggert F1-Resume-Pfad, Drift sollte 0 sein → `Quiet` |
| A23 | Disconnect WAEHREND Sim-Pause: `paused_reason` updated auf `SimDisconnect`, Pause-Block bleibt offen |

### F6 — X-Plane-Plugin-Heartbeat

| # | Kriterium |
|---|---|
| A24 | Plugin v0.3-Heartbeat „paused" empfangen → `paused_reason=SimPause` |
| A25 | Plugin v0.3-Heartbeat „replay" empfangen → `paused_reason=Replay` |
| A26 | Plugin < v0.3 (sendet keinen Pause-Heartbeat) → faellt in Disconnect-Pfad (F1) — kein Regression |

### F7 — Aircraft-Change-Detection

| # | Kriterium |
|---|---|
| A27 | Snapshot mit `aircraft_icao=A320` waehrend `flight.aircraft_icao=B738` → Aircraft-Change-Banner |
| A28 | Cancel-Klick im Banner ruft `flight_cancel` auf |
| A29 | „Ignorieren"-Klick im Banner setzt einen Suppression-Flag fuer den aktuellen Flug |

### F8 — Bid-Change-Detection

| # | Kriterium |
|---|---|
| A30 | Bid-Refresh nach Resume erkennt geaenderten `dep_airport` → Bid-Change-Banner |
| A31 | Cancel + Neuer-Flug-Klick im Banner triggert PIREP-Cancel + Bid-Pickup-Flow |

### Allgemein

| # | Kriterium |
|---|---|
| A32 | Bestehende Disk-Resume-Logik (`ResumeFlightBanner kind=auto_resumed`) bleibt unveraendert |
| A33 | App-Restart waehrend Pause: `pause_total_duration` + `pause_segments` aus Disk geladen |
| A34 | Forward-only: laufende Sessions vor Deploy bekommen kein Auto-Resume nachtraeglich |

---

## 8. Files affected

| Datei | Aenderung | Groessenordnung |
|---|---|---|
| `client/src-tauri/src/lib.rs` | `is_paused`-Block (Z. 10948-10967) — F1-Auto-Resume, `compute_drift`-Helper, `DriftLevel`-Enum, FlightStats-Felder, Block-Time-Berechnung (Z. 14070), Aircraft-Change-Check, Bid-Change-Check, Tauri-Events fuer UI-Feedback | ~400 LOC |
| `client/src-tauri/crates/sim-msfs/src/adapter.rs` | 2 neue Event-IDs (Z. 55), 2 neue `SubscribeToSystemEvent`-Calls (Z. 1495), Dispatch-arm-Erweiterung (Z. 1026) | ~30 LOC |
| `client/src-tauri/crates/sim-msfs/src/lib.rs` | `DispatchMsg`/`SimPauseEvent` exportieren | ~10 LOC |
| `client/src-tauri/crates/sim-core/src/lib.rs` | `SimPauseEvent`-enum-Definition | ~15 LOC |
| `client/src-tauri/crates/sim-xplane/src/adapter.rs` | `PausedHeartbeat`-Packet-Type, Emit `SimPauseEvent::Paused/Resumed` | ~40 LOC |
| `client/src-tauri/crates/sim-xplane/src/dataref.rs` | `paused: false` raus, echte Werte einsetzen | ~5 LOC |
| `xplane-plugin/src/plugin.cpp` | `send_paused_heartbeat`-Funktion + Aufruf in `flight_loop_cb:347`, Magic-Version-Bump | ~40 LOC |
| `xplane-plugin/CMakeLists.txt` | Plugin-Version-Bump | trivial |
| `client/src/components/ResumeToast.tsx` | neu | ~60 LOC |
| `client/src/components/ResumeWarningBanner.tsx` | neu | ~80 LOC |
| `client/src/components/ResumeExtremeBanner.tsx` | neu, mit Cancel-Option | ~100 LOC |
| `client/src/components/AircraftChangeBanner.tsx` | neu | ~80 LOC |
| `client/src/components/BidChangeBanner.tsx` | neu | ~80 LOC |
| `client/src/components/ResumeFlightBanner.tsx` | leichter Refactor — koexistiert mit neuen Toasts/Banners | ~30 LOC Aenderung |
| `client/src/locales/{de,en,it}/common.json` | ~15-20 neue i18n-Keys | trivial |
| `aeroacars-live/recorder/src/mqttSubscriber.ts` | `ensureSession`: `pirep_id`-Join-Pfad + Konstante anheben | ~30 LOC |
| `aeroacars-live/recorder/src/db.ts` | `findSessionByPirepId`, `setSessionPirepId` | ~40 LOC |
| `aeroacars-live/recorder/src/index.ts` | `LINGERING_TIMEOUT_MS` | trivial |
| Tests Client | ~20 Unit-Tests (Drift-Klassifikation, Pause-State-Machine, Block-Time-Abzug, Aircraft-/Bid-Change) + 6 SimConnect-/Plugin-Mock-Tests | ~400 LOC |
| Tests Server | 3 Integration-Tests fuer `ensureSession` mit pirep_id-Match | ~80 LOC |

**Gesamt:** ~1500 LOC inkl. Tests. Komplett-Paket.

---

## 9. Test-Plan

### 9.1 Drift-Klassifikation-Unit-Tests (Rust, Tabular-Driven)

| Drift NM | Drift ft | Drift kg | on_ground | Erwartete Stufe |
|---|---|---|---|---|
| 0 | 0 | 0 | unchanged | Quiet |
| 0.5 | 100 | 50 | unchanged | Quiet |
| 1 | 200 | 100 | unchanged | Quiet (Boundary) |
| 1.01 | 100 | 50 | unchanged | Toast |
| 30 | 500 | 200 | unchanged | Toast |
| 50 | 2000 | 500 | unchanged | Toast (Boundary) |
| 51 | 2000 | 500 | unchanged | Warning |
| 150 | 5000 | 1000 | unchanged | Warning |
| 200 | 10000 | 2000 | unchanged | Warning (Boundary) |
| 201 | 10000 | 2000 | unchanged | ExtremeJump |
| 5000 | 35000 | 100 | unchanged | ExtremeJump |
| 6 | 100 | 0 | changed | ExtremeJump |

### 9.2 Pause-State-Machine-Tests

| Setup | Erwartung |
|---|---|
| ACTIVE → SimConnect-Disconnect 30s+ | PAUSED, paused_reason=SimDisconnect |
| ACTIVE → SimConnect-Pause-Event | PAUSED, paused_reason=SimPause |
| ACTIVE → X-Plane-Plugin-Paused-Heartbeat | PAUSED, paused_reason=SimPause |
| PAUSED → SimConnect-Snapshot Some | ACTIVE (via RESUMING) |
| PAUSED → SimConnect-Unpause-Event | ACTIVE (via RESUMING) |
| PAUSED → Disconnect (Pause-Reason wechselt) | PAUSED, paused_reason=SimDisconnect |
| Toggle-Test: 5 Pausen je 500ms | Akkumulator unveraendert (alle < 1s ignoriert) |
| Toggle-Test: 5 Pausen je 2s | Akkumulator = 10s |

### 9.3 Server-Integration-Tests

| Fall | Setup | Erwartung |
|---|---|---|
| `pirep_id`-Match, Gap 5h | Session A endend, 5h spaeter Position mit gleichem `pirep_id` | Session A reopened |
| `pirep_id`-Mismatch | Session A endend, 5h spaeter Position mit anderem `pirep_id` | Neue Session B |
| Phase-Regression-Schutz | Session A=ARRIVED+pirep_filed, neue Position mit Pushback + gleichem callsign | Neue Session B (phase-regression) |

### 9.4 Manuell (Pilot-QS)

Vollstaendiger Pilot-QS-Test-Plan deckt alle 29 Szenarien aus §3 ab. Hier nur Highlights:

| # | Szenario | Erwartung |
|---|---|---|
| Q1 | MSFS Esc-Pause 60s | Quiet Auto-Resume, kein UI |
| Q2 | MSFS-CTD + Continue Flight (Autosave 10 min) | Toast oder Warning Auto-Resume |
| Q3 | MSFS-CTD + Reload Bid bei Departure | ExtremeJump-Banner mit Cancel-Option |
| Q4 | Pilot laedt versehentlich falsches Bid | Aircraft-Change-Banner (F7) |
| Q5 | Overnight Esc-Pause 8h | Quiet Auto-Resume nach Aufwachen |
| Q6 | App killen mid-pause, 24h spaeter wieder oeffnen | ResumeFlightBanner → Resume → Akkumulator persistiert |
| Q7 | X-Plane Esc-Pause (mit altem Plugin) | Disconnect-Pfad nach 30s |
| Q8 | X-Plane Esc-Pause (mit neuem Plugin v0.3) | Sofortige Pause-Erkennung, Quiet Auto-Resume |

---

## 10. Phasen

| Phase | Inhalt | Estimate |
|---|---|---|
| 0 | FlightStats-Erweiterung, PausedSnapshot-Erweiterung, DriftLevel-Enum, Konstanten, Helper-Skelette | 3h |
| 1 | F1: `is_paused`-Block-Refactor, `compute_drift`, Drift-Klassifikation mit Unit-Tests | 4h |
| 2 | F4: Block-Time-Berechnung in `lib.rs:14070` mit Unit-Tests, Persistenz | 3h |
| 3 | F5: SimConnect-Pause-Event-Subscription + Dispatch-arm + Streamer-Handler | 4h |
| 4 | F2: UI-Stufen (Toast/Warning/ExtremeJump-Komponenten + i18n) | 4h |
| 5 | F7: Aircraft-Change-Detection + Banner | 3h |
| 6 | F8: Bid-Change-Detection + Bid-Refresh-Logik + Banner | 3h |
| 7 | F3: Server-Updates (`ensureSession`, Konstanten, DB-Helper) | 3h |
| 8 | F6: X-Plane-Plugin-Heartbeat-Extension + Plugin-Build + Client-Adapter | 4h |
| 9 | Tests (Unit + SimConnect-Mock + Server-Integration) | 4h |
| 10 | Pilot-QS (29 Szenarien) | 3 Wochen Real-World |

**Gesamt:** ~35h Code + Tests + 3 Wochen Pilot-Testing.

---

## 11. Risiken + Mitigation

| Risiko | Wahrscheinlichkeit | Impact | Mitigation |
|---|---|---|---|
| Pilot uebersieht ExtremeJump-Banner, falscher Flug bleibt mit PIREP verknuepft | Niedrig | Mittel | Banner ist persistent + prominent, Activity-Log haelt fuers spaetere Audit |
| Aircraft-Change-Detection (F7) bei legitim verwandten Variants (B738↔B38M) zu strikt → Pilot-Frust | Mittel | Niedrig | Variant-Toleranz-Config (OF11), Default kann strict bleiben, Pilot kann lockern |
| `pirep_id`-Migration: bestehende Records ohne Feld | Niedrig | Niedrig | `#[serde(default)]`, forward-only Logik |
| F3 ohne Client-`pirep_id`-Payload: Server-Patch wirkungslos | Mittel | Niedrig | Verifikations-Schritt in Phase 7.0 |
| SimConnect-Pause-Event-Unzuverlaessigkeit (MSFS-Version-Bugs) | Mittel | Niedrig | Disconnect-Fallback bleibt aktiv (F1 funktioniert ohne F5) |
| Block-Time-Abzug-Bug → PIREP-`flight_time_min` falsch | Niedrig (mit Unit-Tests) | Mittel | Tabular-Tests fuer alle Pause-Konfigurationen |
| Pause-Akkumulator-Persistenz-Bug → bei App-Restart Pause-Zeit verloren | Mittel | Mittel | Integration-Test der `save_active_flight` / Load-Roundtrip |
| Heartbeat waehrend Pause versehentlich deaktiviert | Niedrig (Code-Review) | Hoch (PIREP wird gekillt) | Expliziter Test A20 |
| X-Plane-Plugin-Version-Bump: alte Clients mit neuem Plugin → Disconnect-Pfad-Fallback ist transparent | Niedrig | Niedrig | Forward-only, magic-number-Check |
| Cancel-PIREP-Flow ueber Banner: Pilot canceled versehentlich | Niedrig (Confirm-Dialog) | Mittel | Zwei-Stufen-Confirm: Banner-Klick → ConfirmDialog „PIREP wirklich canceln?" |

---

## 12. Offene Fragen (v0.3 Stand)

| # | Frage | Verantwortlich |
|---|---|---|
| OF1 | Sollen die `DriftLevel`-Schwellen (1/50/200 NM) Phase-abhaengig sein? Z.B. Approach/Final stricter weil Touchdown-Forensik dort sensibel | Pilot-Owner |
| OF2 | F7-Variant-Toleranz: strict match oder family match (B737-Familie)? | Pilot-Owner |
| OF3 | F7 bei Aircraft-Change: soll Aircraft-Mismatch im PIREP-Payload exposed werden (z.B. fuer Server-/Web-Anzeige)? | Server-Owner |
| OF4 | F8 Bid-Change-Detection: wie haeufig Bid-Refresh? Bei jedem Resume? Periodisch? | Pilot-Owner |
| OF5 | F6 X-Plane-Plugin-Heartbeat-Frequenz: 1 Hz reicht? Oder lieber 0.5 Hz fuer weniger Network-Noise? | Plugin-Owner |
| OF6 | F5 SimConnect: `"Paused" + "Unpaused"` Events vs. `"Pause_EX1"` flagged-State — welche SDK-Variante? Vorschlag: simple zwei-Event-Variante | Implementor |
| OF7 | Block-Time-Abzug bei Pre-Flight/Taxi-Pausen: alle abziehen, oder nur airborne-Pausen? Vorschlag: alle | Pilot-Owner |
| OF8 | `pause_segments` ins PIREP-Payload (Server-/Web-Anzeige „3 Pausen, Total 8 min")? | Server-Owner |
| OF9 | ExtremeJump-Banner-Schwelle (>200 NM): zu locker fuer kuerzer Strecken (z.B. 100 NM-Flug bei dem 200 NM Drift = doppelte Strecke)? Adaptive Schwelle relative zur Flugstrecke? | Pilot-Owner |
| OF10 | F2: sollen Toast und Warning auf der **Karte** zusaetzlich eine gestrichelte Drift-Linie zeigen? | UI-Owner |
| OF11 | F7 Aircraft-Variant-Familien: konkrete Definition? z.B. ICAO Variants `Bxxx` → Familie nach erstem stem? | Pilot-Owner |
| OF12 | Drift-Klassifikations-Schwellen (§6.3): sind 1/50/200 NM realistic? Pilot-QS muss zeigen ob die Stufen sinnvoll greifen | Pilot-Owner |
| OF13 | Soll der MQTT-Broker-Disconnect (3.5.1) auch als `PausedReason::NetworkLoss` getrackt werden, oder nur intern abgefangen? | Server-Owner |
| OF14 | Plugin-Version-Bump (F6): wann releasen? Vorher / parallel zur Client-v0.3? | Plugin-Owner |
| OF15 | Soll Suppress-Flag bei „Ignorieren"-Klick im Aircraft-Change-Banner per-Flug oder global persistiert werden? | Pilot-Owner |

---

## 13. Approval-Status

| Version | Datum | Reviewer | Status |
|---|---|---|---|
| v0.1 DRAFT | 2026-05-11 | Spec-Autor (Claude) | Initial Draft — **OBSOLETE** durch v0.3 |
| v0.2 DRAFT | 2026-05-11 | Spec-Autor (Claude) | Erweiterung um Sim-Pause-Detection (F5), Zeiten angepasst — **OBSOLETE** durch v0.3 |
| **v0.3 DRAFT** | 2026-05-12 | Spec-Autor (Claude) | BREAKING CHANGE: Toleranzen verworfen, `pirep_id` als Identitaet, Szenario-Matrix, F6/F7/F8 ergaenzt — weiterhin DEFERRED |
| — | — | Pilot-Owner (Thomas K) | „Toleranzen muessen raus" (2026-05-12) — wartet auf finale Review |

---

## 14. Trigger-Daten (fuer spaeteren Review)

**PIREP `J2VoaZmoD6LQGpMg`** — vollstaendiger JSONL-Log im Client-Log-Upload verfuegbar. Sessions im Server-Snapshot `aeroacars-live-snapshot-20260510.db` NICHT enthalten (Snapshot ist vom Vortag); nach 2026-05-11 muessten zwei Session-Zeilen im Live-DB existieren:

- Session A: `callsign='AUA 323'`, `dep='LOWW'`, `arr='ESGG'`, `started_at ≈ 11:02:58`, `ended_at ≈ 12:38:34` (Janitor-finalisiert), `last_phase='DESCENT'`, `pirep_id=NULL`, `position_count=2062`
- Session B: gleiche Identifiers, `started_at ≈ 13:02:14`, `ended_at ≈ 13:04:26`, `last_phase='ARRIVED'`, `pirep_id='J2VoaZmoD6LQGpMg'`, `position_count=124`

Diese zwei Zeilen sind der **konkrete forensische Anker** fuer die Bewertung — wenn die Spec irgendwann reviewed wird, sollte der Reviewer beide Sessions in der Live-DB inspizieren koennen.

---

## Anhang A — Verifizierte Code-Anker

Pre-Implementation muss der Entwickler verifizieren dass diese Anker noch existieren (Drift-Schutz):

| Anker | Datei | Zweck |
|---|---|---|
| `lib.rs:10948-10967` | `client/src-tauri/src/lib.rs` | `is_paused`-Block im Streamer-Loop — F1 ersetzt komplett |
| `lib.rs:10970-11020` | `client/src-tauri/src/lib.rs` | Sim-Disconnect-Detection (`snapshot.is_some()` Check) — bleibt unveraendert |
| `lib.rs:14070-14079` | `client/src-tauri/src/lib.rs` | Block-Time-Berechnung — F4 fuegt Pause-Abzug hier ein |
| `lib.rs:1391-1414` | `client/src-tauri/src/lib.rs` | FlightStats-Definition mit `paused_since`/`paused_last_known` — erweitert in §6.1 |
| `lib.rs:10901+` | `client/src-tauri/src/lib.rs` | Heartbeat-Codepfad — darf NICHT gebrochen werden (Acceptance A20) |
| `adapter.rs:55` | `client/src-tauri/crates/sim-msfs/src/adapter.rs` | `SIM_START_EVENT_ID = 300` — F5 fuegt 301/302 daneben |
| `adapter.rs:1495-1505` | `client/src-tauri/crates/sim-msfs/src/adapter.rs` | `SimConnect_SubscribeToSystemEvent("SimStart")` — F5 ergaenzt Pause/Unpaused-Subscriptions |
| `adapter.rs:1026-1029` | `client/src-tauri/crates/sim-msfs/src/adapter.rs` | Dispatch-`SystemEvent`-arm — F5 erweitert um Pause-/Unpause-Branches |
| `plugin.cpp:343-349` | `xplane-plugin/src/plugin.cpp` | `sim_paused` / `sim_in_replay` DataRef-Check — F6 fuegt `send_paused_heartbeat` davor ein |
| `plugin.cpp:648` | `xplane-plugin/src/plugin.cpp` | `find_ref("sim/time/paused")` — F6 nutzt die bestehende DataRef |
| `dataref.rs:716` | `client/src-tauri/crates/sim-xplane/src/dataref.rs` | `paused: false` hardcoded — F6 ersetzt mit echten Werten |
| `mqttSubscriber.ts:127` | `aeroacars-live/recorder/src/mqttSubscriber.ts` | `RESUME_WINDOW_MS` — F3 hebt auf 4h |
| `mqttSubscriber.ts:206-291` | `aeroacars-live/recorder/src/mqttSubscriber.ts` | `ensureSession` — F3 fuegt pirep_id-Join vor Z. 217 ein |
| `db.ts:1115-1140` | `aeroacars-live/recorder/src/db.ts` | `findActiveSession` — Vorbild fuer `findSessionByPirepId` |
| `db.ts:1165-1171` | `aeroacars-live/recorder/src/db.ts` | `endSession` — F3 setzt `pirep_id` frueher |
| `index.ts:179` | `aeroacars-live/recorder/src/index.ts` | `LINGERING_TIMEOUT_MS` — F3 hebt auf 30 min |
| `ResumeFlightBanner.tsx:7` | `client/src/components/ResumeFlightBanner.tsx` | `COUNTDOWN_SECONDS = 30` — bleibt unveraendert (Disk-Resume-Pfad) |

---

## Anhang B — Pause-State-Machine-Diagramm

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                     │
│  ┌────────┐                                                         │
│  │ ACTIVE │ ◀──────────────────────────────────────────────┐        │
│  └───┬────┘                                                │        │
│      │                                                     │        │
│      │   ┌──────────────────────────┐                      │        │
│      ├──▶│ SimConnect-Snap=None>30s │──┐                   │        │
│      │   └──────────────────────────┘  │                   │        │
│      │   ┌──────────────────────────┐  │   reason =        │        │
│      ├──▶│ SimConnect Paused-Event  │──┤   SimDisconnect   │        │
│      │   └──────────────────────────┘  │   | SimPause      │        │
│      │   ┌──────────────────────────┐  │   | Replay        │        │
│      └──▶│ X-Plane Paused-Heartbeat │──┘                   │        │
│          └──────────────────────────┘                      │        │
│                          │                                 │        │
│                          ▼                                 │        │
│                     ┌────────┐                             │        │
│                     │ PAUSED │                             │        │
│                     └───┬────┘                             │        │
│                         │                                  │        │
│                         │   ┌──────────────────────────┐   │        │
│                         ├──▶│ SimConnect-Snap=Some     │──┐│        │
│                         │   └──────────────────────────┘  ││        │
│                         │   ┌──────────────────────────┐  ││        │
│                         ├──▶│ SimConnect Unpause-Event │──┤│        │
│                         │   └──────────────────────────┘  ││        │
│                         │   ┌──────────────────────────┐  ││        │
│                         ├──▶│ Plugin Resumed-Heartbeat │──┤│        │
│                         │   └──────────────────────────┘  ││        │
│                         │   ┌──────────────────────────┐  ││        │
│                         └──▶│ User-Klick Resume-Banner │──┘│        │
│                             └──────────────────────────┘   │        │
│                                                            │        │
│                                  ▼                         │        │
│                            ┌──────────┐                    │        │
│                            │ RESUMING │ (1-Tick-Transition)│        │
│                            └────┬─────┘                    │        │
│                                 │                          │        │
│                                 │   - Pause-Block close    │        │
│                                 │   - Akkumulator update   │        │
│                                 │   - pause_segments.push  │        │
│                                 │   - Drift compute        │        │
│                                 │   - UI-Level classify    │        │
│                                 │   - Emit UI-Event        │        │
│                                 └──────────────────────────┘        │
│                                                                     │
│  Out-of-Loop (jederzeit aus PAUSED):                                │
│    ─▶ User cancelt PIREP via UI → Session beendet, kein Resume      │
│    ─▶ phpVMS-Cron killt PIREP (sehr selten, Heartbeat sollte das    │
│       verhindern) → Discover-Resume-Pfad via ResumeFlightBanner     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

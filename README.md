# AeroACARS

> Modern, open-source ACARS client for [phpVMS 7](https://phpvms.net) — Tauri 2 + Rust + React.
> Made with ❤️ in Gifhorn — by Thomas Kant.

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows-blue.svg)](#installation)
[![phpVMS 7](https://img.shields.io/badge/phpVMS-7-orange.svg)](https://phpvms.net)

---

## Was ist AeroACARS?

Ein moderner, plattformübergreifender ACARS-Client für phpVMS 7. Erfasst
Telemetrie aus Flight Simulators, scort Landungen mit industrie-validierten
Schwellen, korreliert Touchdowns auf Runway-Centerline-Genauigkeit und
shippt saubere PIREPs zu deinem phpVMS-Server.

**Aktuell unterstützt:**

- ✅ **MSFS 2020 / MSFS 2024** — über raw SimConnect FFI (Windows-only,
  kein FSUIPC nötig)
- ✅ **X-Plane 11 / X-Plane 12** — über native UDP DataRefs
  (cross-platform, kein Plugin nötig)

**Status:** Pre-Beta. Diese Version ist auf den phpVMS-Host
`german-sky-group.eu` hardcoded — andere VAs können den Code forken
und den Host in `client/src-tauri/src/lib.rs` (`ALLOWED_PHPVMS_HOST`)
anpassen.

---

## Installation

1. [Latest Release](https://github.com/MANFahrer-GF/AeroACARS/releases/latest) herunterladen
2. `AeroACARS_<version>_x64-setup.exe` ausführen
3. SmartScreen-Warnung wegklicken („Weitere Informationen" → „Trotzdem ausführen") — wir sind noch nicht code-signed
4. AeroACARS startet automatisch. Login mit deinem phpVMS-API-Key.

Auto-Updates kommen ab v0.1.0+ direkt in der App.

---

## Was kann AeroACARS?

### Live-Telemetrie + Flugverfolgung
- Phase-Detection-FSM (16 Phasen: Boarding → Pushback → TaxiOut → Takeoff → Climb → Cruise → Descent → Approach → Final → Landing → TaxiIn → BlocksOn → Arrived → PIREP)
- Position-Streaming an phpVMS mit phasen-adaptiver Cadence
- Offline-Queue für Position-Posts wenn das Netzwerk wegbricht

### Touchdown-Analyse (industriegrade)
- 50 Hz Sampling (matches GEES, höher als MSFS' default)
- V/S-Capture aus latched SimVar (MSFS) oder Buffer-Min ±250 ms (GEES-Pattern)
- Peak-G im 800-ms-Fenster nach Aufprall (Strut-Rebound ausgeschlossen)
- AGL-basierte Bounce-Detection (35→5 ft, BeatMyLanding-aligned)
- Native Sideslip aus VEL_BODY_X/Z (`atan2`)
- Headwind/Crosswind aus airframe-relativen Wind-Komponenten
- Score-Schwellen aus Boeing 737 FCOM, Airbus A320 FCOM, LH FOQA, vmsACARS-Defaults

### Runway-Korrelation
- OurAirports.com Runway-Datensatz (47.681 Bahnen, 4 MB) embedded
- Touchdown-Lat/Lon → exakte Runway + Centerline-Distance + Threshold-Distance

### PIREP-Submission
- Voller Notes-Block (TIMES / TOUCHDOWN / RUNWAY / FUEL / DISTANCE / METAR)
- ~40 Custom Fields (Title-Case + snake_case für Leaderboards)
- Auto-File bei `Arrived`, mit manueller Override-Option
- Bid-Delete via korrektem `/api/user/bids` Endpoint

### Comfort-Features
- Auto-Start-Watcher: Aufzeichnung beginnt automatisch wenn Aircraft am Bid-Departure-Airport steht
- Persistente Activity-Log mit Crash-Recovery (per-Flight reset)
- Live-Sim-Inspector im Debug-Modus (MSFS SimVars/LVars + X-Plane DataRefs)
- METAR-Snapshots Dep/Arr automatisch beim Takeoff/Final

---

## Tech-Stack

- **Backend:** Rust (Tauri 2, raw SimConnect FFI für MSFS, std::net für X-Plane UDP)
- **Frontend:** React 19 + TypeScript + Vite
- **Persistence:** OS-Keyring für API-Keys, JSON-Sidecars für Activity-Log + Active-Flight-State
- **Updater:** Tauri-Plugin-Updater mit Ed25519-Signatur, GitHub Releases als Source

---

## Schultern, auf denen AeroACARS steht

- **OurAirports** — Public-domain Runway-Datensatz
- **BeatMyLanding** — Touchdown-Window-Calibration und Bounce-Detection-Pattern
- **GEES** — Open-Source-Landingrate-Logger; reverse-engineered für V/S-Sign-Convention und native Sideslip-Berechnung
- **LandingToast** — Live-VS-at-OnGround-Edge-Pattern
- **Tauri 2 + Rust + React** — App-Framework
- **MSFS SDK + X-Plane SDK** — Sim-Integration

---

## Entwicklung

```bash
# Voraussetzung: Rust toolchain, Node.js 20+, ggf. MSFS 2024 SDK für sim-msfs build
git clone https://github.com/MANFahrer-GF/AeroACARS.git
cd AeroACARS/client
npm install
npm run tauri dev          # Dev-Mode mit Hot-Reload
npm run tauri build -- --bundles nsis   # Release-Installer bauen
```

---

## License

MIT — siehe [LICENSE](LICENSE).

---

**Contact:** Thomas Kant · German Sky Group · [github.com/MANFahrer-GF](https://github.com/MANFahrer-GF)

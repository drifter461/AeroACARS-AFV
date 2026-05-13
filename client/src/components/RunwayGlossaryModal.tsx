// Glossar-Modal für RunwayDiagramV2.
// Spec: docs/spec/runway-diagram-v2.contract.md §Glossar (17 Begriffe).
// Accessible: ESC schließt, Focus-Trap auf Modal, role="dialog".

import { useEffect, useRef } from "react";

interface GlossaryEntry {
  abbr: string;
  full: string;
  explanation: string;
}

const ENTRIES: GlossaryEntry[] = [
  {
    abbr: "Threshold (THR)",
    full: "Bahnschwelle",
    explanation:
      "Die großen weißen Querstreifen am Bahnanfang. Ab dieser Linie darfst du landen.",
  },
  {
    abbr: "Touchdown (TD)",
    full: "Aufsetzen",
    explanation: "Der Moment, in dem die Räder den Bahnbelag berühren.",
  },
  {
    abbr: "Centerline (CL)",
    full: "Mittellinie",
    explanation: "Die gestrichelte weiße Linie genau in der Mitte der Bahn.",
  },
  {
    abbr: "Centerline-Offset / XTD",
    full: "Seitenabweichung",
    explanation:
      "Wie weit links oder rechts von der Mittellinie bist du aufgesetzt? Idealwert: 0 m.",
  },
  {
    abbr: "TDZ — Touchdown Zone",
    full: "Aufsetzzone",
    explanation:
      "Der Soll-Bereich zum Aufsetzen — proportional zur Bahnlänge, gedeckelt bei 900 m. Formel im Code: TDZ-Länge = min(Bahnlänge ÷ 3, 900 m). Heißt: bei einer 1500-m-Bahn ist die TDZ 500 m lang, bei 2100 m sind es 700 m, ab 2700 m Bahnlänge greift der 900-m-Cap — egal wie lang die Bahn dann noch wird. Auf echten Bahnen siehst du die TDZ als Gruppen weißer Querstreifen entlang der Centerline (ICAO Annex 14). Beispiel: LIEE 2803 m → 900 m TDZ (Cap), EDDR 1320 m → 440 m TDZ.",
  },
  {
    abbr: "AIM — Aim Point",
    full: "Ziel-Markierung",
    explanation:
      "ZWEI breite weiße Streifen auf der Bahn — einer direkt OBERHALB, einer direkt UNTERHALB der Mittellinie, symmetrisch (ICAO Annex 14 §5.2.6). Im stabilisierten Anflug zielt dein Blick GENAU dort hin, weil der 3°-Glideslope dich exakt zu diesem Punkt führen würde, wenn du nicht abfangen (flaren) würdest. Beim Flare hebst du die Nase, drosselst — und setzt typisch 50–150 m HINTER dem Aim-Point auf (= Anfang der TDZ). POSITION (AeroACARS 2-Bucket-Logik, FAA AIM 8-9-1): Bahn ≥ 2400 m → Aim-Point bei 400 m hinter der Schwelle; Bahn < 2400 m → Aim-Point bei 300 m hinter der Schwelle. Beispiel: LIEE 2803 m → 400 m, EDDR 1320 m → 300 m. ICAO Annex 14 hätte feiner gestaffelt (150/250/300/400 m je nach LDA), die FAA-Vereinfachung mit 300/400 reicht aber für die Bewertung. Streifen-Länge auf der echten Bahn: 30–60 m.",
  },
  {
    abbr: "TCH — Threshold Crossing Height",
    full: "Schwellen-Überflug-Höhe",
    explanation:
      "Wie hoch warst du über dem Boden, als du die Schwelle überflogen hast? ILS-Anflug typisch 49 ft (≈ 15 m). Zu niedrig: Tail-Strike-Risiko. Zu hoch: Long-Landing.",
  },
  {
    abbr: "DDS — Displaced Threshold",
    full: "Versetzte Schwelle",
    explanation:
      "Manche Bahnen haben einen Bereich VOR der echten Landeschwelle, der für die Landung verboten ist (Pfeile auf der Bahn). Aufsetzen davor = illegal. Beispiel: OLBA RWY 35, 820 m DDS.",
  },
  {
    abbr: "Glide Slope",
    full: "Anflug-Winkel",
    explanation: "ILS-Standard 3°. Du sinkst 1 m für je 19 m vorwärts.",
  },
  {
    abbr: "Bremspunkt (im Diagramm orange)",
    full: "40-kt-Punkt",
    explanation:
      "Der orange Kreis im Diagramm markiert die Stelle, an der die Groundspeed während des Ausrollens unter ~40 kt gefallen ist (= ROLLOUT_STOP_GS_KT). Das ist KEINE konkrete Stelle wo der Pilot abbiegt — die echte Abzweigung passiert später an einem konkreten Taxiway. Sondern: ab diesem Punkt bist du langsam genug für einen normalen High-Speed-Exit, und ab hier wird auch das Diagramm das nicht mehr genutzte Bahn-Stück als 'verbleibend' markieren.",
  },
  {
    abbr: "Rollout",
    full: "Ausrollstrecke",
    explanation:
      "Wie viele Meter rollst du nach dem Aufsetzen, bis du auf ~40 kt abgebremst hast — das ist die typische High-Speed-Exit-Geschwindigkeit, mit der du am nächsten Rollwege-Abzweig die Bahn verlässt. Bis zum vollen Stand auf der Bahn rollt fast niemand aus (das wäre verschwendete Bahn).",
  },
  {
    abbr: "Bahn-Auslastung",
    full: "",
    explanation: "Ausrollstrecke ÷ Bahnlänge × 100 %. 80 % = nur 20 % Bahn übrig (knapp).",
  },
  {
    abbr: "AIRAC-Cycle",
    full: "",
    explanation:
      'Offizielle Aviation-Daten werden alle 28 Tage aktualisiert. „Cycle 2604" = 4. Update 2026.',
  },
  {
    abbr: "VPS Navdata",
    full: "",
    explanation:
      "Zentrale, vom VA-Admin gepflegte AIRAC-Daten auf dem VPS. Pilot-Client zieht sie pro Flugstart. Technische Quelle dahinter: Aerosoft DFD (Lizenz: VA-Admin-Subscription).",
  },
  {
    abbr: "OurAirports",
    full: "",
    explanation:
      "Community-Wiki-Datenquelle als Fallback wenn der VPS nicht erreichbar ist. Schwellen-Positionen können abweichen.",
  },
  {
    abbr: "AGL",
    full: "Above Ground Level",
    explanation: "Höhe über Grund (nicht über Meer).",
  },
  {
    abbr: "fpm",
    full: "Feet per Minute",
    explanation: "Sinkrate-Einheit. Negativ = Sinkflug.",
  },
  {
    abbr: "kt",
    full: "Knots / Knoten",
    explanation: "Geschwindigkeitseinheit, ≈ 1.852 km/h.",
  },
];

export function GlossaryModal({ onClose }: { onClose: () => void }) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeBtnRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const root = dialogRef.current;
      if (!root) return;
      const focusables = root.querySelectorAll<HTMLElement>(
        'button, [href], input, [tabindex]:not([tabindex="-1"])',
      );
      if (focusables.length === 0) return;
      const first = focusables[0]!;
      const last = focusables[focusables.length - 1]!;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      onClick={onClose}
      role="presentation"
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.65)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 10000,
        padding: 16,
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="rwy-glossary-title"
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "#111827",
          border: "1px solid rgba(255,255,255,0.18)",
          borderRadius: 10,
          maxWidth: 760,
          width: "100%",
          maxHeight: "85vh",
          display: "flex",
          flexDirection: "column",
          boxShadow: "0 20px 60px rgba(0,0,0,0.6)",
        }}
      >
        <header
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "14px 18px",
            borderBottom: "1px solid rgba(255,255,255,0.10)",
          }}
        >
          <h3 id="rwy-glossary-title" style={{ margin: 0, fontSize: "1.1rem" }}>
            🛬 Begriffe in der Landebahn-Analyse
          </h3>
          <button
            ref={closeBtnRef}
            type="button"
            onClick={onClose}
            aria-label="Glossar schließen"
            style={{
              padding: "4px 12px",
              background: "rgba(255,255,255,0.08)",
              border: "1px solid rgba(255,255,255,0.18)",
              borderRadius: 6,
              color: "inherit",
              cursor: "pointer",
            }}
          >
            Schließen ✕
          </button>
        </header>
        <div
          style={{
            padding: "12px 18px 18px 18px",
            overflowY: "auto",
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          <p style={{ margin: 0, opacity: 0.75, fontSize: "0.88rem" }}>
            Kurzerklärung aller Begriffe und Abkürzungen, die im Diagramm und in
            den Detail-Karten auftauchen — in einfacher Sprache.
          </p>
          {ENTRIES.map((e) => (
            <div
              key={e.abbr}
              style={{
                background: "rgba(255,255,255,0.04)",
                border: "1px solid rgba(255,255,255,0.08)",
                borderRadius: 6,
                padding: "10px 12px",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "baseline",
                  gap: 8,
                  flexWrap: "wrap",
                  marginBottom: 4,
                }}
              >
                <strong style={{ fontSize: "0.98rem" }}>{e.abbr}</strong>
                {e.full && (
                  <span style={{ opacity: 0.6, fontSize: "0.85rem" }}>
                    — {e.full}
                  </span>
                )}
              </div>
              <div style={{ fontSize: "0.9rem", lineHeight: 1.5, opacity: 0.92 }}>
                {e.explanation}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

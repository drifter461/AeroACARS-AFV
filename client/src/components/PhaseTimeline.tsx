import { useTranslation } from "react-i18next";
import type { FlightPhase } from "../types";

/**
 * Visual timeline of the major flight phases with a small SVG plane
 * that glides between checkpoints as the FSM advances. We don't render
 * every FSM phase as its own checkpoint — that's eleven and would be
 * cluttered — instead we collapse them into seven major checkpoints
 * the pilot recognises from the flight plan.
 *
 * The plane's `transform: translateX(...)` is CSS-transitioned so the
 * jump between phases looks like the aircraft taxiing / accelerating
 * forward instead of teleporting.
 */
interface Props {
  phase: FlightPhase;
}

interface Checkpoint {
  key: string;
  fsm: FlightPhase[];
}

const CHECKPOINTS: Checkpoint[] = [
  { key: "boarding", fsm: ["preflight", "boarding"] },
  { key: "taxi_out", fsm: ["pushback", "taxi_out"] },
  { key: "takeoff", fsm: ["takeoff_roll", "takeoff"] },
  { key: "cruise", fsm: ["climb", "cruise"] },
  { key: "approach", fsm: ["descent", "approach", "final"] },
  { key: "landing", fsm: ["landing", "taxi_in"] },
  { key: "arrived", fsm: ["blocks_on", "arrived", "pirep_submitted"] },
];

function activeIndex(phase: FlightPhase): number {
  for (let i = 0; i < CHECKPOINTS.length; i++) {
    if (CHECKPOINTS[i]!.fsm.includes(phase)) return i;
  }
  return 0;
}

export function PhaseTimeline({ phase }: Props) {
  const { t } = useTranslation();
  const current = activeIndex(phase);
  const lastIndex = CHECKPOINTS.length - 1;
  // 0..1 — the plane sits exactly on the active checkpoint, smoothly
  // transitioned in CSS so the move between phases feels alive.
  const progress = lastIndex === 0 ? 0 : current / lastIndex;

  return (
    <div className="phase-timeline" aria-label={t("phase_timeline.title")}>
      <div className="phase-timeline__track">
        <div
          className="phase-timeline__track-fill"
          style={{ width: `${progress * 100}%` }}
        />
        {CHECKPOINTS.map((cp, i) => {
          const reached = i <= current;
          return (
            <div
              key={cp.key}
              className={`phase-timeline__node ${
                i < current ? "phase-timeline__node--past" : ""
              } ${i === current ? "phase-timeline__node--current" : ""} ${
                reached ? "phase-timeline__node--reached" : ""
              }`}
              style={{ left: `${(i / lastIndex) * 100}%` }}
            >
              <span className="phase-timeline__dot" />
              <span className="phase-timeline__label">
                {t(`phase_timeline.nodes.${cp.key}`)}
              </span>
            </div>
          );
        })}
        <div
          className="phase-timeline__plane"
          style={{ left: `${progress * 100}%` }}
          aria-hidden="true"
        >
          <PlaneIcon />
        </div>
      </div>
    </div>
  );
}

function PlaneIcon() {
  // Inline SVG so we can colour it via currentColor and don't need an
  // asset round-trip. Rotated 0° because the timeline is horizontal —
  // the silhouette already reads "moving right".
  return (
    <svg
      viewBox="0 0 24 24"
      width="22"
      height="22"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path
        fill="currentColor"
        d="M21 16v-2l-8-5V3.5a1.5 1.5 0 0 0-3 0V9l-8 5v2l8-2.5V19l-2 1.5V22l3.5-1 3.5 1v-1.5L13 19v-5.5z"
      />
    </svg>
  );
}

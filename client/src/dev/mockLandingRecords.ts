// Dev-only Mock-LandingRecords für RunwayDiagram-Design-Iteration.
// Aktiviert über das versteckte "Preview"-Tab (nur in `npm run tauri dev`
// sichtbar dank `import.meta.env.DEV`). Im Production-Build ungenutzt.
//
// 4 Varianten decken die wichtigsten visuellen Cases ab:
//   * MS713-Anchor  — OLBA RWY 17, leicht left of CL, Aim −80 m short
//   * Perfect       — TDZ-Treffer, Aim ±0, CL=0
//   * Long Landing  — TDZ verfehlt, Aim +500 m past
//   * DDS-Violation — Touchdown vor displaced threshold (OLBA RWY 35)

import type { LandingRecord } from "../components/LandingPanel";

const NOW_ISO = "2026-05-13T17:42:00Z";

function baseRecord(): LandingRecord {
  return {
    pirep_id: "preview-mock",
    touchdown_at: NOW_ISO,
    recorded_at: NOW_ISO,
    flight_number: "MS713",
    airline_icao: "MSR",
    dpt_airport: "HECA",
    arr_airport: "OLBA",
    touchdown_airport: "OLBA",
    touchdown_airport_source: "runway_match",
    touchdown_distance_to_destination_nm: 0,
    touchdown_nearest_distance_nm: null,
    aircraft_registration: "SU-GCC",
    aircraft_icao: "B738",
    aircraft_title: "Boeing 737-800 PMDG",
    sim_kind: "X-PLANE",

    score_numeric: 82,
    score_label: "smooth",
    grade_letter: "A",

    landing_rate_fpm: -194,
    landing_peak_vs_fpm: -210,
    landing_g_force: 1.32,
    landing_peak_g_force: 1.52,
    landing_pitch_deg: 4.2,
    landing_bank_deg: 0.3,
    landing_speed_kt: 142,
    landing_heading_deg: 172,
    landing_weight_kg: 62500,
    touchdown_sideslip_deg: 0.4,
    bounce_count: 0,

    headwind_kt: 8,
    crosswind_kt: -2,

    approach_vs_stddev_fpm: 65,
    approach_bank_stddev_deg: 2.1,
    rollout_distance_m: 1100,

    planned_block_fuel_kg: 8800,
    planned_burn_kg: 4500,
    planned_tow_kg: 65800,
    planned_ldw_kg: 61300,
    planned_zfw_kg: 56200,
    actual_trip_burn_kg: 4620,
    fuel_efficiency_kg_diff: 120,
    fuel_efficiency_pct: 2.7,
    takeoff_weight_kg: 66000,
    takeoff_fuel_kg: 9100,
    landing_fuel_kg: 4480,
    block_fuel_kg: 8800,

    runway_match: {
      airport_ident: "OLBA",
      runway_ident: "17",
      surface: "ASP",
      length_ft: 10663,
      centerline_distance_m: -6.6,
      centerline_distance_abs_ft: 21.65,
      side: "LEFT",
      touchdown_distance_from_threshold_ft: 1050,
      source: "navigraph",
      nav_cycle: "2604",
      true_course_deg: 176.94,
      displaced_threshold_ft: 0,
      tch_expected_ft: 49,
      glideslope_angle_deg: 3.0,
    },
    touchdown_profile: [],
    approach_samples: [],

    ux_version: 1,
    forensics_version: 2,
    landing_confidence: "high",
    landing_source: "vs_at_impact",
    sub_scores: [],

    runway_geometry_trusted: true,
    runway_geometry_reason: null,

    accident: false,

    // v0.8.0 assessment fields
    td_distance_from_threshold_m: 320,
    td_in_tdz: true,
    td_third: 1,
    td_tdz_length_m: 900,
    aim_delta_m: -80,
    aim_class: "short_of_aim",
    aim_point_m: 400,
    tch_actual_ft: 47,
    tch_delta_ft: -2,
    tch_class: "on_profile",
    pre_displaced_threshold: false,
  };
}

export type MockKey = "ms713" | "perfect" | "long_landing" | "dds_violation";

export interface MockOption {
  key: MockKey;
  label: string;
  hint: string;
  build: () => LandingRecord;
}

export const MOCK_LANDING_OPTIONS: MockOption[] = [
  {
    key: "ms713",
    label: "MS713 (OLBA 17, 6.6 m left, aim short −80 m)",
    hint: "Real-Anchor aus dem v0.8.0-Bug-Report. Navigraph-Source, TDZ-Treffer, Aim leicht zu kurz.",
    build: baseRecord,
  },
  {
    key: "perfect",
    label: "Perfect Landing (OLBA 17, on centerline, aim ±0)",
    hint: "Idealwerte: CL=0 m, Aim exact, TCH on profile, TDZ-Treffer.",
    build: () => {
      const r = baseRecord();
      r.runway_match!.centerline_distance_m = 0.2;
      r.runway_match!.centerline_distance_abs_ft = 0.66;
      r.runway_match!.side = "CENTER";
      r.runway_match!.touchdown_distance_from_threshold_ft = 1312;
      r.td_distance_from_threshold_m = 400;
      r.aim_delta_m = 0;
      r.aim_class = "perfect";
      r.tch_actual_ft = 49;
      r.tch_delta_ft = 0;
      r.score_numeric = 96;
      r.score_label = "smooth";
      r.grade_letter = "A";
      return r;
    },
  },
  {
    key: "long_landing",
    label: "Long Landing (OLBA 17, +500 m past aim, outside TDZ)",
    hint: "Pilot setzt erst bei 900 m past threshold auf — TDZ verfehlt, Aim +500 m → Long-Landing-Pill.",
    build: () => {
      const r = baseRecord();
      r.runway_match!.centerline_distance_m = 3.5;
      r.runway_match!.centerline_distance_abs_ft = 11.48;
      r.runway_match!.side = "RIGHT";
      r.runway_match!.touchdown_distance_from_threshold_ft = 2953;
      r.td_distance_from_threshold_m = 900;
      r.td_in_tdz = false;
      r.td_third = 2;
      r.aim_delta_m = 500;
      r.aim_class = "long_landing";
      r.tch_actual_ft = 75;
      r.tch_delta_ft = 26;
      r.tch_class = "high";
      r.score_numeric = 58;
      r.score_label = "acceptable";
      r.grade_letter = "C";
      return r;
    },
  },
  {
    key: "dds_violation",
    label: "DDS Violation (OLBA 35, touchdown vor displaced threshold)",
    hint: "OLBA RWY 35 hat 2690 ft (820 m) displaced. Pilot setzt 50 m VOR Landing-Threshold auf → illegal.",
    build: () => {
      const r = baseRecord();
      r.runway_match!.runway_ident = "35";
      r.runway_match!.length_ft = 10663;
      r.runway_match!.centerline_distance_m = -1.2;
      r.runway_match!.centerline_distance_abs_ft = 3.94;
      r.runway_match!.side = "LEFT";
      r.runway_match!.touchdown_distance_from_threshold_ft = -164; // ~ -50 m
      r.runway_match!.displaced_threshold_ft = 2690;
      r.runway_match!.true_course_deg = 356.94;
      r.runway_match!.tch_expected_ft = 50;
      r.td_distance_from_threshold_m = -50;
      r.td_in_tdz = false;
      r.td_third = 1;
      r.aim_delta_m = -450;
      r.aim_class = "severe";
      r.tch_actual_ft = 18;
      r.tch_delta_ft = -32;
      r.tch_class = "below_profile";
      r.pre_displaced_threshold = true;
      r.score_numeric = 32;
      r.score_label = "hard";
      r.grade_letter = "F";
      return r;
    },
  },
];

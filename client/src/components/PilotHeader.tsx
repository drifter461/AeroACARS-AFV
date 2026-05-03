import { useTranslation } from "react-i18next";
import type { Profile } from "../types";

interface Props {
  profile: Profile;
  onLogout: () => void;
}

/**
 * Slim pilot identity row above the bids list. Layout:
 *
 *   ┌────────────┐  Name (large)
 *   │ Logo 180×80│  Ident · Rank · Airline      📍 EDDM  🏠 EDDV  [⏻]
 *   └────────────┘
 *
 * Logo is 180×80 (= GSG native size, 2.25:1 ratio) on a white
 * background — airline logos are universally designed for white,
 * forcing the bg means a Turkish red, a GSG dark-blue, a Lufthansa
 * yellow all render correctly. Pre-v0.1.30 the logo lived in a
 * 36×36 square on a dark surface and was effectively invisible.
 *
 * Tooltips on 📍/🏠 say "laut Webseite ({{airline}})" — using the
 * airline ICAO from the profile so multi-VA pilots can tell which
 * site a stale value is coming from. Falls back to "Webseite" alone
 * when the airline relation isn't set.
 */
export function PilotHeader({ profile, onLogout }: Props) {
  const { t } = useTranslation();
  const airline = profile.airline;
  const airlineIcao = airline?.icao ?? "";

  return (
    <section className="pilot-header pilot-header--slim">
      <div className="pilot-header__logo-slim">
        {airline?.logo ? (
          <img src={airline.logo} alt={airline.name} />
        ) : (
          <div className="pilot-header__logo-fallback" aria-hidden="true">
            {airline?.icao ?? "✈"}
          </div>
        )}
      </div>

      <div className="pilot-header__identity-slim">
        <div className="pilot-header__identity-line">
          <span className="pilot-header__name-slim">{profile.name}</span>
        </div>
        <div className="pilot-header__identity-line">
          {profile.ident && (
            <span className="pilot-header__chip-slim">{profile.ident}</span>
          )}
          {profile.rank?.name && (
            <>
              <span className="pilot-header__sep" aria-hidden="true">·</span>
              <span className="pilot-header__chip-slim pilot-header__chip-slim--muted">
                {profile.rank.name}
              </span>
            </>
          )}
          {airline && (
            <>
              <span className="pilot-header__sep" aria-hidden="true">·</span>
              <span className="pilot-header__chip-slim pilot-header__chip-slim--muted">
                {airline.icao}
              </span>
            </>
          )}
        </div>
      </div>

      <div className="pilot-header__locations-slim">
        <span
          className="pilot-header__loc-slim"
          title={t("pilot_header.location_tooltip", { airline: airlineIcao })}
        >
          <span className="pilot-header__loc-icon" aria-hidden="true">📍</span>
          <span className="pilot-header__loc-value-slim">
            {profile.curr_airport ?? "—"}
          </span>
        </span>
        <span
          className="pilot-header__loc-slim"
          title={t("pilot_header.home_tooltip", { airline: airlineIcao })}
        >
          <span className="pilot-header__loc-icon" aria-hidden="true">🏠</span>
          <span className="pilot-header__loc-value-slim">
            {profile.home_airport ?? "—"}
          </span>
        </span>
      </div>

      <button
        type="button"
        className="pilot-header__logout-slim"
        onClick={onLogout}
        title={t("actions.logout")}
        aria-label={t("actions.logout")}
      >
        ⏻
      </button>
    </section>
  );
}

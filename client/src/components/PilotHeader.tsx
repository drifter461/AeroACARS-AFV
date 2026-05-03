import { useTranslation } from "react-i18next";
import type { Profile } from "../types";

interface Props {
  profile: Profile;
  onLogout: () => void;
}

/**
 * Slim pilot identity row above the bids list. Replaces the older big
 * "card" layout (which competed visually with the bid list and made
 * the tab feel like three competing zones — profile / sim status /
 * bids — instead of a clear "here are your flights" page).
 *
 * Single horizontal line:
 *   [Logo]  Name · Ident · Rank · Airline    📍 EDDL  🏠 EDLE   [⏻]
 *
 * 📍 = `curr_airport` (where the pilot is stationed in phpVMS — last
 * PIREP's destination). Tooltip explains it so nobody confuses it with
 * "current FLIGHT".
 *
 * 🏠 = `home_airport` (career home base). Tooltip explains.
 *
 * Logout is icon-only (⏻) on the right — secondary action, doesn't
 * shout for attention. Hover tooltip shows the localised "Logout".
 */
export function PilotHeader({ profile, onLogout }: Props) {
  const { t } = useTranslation();
  const airline = profile.airline;

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
        <span className="pilot-header__name-slim">{profile.name}</span>
        {profile.ident && (
          <>
            <span className="pilot-header__sep" aria-hidden="true">·</span>
            <span className="pilot-header__chip-slim">{profile.ident}</span>
          </>
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

      <div className="pilot-header__locations-slim">
        <span
          className="pilot-header__loc-slim"
          title={t("pilot_header.location_tooltip")}
        >
          <span className="pilot-header__loc-icon" aria-hidden="true">📍</span>
          <span className="pilot-header__loc-value-slim">
            {profile.curr_airport ?? "—"}
          </span>
        </span>
        <span
          className="pilot-header__loc-slim"
          title={t("pilot_header.home_tooltip")}
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

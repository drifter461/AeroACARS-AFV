import { useTranslation } from "react-i18next";
import type { LoginResult } from "../types";

interface Props {
  session: LoginResult;
  onLogout: () => void;
}

export function Dashboard({ session, onLogout }: Props) {
  const { t } = useTranslation();
  const { profile, base_url } = session;
  const airlineLabel = profile.airline
    ? `${profile.airline.icao} — ${profile.airline.name}`
    : "—";

  return (
    <section className="dashboard">
      <header className="dashboard__header">
        <div>
          <h2>{t("dashboard.welcome", { name: profile.name })}</h2>
          <p className="dashboard__site">
            {t("dashboard.site")}: <code>{base_url}</code>
          </p>
        </div>
        <button type="button" onClick={onLogout}>
          {t("actions.logout")}
        </button>
      </header>

      <dl className="dashboard__pilot">
        <dt>{t("dashboard.pilot_id")}</dt>
        <dd>{profile.pilot_id}</dd>

        <dt>{t("dashboard.airline")}</dt>
        <dd>{airlineLabel}</dd>

        <dt>{t("dashboard.current_airport")}</dt>
        <dd>{profile.curr_airport_id ?? "—"}</dd>

        <dt>{t("dashboard.home_airport")}</dt>
        <dd>{profile.home_airport_id ?? "—"}</dd>
      </dl>
    </section>
  );
}

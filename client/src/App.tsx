import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { applyTheme, getInitialTheme, type Theme } from "./theme";

interface AppInfo {
  name: string;
  version: string;
  commit: string | null;
}

function App() {
  const { t, i18n } = useTranslation();
  const [theme, setTheme] = useState<Theme>(() => getInitialTheme());
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  function toggleTheme() {
    setTheme((prev) => (prev === "dark" ? "light" : "dark"));
  }

  async function loadAppInfo() {
    const result = await invoke<AppInfo>("app_info");
    setInfo(result);
  }

  return (
    <main className="app">
      <header className="app__header">
        <h1>{t("app.name")}</h1>
        <p className="tagline">{t("app.tagline")}</p>

        <div className="header-actions">
          <button
            type="button"
            onClick={() => i18n.changeLanguage("de")}
            disabled={i18n.resolvedLanguage === "de"}
          >
            {t("actions.language_de")}
          </button>
          <button
            type="button"
            onClick={() => i18n.changeLanguage("en")}
            disabled={i18n.resolvedLanguage === "en"}
          >
            {t("actions.language_en")}
          </button>
          <button type="button" onClick={toggleTheme}>
            {theme === "dark"
              ? t("actions.toggle_theme_light")
              : t("actions.toggle_theme_dark")}
          </button>
        </div>
      </header>

      <section className="status-grid">
        <div className="status-card status-card--offline">
          <span className="status-card__label">{t("status.phpvms")}</span>
          <span className="status-card__value">
            {t("status.phpvms_disconnected")}
          </span>
        </div>
        <div className="status-card status-card--offline">
          <span className="status-card__label">{t("status.simulator")}</span>
          <span className="status-card__value">
            {t("status.simulator_disconnected")}
          </span>
        </div>
      </section>

      <section className="phase">
        <h2>{t("phase.title")}</h2>
        <p>{t("phase.description")}</p>

        <button type="button" onClick={loadAppInfo}>
          {t("actions.show_app_info")}
        </button>

        {info && (
          <dl className="appinfo">
            <dt>{t("appinfo.version")}</dt>
            <dd>
              {info.name} {info.version}
            </dd>
            <dt>{t("appinfo.commit")}</dt>
            <dd>{info.commit ?? t("appinfo.commit_unknown")}</dd>
          </dl>
        )}
      </section>
    </main>
  );
}

export default App;

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { applyTheme, getInitialTheme, type Theme } from "./theme";
import { LoginPage } from "./components/LoginPage";
import { Dashboard } from "./components/Dashboard";
import type { LoginResult } from "./types";

type SessionStatus =
  | { kind: "loading" }
  | { kind: "loggedOut" }
  | { kind: "loggedIn"; session: LoginResult };

function App() {
  const { t, i18n } = useTranslation();
  const [theme, setTheme] = useState<Theme>(() => getInitialTheme());
  const [status, setStatus] = useState<SessionStatus>({ kind: "loading" });

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const result = await invoke<LoginResult | null>("phpvms_load_session");
        if (cancelled) return;
        setStatus(
          result ? { kind: "loggedIn", session: result } : { kind: "loggedOut" },
        );
      } catch {
        // If session restore fails for any reason, fall back to login.
        if (!cancelled) setStatus({ kind: "loggedOut" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  function toggleTheme() {
    setTheme((prev) => (prev === "dark" ? "light" : "dark"));
  }

  async function handleLogout() {
    try {
      await invoke("phpvms_logout");
    } catch {
      // Even if the keyring call fails, drop in-memory session.
    }
    setStatus({ kind: "loggedOut" });
  }

  const phpvmsConnected = status.kind === "loggedIn";

  return (
    <main className="app">
      <header className="app__header">
        <div>
          <h1>{t("app.name")}</h1>
          <p className="tagline">{t("app.tagline")}</p>
        </div>

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
        <div
          className={`status-card status-card--${
            phpvmsConnected ? "online" : "offline"
          }`}
        >
          <span className="status-card__label">{t("status.phpvms")}</span>
          <span className="status-card__value">
            {phpvmsConnected
              ? t("status.phpvms_connected")
              : t("status.phpvms_disconnected")}
          </span>
        </div>
        <div className="status-card status-card--offline">
          <span className="status-card__label">{t("status.simulator")}</span>
          <span className="status-card__value">
            {t("status.simulator_disconnected")}
          </span>
        </div>
      </section>

      {status.kind === "loading" && (
        <section className="phase">
          <p>{t("status.checking_session")}</p>
        </section>
      )}

      {status.kind === "loggedOut" && <LoginPage onSuccess={(s) => setStatus({ kind: "loggedIn", session: s })} />}

      {status.kind === "loggedIn" && (
        <Dashboard session={status.session} onLogout={handleLogout} />
      )}
    </main>
  );
}

export default App;

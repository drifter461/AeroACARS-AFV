import { type FormEvent, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { LoginResult, UiError } from "../types";

interface Props {
  initialUrl?: string;
  onSuccess: (result: LoginResult) => void;
}

const KNOWN_ERROR_CODES = new Set([
  "invalid_url",
  "network",
  "unauthenticated",
  "forbidden",
  "not_found",
  "rate_limited",
  "server",
  "bad_response",
  "keyring",
  "config_path",
  "config_read",
  "config_write",
  "config_parse",
]);

function errorKey(code: string): string {
  return KNOWN_ERROR_CODES.has(code)
    ? `login.error.${code}`
    : "login.error.unknown";
}

function isUiError(value: unknown): value is UiError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

export function LoginPage({ initialUrl = "", onSuccess }: Props) {
  const { t } = useTranslation();
  const [url, setUrl] = useState(initialUrl);
  const [apiKey, setApiKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<UiError | null>(null);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      const result = await invoke<LoginResult>("phpvms_login", {
        url: url.trim(),
        apiKey: apiKey.trim(),
      });
      onSuccess(result);
    } catch (err: unknown) {
      if (isUiError(err)) {
        setError(err);
      } else {
        setError({ code: "unknown", message: String(err) });
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="login">
      <h2>{t("login.title")}</h2>
      <p className="login__description">{t("login.description")}</p>

      <form className="login__form" onSubmit={handleSubmit}>
        <label className="field">
          <span className="field__label">{t("login.url_label")}</span>
          <input
            type="url"
            inputMode="url"
            autoComplete="url"
            required
            placeholder={t("login.url_placeholder")}
            value={url}
            onChange={(e) => setUrl(e.currentTarget.value)}
            disabled={submitting}
          />
        </label>

        <label className="field">
          <span className="field__label">{t("login.api_key_label")}</span>
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            required
            placeholder={t("login.api_key_placeholder")}
            value={apiKey}
            onChange={(e) => setApiKey(e.currentTarget.value)}
            disabled={submitting}
          />
        </label>

        {error && (
          <div className="login__error" role="alert">
            {t(errorKey(error.code))}
          </div>
        )}

        <button
          type="submit"
          className="button button--primary"
          disabled={submitting || !url || !apiKey}
        >
          {submitting ? t("login.submitting") : t("login.submit")}
        </button>
      </form>
    </section>
  );
}

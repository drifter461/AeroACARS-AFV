import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";

interface Props {
  /** ICAO of the planned arrival airport — shown in the divert hint. */
  plannedArrival: string;
  /** List of i18n field keys reported by the backend's validation. */
  missing: string[];
  /** Called after the manual PIREP was filed successfully. */
  onFiled: () => void;
  /** Called when the user wants to cancel the flight (PIREP discarded server-side). */
  onCancelFlight: () => void;
  /** Called when the user dismisses the dialog without taking action. */
  onClose: () => void;
}

type Stage = "options" | "manual_form";

export function ManualFileDialog({
  plannedArrival,
  missing,
  onFiled,
  onCancelFlight,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const [stage, setStage] = useState<Stage>("options");
  const [divert, setDivert] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submitManual() {
    const trimmedDivert = divert.trim();
    const trimmedReason = reason.trim();
    // Divert without a reason is meaningless — the admin needs context.
    if (trimmedDivert && !trimmedReason) {
      setError(t("active_flight.validation.reason_required_for_divert"));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await invoke("flight_end_manual", {
        notesOverride: trimmedReason ? null : null,
        divertTo: trimmedDivert || null,
        reason: trimmedReason || null,
      });
      onFiled();
    } catch (err: unknown) {
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: string }).message)
          : String(err);
      setError(`${t("active_flight.validation.manual_failed")}\n\n${msg}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="manual-dialog-backdrop" role="dialog" aria-modal="true">
      <div className="manual-dialog">
        {stage === "options" && (
          <>
            <h2 className="manual-dialog__title">
              {t("active_flight.validation.title")}
            </h2>
            <p className="manual-dialog__intro">
              {t("active_flight.validation.intro")}
            </p>
            <ul className="manual-dialog__missing">
              {missing.map((key) => (
                <li key={key}>
                  {t(`active_flight.validation.fields.${key}`, {
                    defaultValue: key,
                  })}
                </li>
              ))}
            </ul>

            <h3 className="manual-dialog__subtitle">
              {t("active_flight.validation.options_title")}
            </h3>
            <div className="manual-dialog__options">
              <button
                type="button"
                className="button button--primary"
                onClick={() => setStage("manual_form")}
                disabled={busy}
              >
                {t("active_flight.validation.option_manual")}
              </button>
              <span className="manual-dialog__hint">
                {t("active_flight.validation.option_manual_hint")}
              </span>

              <button
                type="button"
                className="manual-dialog__danger"
                onClick={onCancelFlight}
                disabled={busy}
              >
                {t("active_flight.validation.option_cancel")}
              </button>
              <span className="manual-dialog__hint">
                {t("active_flight.validation.option_cancel_hint")}
              </span>

              <button
                type="button"
                className="manual-dialog__secondary"
                onClick={onClose}
                disabled={busy}
              >
                {t("active_flight.validation.option_back")}
              </button>
            </div>
          </>
        )}

        {stage === "manual_form" && (
          <>
            <h2 className="manual-dialog__title">
              {t("active_flight.validation.manual_form_title")}
            </h2>
            <p className="manual-dialog__intro">
              {t("active_flight.validation.manual_form_intro")}
            </p>

            <label className="manual-dialog__field">
              <span>{t("active_flight.validation.divert_label")}</span>
              <input
                type="text"
                value={divert}
                onChange={(e) => setDivert(e.target.value.toUpperCase())}
                maxLength={4}
                placeholder="EDDV"
                disabled={busy}
              />
              <small>
                {t("active_flight.validation.divert_hint", {
                  planned: plannedArrival,
                })}
              </small>
            </label>

            <label className="manual-dialog__field">
              <span>{t("active_flight.validation.reason_label")}</span>
              <textarea
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                rows={4}
                placeholder={t(
                  "active_flight.validation.reason_placeholder",
                )}
                disabled={busy}
              />
            </label>

            {error && (
              <p className="manual-dialog__error" role="alert">
                {error}
              </p>
            )}

            <div className="manual-dialog__options">
              <button
                type="button"
                className="button button--primary"
                onClick={() => void submitManual()}
                disabled={busy}
              >
                {busy
                  ? t("active_flight.validation.submitting_manual")
                  : t("active_flight.validation.submit_manual")}
              </button>
              <button
                type="button"
                className="manual-dialog__secondary"
                onClick={() => setStage("options")}
                disabled={busy}
              >
                {t("active_flight.validation.option_back")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

/**
 * In-app modal confirm dialog. Replaces `window.confirm()` everywhere
 * because Tauri's macOS WKWebView silently drops `confirm()` calls —
 * the dialog never shows, the call returns `false`, and the user is
 * stuck unable to action destructive buttons. v0.3.1 shipped with the
 * native call still in place; v0.3.2 routes everything through here.
 *
 * Usage:
 *
 * ```tsx
 * const { confirm, dialog } = useConfirm();
 *
 * async function handleDiscard() {
 *   if (!(await confirm({ message: t("...") }))) return;
 *   await invoke("flight_cancel");
 * }
 *
 * return <>
 *   {dialog}
 *   <button onClick={handleDiscard}>Discard</button>
 * </>;
 * ```
 *
 * The hook keeps the call-site change minimal — only `confirm(msg)`
 * becomes `await confirm({ message: msg })` plus mounting `{dialog}`.
 * The dialog itself renders nothing until a confirm is in flight.
 */

interface ConfirmOptions {
  message: string;
  /** Optional title above the message. Defaults to the i18n key
   *  `confirm_dialog.default_title` (= "Bist du sicher?" / "Are you sure?"). */
  title?: string;
  /** Confirm-button label. Defaults to i18n `confirm_dialog.confirm`. */
  confirmLabel?: string;
  /** Cancel-button label. Defaults to i18n `confirm_dialog.cancel`. */
  cancelLabel?: string;
  /** Treat the action as destructive (red confirm button). */
  destructive?: boolean;
}

interface PendingConfirm extends ConfirmOptions {
  resolve: (ok: boolean) => void;
}

export function useConfirm() {
  const { t } = useTranslation();
  const [pending, setPending] = useState<PendingConfirm | null>(null);
  // Stable resolver ref so the dialog's button handlers don't rebind
  // every render (would cause focus to jump on key state changes).
  const pendingRef = useRef<PendingConfirm | null>(null);
  pendingRef.current = pending;

  const confirm = useCallback((opts: ConfirmOptions): Promise<boolean> => {
    return new Promise((resolve) => {
      setPending({ ...opts, resolve });
    });
  }, []);

  const close = useCallback((ok: boolean) => {
    const cur = pendingRef.current;
    if (cur) cur.resolve(ok);
    setPending(null);
  }, []);

  // Esc cancels, Enter confirms — matches what users expect from
  // native `confirm()`. Listener attaches only while a dialog is open.
  useEffect(() => {
    if (!pending) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(false);
      } else if (e.key === "Enter") {
        e.preventDefault();
        close(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, close]);

  const dialog = pending ? (
    <div
      className="confirm-dialog__overlay"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      onClick={(e) => {
        // Click on backdrop (not on the dialog box) → cancel.
        if (e.target === e.currentTarget) close(false);
      }}
    >
      <div
        className={
          "confirm-dialog" +
          (pending.destructive ? " confirm-dialog--destructive" : "")
        }
      >
        <h2 id="confirm-dialog-title" className="confirm-dialog__title">
          {pending.title ?? t("confirm_dialog.default_title")}
        </h2>
        <p className="confirm-dialog__message">{pending.message}</p>
        <div className="confirm-dialog__actions">
          <button
            type="button"
            className="confirm-dialog__btn confirm-dialog__btn--secondary"
            onClick={() => close(false)}
            autoFocus
          >
            {pending.cancelLabel ?? t("confirm_dialog.cancel")}
          </button>
          <button
            type="button"
            className={
              "confirm-dialog__btn confirm-dialog__btn--primary" +
              (pending.destructive ? " confirm-dialog__btn--destructive" : "")
            }
            onClick={() => close(true)}
          >
            {pending.confirmLabel ?? t("confirm_dialog.confirm")}
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return { confirm, dialog };
}

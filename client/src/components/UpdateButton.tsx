import { useEffect, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Compact "Update available" button — sits inline in the app header
 * next to the status pills, opens a modal with release notes +
 * install action when clicked.
 *
 * Replaces the previous full-width `UpdateBanner` design that pushed
 * all page content down. Pilots prefer the inline button: less
 * invasive, more polished, dismissible just by ignoring it.
 *
 * Renders nothing when:
 *   * No update was found (network error, GitHub down, or already
 *     on the latest version)
 *   * The pilot dismissed the prompt for this session.
 */
export function UpdateButton() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [open, setOpen] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const u = await check();
        if (!cancelled && u) setUpdate(u);
      } catch {
        // Silent: offline / GitHub down. Pilot can manually retry
        // by restarting the app.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (!update) return null;

  async function handleInstall() {
    if (!update || installing) return;
    setInstalling(true);
    setProgress("Lädt Update herunter…");
    try {
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            setProgress(
              total > 0
                ? `Download: 0 / ${(total / 1_048_576).toFixed(1)} MB`
                : "Download startet…",
            );
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setProgress(
              total > 0
                ? `Download: ${(downloaded / 1_048_576).toFixed(1)} / ${(total / 1_048_576).toFixed(1)} MB`
                : `Download: ${(downloaded / 1_048_576).toFixed(1)} MB`,
            );
            break;
          case "Finished":
            setProgress("Installiere — App startet gleich neu…");
            break;
        }
      });
      await relaunch();
    } catch (err) {
      setProgress(`Fehler: ${err}`);
      setInstalling(false);
    }
  }

  return (
    <>
      <button
        type="button"
        className="update-button"
        onClick={() => setOpen(true)}
        title={`Update verfügbar — v${update.version}`}
      >
        <span className="update-button__icon" aria-hidden="true">
          ⬇
        </span>
        <span>Update verfügbar</span>
      </button>

      {open && (
        <div
          className="update-modal__backdrop"
          onClick={() => !installing && setOpen(false)}
        >
          <div
            className="update-modal"
            role="dialog"
            aria-labelledby="update-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 id="update-modal-title" className="update-modal__title">
              Update verfügbar — v{update.version}
            </h3>
            {update.body && (
              <p className="update-modal__notes">{update.body}</p>
            )}
            {progress && (
              <p className="update-modal__progress">{progress}</p>
            )}
            <div className="update-modal__actions">
              <button
                type="button"
                className="button button--primary"
                onClick={() => void handleInstall()}
                disabled={installing}
              >
                {installing ? "…" : "Jetzt installieren"}
              </button>
              <button
                type="button"
                className="button"
                onClick={() => setOpen(false)}
                disabled={installing}
              >
                Später
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

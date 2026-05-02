import { useEffect, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Update-Check banner. On app mount, asks Tauri's updater plugin
 * whether GitHub Releases has a newer build than us. If so, shows
 * a discreet banner with the version + release notes and lets the
 * pilot install on demand.
 *
 * Rules:
 *   * Silent on no-update — banner just stays hidden.
 *   * Silent on errors (offline, GitHub down) — we don't want to
 *     spam the pilot with red bars when their network blips.
 *   * Pilot can dismiss — we hide the banner for the rest of this
 *     session via in-memory state. Persisting "skipped versions"
 *     across restarts is Phase 3.
 */
export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const u = await check();
        if (!cancelled && u) {
          setUpdate(u);
        }
      } catch {
        // Network error / offline / GitHub down — silently skip.
        // Pilot can manually retry via Settings → "Auf Updates prüfen".
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (!update || dismissed) return null;

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
      // Tauri's installer replaces the binary in place; relaunch
      // picks up the new version.
      await relaunch();
    } catch (err) {
      setProgress(`Fehler: ${err}`);
      setInstalling(false);
    }
  }

  return (
    <aside className="update-banner" role="status">
      <div className="update-banner__main">
        <strong>Update verfügbar — v{update.version}</strong>
        {update.body && (
          <span className="update-banner__notes">{update.body}</span>
        )}
        {progress && <span className="update-banner__progress">{progress}</span>}
      </div>
      <div className="update-banner__actions">
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
          onClick={() => setDismissed(true)}
          disabled={installing}
        >
          Später
        </button>
      </div>
    </aside>
  );
}

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { addToast } from "../util/toast";

export interface UpdateStatus {
  checking: boolean;
  available: boolean;
  version?: string;
  body?: string;
  downloading: boolean;
  downloadedBytes: number;
  totalBytes?: number;
  error?: string;
}

let activeUpdate: Update | null = null;

export async function checkForAppUpdates(manual = false): Promise<Update | null> {
  try {
    const update = await check();
    if (update?.available) {
      activeUpdate = update;
      addToast({
        message: `Vantage v${update.version} is available!`,
        kind: "info",
        actionLabel: "Update",
        action: () => {
          downloadAndInstallUpdate(update);
        },
      });
      return update;
    } else if (manual) {
      addToast({
        message: "Vantage is up to date.",
        kind: "info",
      });
    }
    return null;
  } catch (err: any) {
    if (manual) {
      addToast({
        message: `Update check failed: ${err?.message || err}`,
        kind: "error",
      });
    }
    return null;
  }
}

export async function downloadAndInstallUpdate(update?: Update | null) {
  const target = update || activeUpdate;
  if (!target) return;

  try {
    addToast({
      message: `Downloading Vantage v${target.version}…`,
      kind: "info",
    });

    let downloaded = 0;
    let contentLength = 0;

    await target.downloadAndInstall((event) => {
      switch (event.event) {
        case "Started":
          contentLength = event.data.contentLength || 0;
          break;
        case "Progress":
          downloaded += event.data.chunkLength;
          break;
        case "Finished":
          addToast({
            message: "Update installed! Restarting Vantage…",
            kind: "success",
          });
          break;
      }
    });

    await relaunch();
  } catch (err: any) {
    addToast({
      message: `Failed to install update: ${err?.message || err}`,
      kind: "error",
    });
  }
}

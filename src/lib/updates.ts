// Checking for and installing new versions (Tauri updater, signed releases on GitHub).

import { isTauri } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useSyncExternalStore } from "react";

let available: Update | null = null;
const listeners = new Set<() => void>();

function setAvailable(update: Update | null) {
  available = update;
  for (const l of listeners) l();
}

/** The update found by the last check, if any. */
export function useAvailableUpdate(): Update | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => available,
  );
}

/** Looks for a newer version. Throws if the update server can't be reached. */
export async function checkForUpdate(): Promise<Update | null> {
  if (!isTauri()) return null;
  const update = await check();
  setAvailable(update);
  return update;
}

let startupCheckDone = false;

/** One quiet check per launch; failures (offline, private repo) are ignored. */
export function checkOnStartup() {
  if (startupCheckDone) return;
  startupCheckDone = true;
  window.setTimeout(() => void checkForUpdate().catch(() => undefined), 5000);
}

/** Downloads and installs `update`, reporting progress 0..1 (or null when the size is unknown), then restarts. */
export async function installUpdate(update: Update, onProgress: (fraction: number | null) => void) {
  let total = 0;
  let received = 0;
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      onProgress(total ? 0 : null);
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
      onProgress(total ? Math.min(1, received / total) : null);
    } else {
      onProgress(1);
    }
  });
  await relaunch();
}

export function describeCheckError(e: unknown): string {
  const text = e instanceof Error ? e.message : String(e);
  return `PolarisCal couldn't reach the update server (${text}). Updates are published on GitHub, so this computer needs internet access, and the project's releases must be public.`;
}

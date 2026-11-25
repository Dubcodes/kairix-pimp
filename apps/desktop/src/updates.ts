import {
  check,
  type DownloadEvent,
  type Update,
} from '@tauri-apps/plugin-updater';
import { desktopBridge } from './bridge';

export interface AvailableUpdate {
  currentVersion: string;
  version: string;
  date?: string;
  notes?: string;
  sizeBytes?: number;
  classification: 'Development' | 'Stable';
}

let pendingUpdate: Update | null = null;
let checkInFlight: Promise<AvailableUpdate | null> | null = null;
let installInFlight: Promise<void> | null = null;

async function closePendingUpdate(): Promise<void> {
  const previous = pendingUpdate;
  pendingUpdate = null;
  await previous?.close();
}

export async function checkApprovedBetaUpdate(): Promise<AvailableUpdate | null> {
  if (installInFlight) {
    throw new Error('An approved update installation is already in progress.');
  }
  if (checkInFlight) return checkInFlight;
  const operation = (async () => {
    // Clear the old resource before any fallible preflight or network work. A
    // rejected check must never leave a closed update installable.
    await closePendingUpdate();
    const configuration = await desktopBridge.updaterConfiguration();
    if (!configuration.configured) {
      throw new Error(
        configuration.reason ??
          'Signed updates are not configured in this build.',
      );
    }
    const update = await check({
      timeout: 30_000,
      target: configuration.target,
    });
    pendingUpdate = update;
    if (!update) return null;
    const declaredClassification = update.rawJson.classification;
    let classification: AvailableUpdate['classification'];
    if (
      declaredClassification === undefined &&
      update.version.startsWith('0.')
    ) {
      classification = 'Development';
    } else if (
      declaredClassification === 'Development' ||
      declaredClassification === 'Stable'
    ) {
      classification = declaredClassification;
    } else {
      await closePendingUpdate();
      throw new Error(
        'The signed update manifest has no valid release classification.',
      );
    }
    if (configuration.channel === 'Stable' && classification !== 'Stable') {
      await closePendingUpdate();
      throw new Error('The Stable channel rejected a Development release.');
    }
    return {
      currentVersion: update.currentVersion,
      version: update.version,
      date: update.date,
      notes: update.body,
      sizeBytes:
        typeof update.rawJson.size === 'number'
          ? update.rawJson.size
          : undefined,
      classification,
    };
  })();
  checkInFlight = operation;
  try {
    return await operation;
  } finally {
    if (checkInFlight === operation) checkInFlight = null;
  }
}

export async function installApprovedBetaUpdate(
  onEvent: (event: DownloadEvent) => void,
): Promise<void> {
  if (checkInFlight) {
    throw new Error('Wait for the current signed update check to finish.');
  }
  if (installInFlight) {
    throw new Error('An approved update installation is already in progress.');
  }
  if (!pendingUpdate) {
    throw new Error('Check for a signed update before installing.');
  }
  const update = pendingUpdate;
  const operation = (async () => {
    await desktopBridge.beginUpdateInstallation();
    pendingUpdate = null;
    try {
      await update.downloadAndInstall(onEvent, {
        timeout: 120_000,
        restartAfterInstall: true,
      });
    } catch (error) {
      // A partial/failed backend download is not reused. The next attempt must
      // obtain a fresh signed manifest and updater resource.
      try {
        await update.close();
      } catch {
        // Preserve the installation error; the pending reference is already
        // cleared and cannot be reused even if backend cleanup also failed.
      }
      throw error;
    } finally {
      await desktopBridge.endUpdateInstallation();
    }
  })();
  installInFlight = operation;
  try {
    await operation;
  } finally {
    if (installInFlight === operation) installInFlight = null;
  }
}

export async function discardPendingUpdate(): Promise<void> {
  if (checkInFlight) await checkInFlight.catch(() => undefined);
  if (installInFlight) await installInFlight.catch(() => undefined);
  await closePendingUpdate();
}

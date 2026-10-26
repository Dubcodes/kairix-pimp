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
}

let pendingUpdate: Update | null = null;

export async function checkApprovedBetaUpdate(): Promise<AvailableUpdate | null> {
  const configuration = await desktopBridge.updaterConfiguration();
  if (!configuration.configured) {
    throw new Error(
      configuration.reason ??
        'Signed Beta updates are not configured in this build.',
    );
  }
  if (configuration.channel !== 'Beta') {
    throw new Error(
      'This build is not configured for the approved Beta channel.',
    );
  }
  await pendingUpdate?.close();
  pendingUpdate = await check({ timeout: 30_000 });
  if (!pendingUpdate) return null;
  return {
    currentVersion: pendingUpdate.currentVersion,
    version: pendingUpdate.version,
    date: pendingUpdate.date,
    notes: pendingUpdate.body,
    sizeBytes:
      typeof pendingUpdate.rawJson.size === 'number'
        ? pendingUpdate.rawJson.size
        : undefined,
  };
}

export async function installApprovedBetaUpdate(
  onEvent: (event: DownloadEvent) => void,
): Promise<void> {
  if (!pendingUpdate) {
    throw new Error('Check for a signed Beta update before installing.');
  }
  await pendingUpdate.downloadAndInstall(onEvent, {
    timeout: 120_000,
    restartAfterInstall: true,
  });
}

export async function discardPendingUpdate(): Promise<void> {
  await pendingUpdate?.close();
  pendingUpdate = null;
}

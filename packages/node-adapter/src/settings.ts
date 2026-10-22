import { readFile } from 'node:fs/promises';
import path from 'node:path';
import type { DeviceSettings, SharedSettings } from '@kairix-pimp/core';
import { atomicWrite } from './atomic.js';

export async function saveDeviceSettings(
  configDirectory: string,
  settings: DeviceSettings,
): Promise<void> {
  await atomicWrite(
    path.join(configDirectory, 'device.json'),
    JSON.stringify(settings, null, 2),
  );
}

export async function loadDeviceSettings(
  configDirectory: string,
): Promise<DeviceSettings | undefined> {
  try {
    return JSON.parse(
      await readFile(path.join(configDirectory, 'device.json'), 'utf8'),
    ) as DeviceSettings;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
}

export async function saveSharedSettings(
  masterBin: string,
  settings: SharedSettings,
): Promise<void> {
  if (settings.schemaVersion !== 1)
    throw new Error('Unsupported shared settings schema');
  await atomicWrite(
    path.join(masterBin, '.pimp', 'settings.json'),
    JSON.stringify(settings, null, 2),
  );
}

export async function loadSharedSettings(
  masterBin: string,
): Promise<SharedSettings | undefined> {
  try {
    const parsed = JSON.parse(
      await readFile(path.join(masterBin, '.pimp', 'settings.json'), 'utf8'),
    ) as SharedSettings;
    if (parsed.schemaVersion !== 1)
      throw new Error('Unsupported shared settings schema');
    return parsed;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
}

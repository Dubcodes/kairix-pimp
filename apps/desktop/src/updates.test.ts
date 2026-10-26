import { beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  configuration: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }));
vi.mock('./bridge', () => ({
  desktopBridge: { updaterConfiguration: mocks.configuration },
}));

import {
  checkApprovedBetaUpdate,
  discardPendingUpdate,
  installApprovedBetaUpdate,
} from './updates';

beforeEach(async () => {
  await discardPendingUpdate();
  mocks.check.mockReset();
  mocks.configuration.mockReset();
});

it('fails closed before contacting an update endpoint when no public key is configured', async () => {
  mocks.configuration.mockResolvedValue({
    configured: false,
    channel: 'Beta',
    endpoint:
      'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
    reason: 'Signing key not provisioned',
  });
  await expect(checkApprovedBetaUpdate()).rejects.toThrow(
    'Signing key not provisioned',
  );
  expect(mocks.check).not.toHaveBeenCalled();
});

it('reports an approved higher beta returned by the configured updater', async () => {
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
    endpoint:
      'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
  });
  mocks.check.mockResolvedValue({
    currentVersion: '0.1.0',
    version: '0.1.1-1',
    date: '2026-10-26T00:00:00Z',
    body: 'Signed beta',
    rawJson: {},
    close: vi.fn(),
  });
  await expect(checkApprovedBetaUpdate()).resolves.toEqual({
    currentVersion: '0.1.0',
    version: '0.1.1-1',
    date: '2026-10-26T00:00:00Z',
    notes: 'Signed beta',
    sizeBytes: undefined,
  });
});

it('refuses installation until an approved update has been checked', async () => {
  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'Check for a signed Beta update before installing.',
  );
});

it('surfaces interrupted updater installation without reporting success', async () => {
  const interrupted = new Error('download interrupted');
  const downloadAndInstall = vi.fn().mockRejectedValue(interrupted);
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
    endpoint:
      'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
  });
  mocks.check.mockResolvedValue({
    currentVersion: '0.1.0',
    version: '0.1.1-1',
    rawJson: {},
    close: vi.fn(),
    downloadAndInstall,
  });
  await checkApprovedBetaUpdate();

  const onEvent = vi.fn();
  await expect(installApprovedBetaUpdate(onEvent)).rejects.toBe(interrupted);
  expect(downloadAndInstall).toHaveBeenCalledWith(onEvent, {
    timeout: 120_000,
    restartAfterInstall: true,
  });
});

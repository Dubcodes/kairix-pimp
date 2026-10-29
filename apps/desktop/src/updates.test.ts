import { beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  check: vi.fn(),
  configuration: vi.fn(),
  beginInstallation: vi.fn(),
  endInstallation: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }));
vi.mock('./bridge', () => ({
  desktopBridge: {
    updaterConfiguration: mocks.configuration,
    beginUpdateInstallation: mocks.beginInstallation,
    endUpdateInstallation: mocks.endInstallation,
  },
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
  mocks.beginInstallation.mockReset();
  mocks.beginInstallation.mockResolvedValue(undefined);
  mocks.endInstallation.mockReset();
  mocks.endInstallation.mockResolvedValue(undefined);
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

it('fails closed for an invalid embedded public key before contacting the endpoint', async () => {
  mocks.configuration.mockResolvedValue({
    configured: false,
    channel: 'Beta',
    reason: 'The embedded updater public key is invalid.',
  });
  await expect(checkApprovedBetaUpdate()).rejects.toThrow(
    'The embedded updater public key is invalid.',
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

it('clears a closed pending update when the next check fails', async () => {
  const close = vi.fn();
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
  });
  mocks.check
    .mockResolvedValueOnce({
      currentVersion: '0.1.1-1',
      version: '0.1.1-2',
      rawJson: {},
      close,
    })
    .mockRejectedValueOnce(new Error('offline'));

  await checkApprovedBetaUpdate();
  await expect(checkApprovedBetaUpdate()).rejects.toThrow('offline');
  expect(close).toHaveBeenCalledOnce();
  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'Check for a signed Beta update before installing.',
  );
});

it.each(['manifest is malformed', 'signature verification failed'])(
  'clears update state when the updater reports %s',
  async (message) => {
    mocks.configuration.mockResolvedValue({
      configured: true,
      channel: 'Beta',
    });
    mocks.check.mockRejectedValue(new Error(message));
    await expect(checkApprovedBetaUpdate()).rejects.toThrow(message);
    await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
      'Check for a signed Beta update before installing.',
    );
  },
);

it('allows a successful fresh check after an offline failure', async () => {
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
  });
  mocks.check
    .mockRejectedValueOnce(new Error('offline'))
    .mockResolvedValueOnce({
      currentVersion: '0.1.1-1',
      version: '0.1.1-2',
      rawJson: {},
      close: vi.fn(),
    });
  await expect(checkApprovedBetaUpdate()).rejects.toThrow('offline');
  await expect(checkApprovedBetaUpdate()).resolves.toMatchObject({
    version: '0.1.1-2',
  });
  expect(mocks.check).toHaveBeenCalledTimes(2);
});

it('coalesces overlapping checks into one updater request', async () => {
  let release!: () => void;
  const blocked = new Promise<null>((resolve) => {
    release = () => resolve(null);
  });
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
  });
  mocks.check.mockReturnValue(blocked);

  const first = checkApprovedBetaUpdate();
  const second = checkApprovedBetaUpdate();
  await vi.waitFor(() => expect(mocks.check).toHaveBeenCalledOnce());
  release();
  await expect(Promise.all([first, second])).resolves.toEqual([null, null]);
});

it('refuses installation until an approved update has been checked', async () => {
  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'Check for a signed Beta update before installing.',
  );
});

it('does not contact the installer when the native operation gate rejects the update', async () => {
  const downloadAndInstall = vi.fn();
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
    endpoint:
      'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
  });
  mocks.check.mockResolvedValue({
    currentVersion: '0.1.1-1',
    version: '0.1.1-2',
    rawJson: {},
    close: vi.fn(),
    downloadAndInstall,
  });
  mocks.beginInstallation.mockRejectedValue(
    new Error('Finish every active ingest job first.'),
  );
  await checkApprovedBetaUpdate();

  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'Finish every active ingest job first.',
  );
  expect(downloadAndInstall).not.toHaveBeenCalled();
  expect(mocks.endInstallation).not.toHaveBeenCalled();
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
  expect(mocks.beginInstallation).toHaveBeenCalledOnce();
  expect(mocks.endInstallation).toHaveBeenCalledOnce();
  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'Check for a signed Beta update before installing.',
  );
});

it('rejects overlapping installation attempts', async () => {
  let release!: () => void;
  const download = new Promise<void>((resolve) => {
    release = resolve;
  });
  const downloadAndInstall = vi.fn().mockReturnValue(download);
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
  });
  mocks.check.mockResolvedValue({
    currentVersion: '0.1.1-1',
    version: '0.1.1-2',
    rawJson: {},
    close: vi.fn(),
    downloadAndInstall,
  });
  await checkApprovedBetaUpdate();

  const first = installApprovedBetaUpdate(vi.fn());
  await vi.waitFor(() => expect(downloadAndInstall).toHaveBeenCalledOnce());
  await expect(installApprovedBetaUpdate(vi.fn())).rejects.toThrow(
    'An approved update installation is already in progress.',
  );
  release();
  await first;
});

it('releases the native operation gate after a successful installer return', async () => {
  const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
  mocks.configuration.mockResolvedValue({
    configured: true,
    channel: 'Beta',
    endpoint:
      'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
  });
  mocks.check.mockResolvedValue({
    currentVersion: '0.1.1-1',
    version: '0.1.1-2',
    rawJson: {},
    close: vi.fn(),
    downloadAndInstall,
  });
  await checkApprovedBetaUpdate();

  await installApprovedBetaUpdate(vi.fn());
  expect(mocks.beginInstallation).toHaveBeenCalledOnce();
  expect(mocks.endInstallation).toHaveBeenCalledOnce();
});

// @vitest-environment jsdom
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  native: true,
  settings: {
    deviceId: 'device-1',
    masterBinPath: 'C:\\Master',
    workingProjectsPath: 'C:\\Projects',
    catalogueInMasterBin: true,
    sharedCataloguePath: '',
    thumbnailCachePath: 'C:\\Cache',
    thumbnailCacheLimitBytes: 2147483648,
    ingestDateOrganization: 'original-file-dates' as const,
    dateFolderPattern: 'yyyymmdd',
    initializedCataloguePath: 'C:\\Master\\.pimp',
    projectPaths: {},
  },
  catalogue: { media: [], projects: [], updatedAt: '' },
  saveSettings: vi.fn(),
  rescan: vi.fn(),
}));

vi.mock('./bridge', () => ({
  nativeAvailable: () => mocks.native,
  emptySettings: () => ({}),
  emptyCatalogue: () => ({ media: [], projects: [], updatedAt: '' }),
  desktopBridge: {
    loadSettings: vi.fn(async () => mocks.settings),
    saveSettings: mocks.saveSettings,
    loadCatalogue: vi.fn(async () => mocks.catalogue),
    onProgress: vi.fn(async () => () => undefined),
    onFileDrop: vi.fn(async () => () => undefined),
    chooseFolder: vi.fn(),
    chooseFiles: vi.fn(async () => []),
    startIngest: vi.fn(),
    cancelIngest: vi.fn(),
    updateMedia: vi.fn(),
    createProject: vi.fn(),
    rescan: mocks.rescan,
    enrichMedia: vi.fn(async () => 0),
    thumbnailUrl: (path: string) => path,
  },
}));

import { App } from './App';
import { datePatternPreview } from './date-pattern';

describe('native application integration', () => {
  it('previews date, 24-hour and 12-hour folder tokens', () => {
    expect(datePatternPreview('yyyymmdd')).toBe('20261023');
    expect(datePatternPreview('yyyy-MM-dd HH-mm')).toBe('2026-10-23 17-42');
    expect(datePatternPreview('yyyy MM dd hh-mm a')).toBe(
      '2026 10 23 05-42 PM',
    );
  });
  beforeEach(() => {
    mocks.native = true;
    mocks.saveSettings.mockReset();
    mocks.rescan.mockReset();
  });

  it('shows a useful persisted empty library without demo media', async () => {
    render(<App />);
    expect(await screen.findByText('Your Master Bin is ready')).toBeVisible();
    expect(screen.queryByText('PIMP_MASTER_001.MP4')).not.toBeInTheDocument();
  });

  it('runs an explicit manual rescan from the real library', async () => {
    mocks.rescan.mockResolvedValue({ registered: 2, changed: 0, pending: 1 });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: /Rescan/ }));
    expect(mocks.rescan).toHaveBeenCalledOnce();
    expect(
      await screen.findByText(
        '2 external files registered; 0 changed and 1 still awaiting stability.',
      ),
    ).toBeVisible();
  });

  it('exposes persisted configuration rather than hard-coded paths', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    expect(screen.getByDisplayValue('C:\\Master')).toBeVisible();
    expect(screen.getByText(/Catalogue location is locked/)).toBeVisible();
  });
});

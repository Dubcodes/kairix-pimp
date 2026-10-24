// @vitest-environment jsdom
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Catalogue } from './bridge';

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
  catalogue: { media: [], projects: [], updatedAt: '' } as Catalogue,
  saveSettings: vi.fn(),
  loadCatalogue: vi.fn(),
  catalogueSignature: vi.fn(),
  jobs: [] as import('./bridge').IngestJob[],
  initializeIngestJobs: vi.fn(),
  updateIngestFileMetadata: vi.fn(),
  refreshMediaAvailability: vi.fn(),
  rescan: vi.fn(),
}));

vi.mock('./bridge', () => ({
  nativeAvailable: () => mocks.native,
  emptySettings: () => ({}),
  emptyCatalogue: () => ({ media: [], projects: [], updatedAt: '' }),
  desktopBridge: {
    loadSettings: vi.fn(async () => mocks.settings),
    saveSettings: mocks.saveSettings,
    loadCatalogue: mocks.loadCatalogue,
    catalogueSignature: mocks.catalogueSignature,
    refreshMediaAvailability: mocks.refreshMediaAvailability,
    initializeIngestJobs: mocks.initializeIngestJobs,
    listIngestJobs: vi.fn(async () => []),
    updateIngestFileMetadata: mocks.updateIngestFileMetadata,
    pauseIngest: vi.fn(),
    resumeIngest: vi.fn(),
    retryIngest: vi.fn(),
    onProgress: vi.fn(async () => () => undefined),
    onIngestJobsChanged: vi.fn(async () => () => undefined),
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
    mocks.loadCatalogue.mockReset();
    mocks.loadCatalogue.mockImplementation(async () => mocks.catalogue);
    mocks.catalogueSignature.mockReset();
    mocks.catalogueSignature.mockResolvedValue('initial-signature');
    mocks.jobs = [];
    mocks.initializeIngestJobs.mockReset();
    mocks.initializeIngestJobs.mockImplementation(async () => mocks.jobs);
    mocks.updateIngestFileMetadata.mockReset();
    mocks.refreshMediaAvailability.mockReset();
    mocks.refreshMediaAvailability.mockResolvedValue(false);
    mocks.rescan.mockReset();
    mocks.catalogue = { media: [], projects: [], updatedAt: '' };
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

  it.each([
    ['tag-only', ['Remote tag'], [], 'Remote tag'],
    ['project-only', [], ['remote-project'], 'Remote Project'],
  ])(
    'refreshes the open library after a remote %s event',
    async (_kind, tags, projectIds, visibleMetadata) => {
      vi.useFakeTimers();
      try {
        render(<App />);
        await act(async () => {
          await Promise.resolve();
          await Promise.resolve();
          await Promise.resolve();
        });
        mocks.catalogue = {
          media: [
            {
              id: 'media-1',
              relativePath: '2026-10-24/clip.mp4',
              originalFilename: 'clip.mp4',
              sha256: 'abc',
              captureDateProvenance: 'unknown',
              ingestDate: '2026-10-24',
              fileType: 'mp4',
              tags,
              projectIds,
              notes: '',
              technical: { sizeBytes: 10 },
              available: true,
              needsReview: false,
            },
          ],
          projects: projectIds.length
            ? [
                {
                  id: 'remote-project',
                  name: 'Remote Project',
                  workingDirectory: '.',
                  mediaIds: ['media-1'],
                  notes: '',
                  tags: [],
                  createdAt: '2026-10-24T00:00:00Z',
                },
              ]
            : [],
          updatedAt: '2026-10-24T00:00:00Z',
        };
        mocks.catalogueSignature.mockResolvedValue('remote-signature');
        await act(async () => {
          await vi.advanceTimersByTimeAsync(3_000);
        });
        expect(screen.getByText(visibleMetadata)).toBeVisible();
        expect(mocks.rescan).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    },
  );

  it('keeps the library usable while editing metadata on a copying file', async () => {
    mocks.jobs = [
      {
        schemaVersion: 1,
        id: 'job-1',
        state: 'copying',
        createdAt: '2026-10-24T00:00:00Z',
        updatedAt: '2026-10-24T00:00:00Z',
        batchTimestamp: '2026-10-24T00:00:00Z',
        files: [
          {
            id: 'file-1',
            mediaId: 'media-1',
            sourcePath: 'E:/clip.mp4',
            destinationPath: 'C:/Master/20261024/clip.mp4',
            relativePath: '20261024/clip.mp4',
            originalFilename: 'clip.mp4',
            sizeBytes: 100,
            bytesTransferred: 25,
            state: 'copying',
            metadata: {
              tags: [],
              projectIds: [],
              notes: '',
              favourite: false,
            },
          },
        ],
        unsupported: [],
        failures: [],
        currentFile: 'clip.mp4',
        bytesTotal: 100,
        bytesTransferred: 25,
        filesCompleted: 0,
      },
    ];
    mocks.updateIngestFileMetadata.mockImplementation(
      async () => mocks.jobs[0],
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: /1 active/ }));
    const tags = screen.getByPlaceholderText('Track, onboard, interview');
    await user.type(tags, 'Race');
    expect(mocks.updateIngestFileMetadata).toHaveBeenLastCalledWith(
      'job-1',
      'file-1',
      expect.objectContaining({ tags: ['Race'] }),
    );
    await user.type(
      screen.getByRole('textbox', { name: 'Description' }),
      'Lead lap',
    );
    await user.click(screen.getByRole('checkbox', { name: 'Favourite' }));
    expect(mocks.updateIngestFileMetadata).toHaveBeenLastCalledWith(
      'job-1',
      'file-1',
      expect.objectContaining({
        tags: ['Race'],
        notes: 'Lead lap',
        favourite: true,
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Library' }));
    expect(screen.getByRole('heading', { name: 'Library' })).toBeVisible();
  });

  it('refreshes local availability without a catalogue event or media rescan', async () => {
    vi.useFakeTimers();
    try {
      render(<App />);
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      mocks.catalogue = {
        media: [
          {
            id: 'arrived',
            relativePath: '20261024/arrived.mp4',
            originalFilename: 'arrived.mp4',
            sha256: 'abc',
            captureDateProvenance: 'unknown',
            ingestDate: '2026-10-24',
            fileType: 'mp4',
            tags: [],
            projectIds: [],
            notes: '',
            technical: { sizeBytes: 10 },
            available: true,
            needsReview: false,
          },
        ],
        projects: [],
        updatedAt: '2026-10-24T00:00:00Z',
      };
      mocks.refreshMediaAvailability.mockResolvedValue(true);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3_000);
      });
      expect(screen.getByText('arrived.mp4')).toBeVisible();
      expect(mocks.rescan).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});

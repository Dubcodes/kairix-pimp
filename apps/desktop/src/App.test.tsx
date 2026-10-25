// @vitest-environment jsdom
import { act, render, screen, waitFor } from '@testing-library/react';
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
  jobListener: undefined as
    ((jobs: import('./bridge').IngestJob[]) => void) | undefined,
  refreshMediaAvailability: vi.fn(),
  rescan: vi.fn(),
  resolveMediaPath: vi.fn(),
  revealMediaInExplorer: vi.fn(),
  openMediaFolder: vi.fn(),
  mediaToolsStatus: vi.fn(),
  chooseFiles: vi.fn(),
  inventorySources: vi.fn(),
  updateMedia: vi.fn(),
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
    onIngestJobsChanged: vi.fn(async (handler) => {
      mocks.jobListener = handler;
      return () => undefined;
    }),
    onFileDrop: vi.fn(async () => () => undefined),
    chooseFolder: vi.fn(),
    chooseFiles: mocks.chooseFiles,
    inventorySources: mocks.inventorySources,
    startIngest: vi.fn(),
    cancelIngest: vi.fn(),
    updateMedia: mocks.updateMedia,
    createProject: vi.fn(),
    rescan: mocks.rescan,
    enrichMedia: vi.fn(async () => 0),
    mediaToolsStatus: mocks.mediaToolsStatus,
    resolveMediaPath: mocks.resolveMediaPath,
    revealMediaInExplorer: mocks.revealMediaInExplorer,
    openMediaFolder: mocks.openMediaFolder,
    loadThumbnail: vi.fn(async () => null),
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
    mocks.jobListener = undefined;
    mocks.refreshMediaAvailability.mockReset();
    mocks.refreshMediaAvailability.mockResolvedValue(false);
    mocks.rescan.mockReset();
    mocks.resolveMediaPath.mockReset();
    mocks.resolveMediaPath.mockResolvedValue('C:\\Master\\20261024\\clip.mp4');
    mocks.revealMediaInExplorer.mockReset();
    mocks.openMediaFolder.mockReset();
    mocks.mediaToolsStatus.mockReset();
    mocks.mediaToolsStatus.mockResolvedValue({
      ffmpegAvailable: true,
      ffprobeAvailable: true,
    });
    mocks.chooseFiles.mockReset();
    mocks.chooseFiles.mockResolvedValue([]);
    mocks.inventorySources.mockReset();
    mocks.inventorySources.mockResolvedValue({
      files: [],
      excluded: [],
      issues: [],
      totalFileCount: 0,
      totalBytes: 0,
      copyFileCount: 0,
      copyBytes: 0,
    });
    mocks.updateMedia.mockReset();
    mocks.updateMedia.mockImplementation(async () => mocks.catalogue);
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

  it('shows a complete source inventory before enabling ingest', async () => {
    mocks.chooseFiles.mockResolvedValue(['E:\\XDROOT\\Clip\\A001.MXF']);
    mocks.inventorySources.mockResolvedValue({
      files: [
        {
          sourcePath: 'E:\\XDROOT\\Clip\\A001.MXF',
          sourceRelativePath: 'XDROOT/Clip/A001.MXF',
          plannedRelativePath: '20261026/XDROOT/Clip/A001.MXF',
          sizeBytes: 4096,
          kind: 'video',
          analysisSupported: false,
          preserveHierarchy: true,
        },
      ],
      excluded: [
        {
          path: 'E:\\XDROOT\\.syncthing.A002.MXF.tmp',
          reason: 'Syncthing temporary file',
          sizeBytes: 4,
        },
      ],
      issues: [],
      totalFileCount: 2,
      totalBytes: 4100,
      copyFileCount: 1,
      copyBytes: 4096,
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Ingest' }));
    await user.click(screen.getByRole('button', { name: 'Choose files' }));
    expect(
      await screen.findByText('1 of 2 files will be copied (4 KB)'),
    ).toBeVisible();
    expect(
      screen.getByText(/Excluded:.*Syncthing temporary file/),
    ).toBeVisible();
    expect(
      screen.getByText('XDROOT/Clip/A001.MXF → 20261026/XDROOT/Clip/A001.MXF'),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Start verified ingest' }),
    ).toBeEnabled();
  });

  it('exposes persisted configuration rather than hard-coded paths', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    expect(screen.getByDisplayValue('C:\\Master')).toBeVisible();
    expect(screen.getByText(/Catalogue location is locked/)).toBeVisible();
  });

  it('offers validated Windows file actions for one locally available recording', async () => {
    mocks.catalogue = {
      media: [
        {
          id: 'media-1',
          relativePath: '20261024/clip.mp4',
          originalFilename: 'clip.mp4',
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
    const user = userEvent.setup();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    render(<App />);
    await user.click(await screen.findByText('clip.mp4'));

    await user.click(
      screen.getByRole('button', { name: /Reveal in Explorer/ }),
    );
    await user.click(screen.getByRole('button', { name: /Open date folder/ }));
    await user.click(screen.getByRole('button', { name: /^Copy path$/ }));

    expect(mocks.revealMediaInExplorer).toHaveBeenCalledWith('media-1');
    expect(mocks.openMediaFolder).toHaveBeenCalledWith('media-1');
    expect(writeText).toHaveBeenCalledWith('C:\\Master\\20261024\\clip.mp4');
  });

  it('supports desktop selection, batch favourites, multi-path copy and context actions', async () => {
    mocks.catalogue = {
      media: ['one', 'two', 'three'].map((name, index) => ({
        id: `media-${index + 1}`,
        relativePath: `20261024/${name}.mp4`,
        originalFilename: `${name}.mp4`,
        sha256: name,
        captureDateProvenance: 'unknown' as const,
        ingestDate: '2026-10-24',
        fileType: 'mp4',
        tags: [],
        projectIds: [],
        notes: '',
        favourite: false,
        technical: { sizeBytes: 10 },
        available: true,
        needsReview: false,
      })),
      projects: [],
      updatedAt: '2026-10-24T00:00:00Z',
    };
    mocks.resolveMediaPath.mockImplementation(
      async (mediaId: string) => `C:\\Master\\${mediaId}.mp4`,
    );
    const user = userEvent.setup();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    render(<App />);

    await user.click(await screen.findByText('one.mp4'));
    await user.keyboard('{Control>}');
    await user.click(screen.getByText('three.mp4'));
    await user.keyboard('{/Control}');
    expect(screen.getByText('2 selected')).toBeVisible();

    await user.keyboard('{Shift>}');
    await user.click(screen.getByText('two.mp4'));
    await user.keyboard('{/Shift}');
    expect(screen.getByRole('option', { name: /one.mp4/ })).toHaveAttribute(
      'aria-selected',
      'false',
    );
    expect(screen.getByRole('option', { name: /two.mp4/ })).toHaveAttribute(
      'aria-selected',
      'true',
    );

    await user.keyboard('{Control>}a{/Control}');
    expect(screen.getByText('3 selected')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Favourite' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['media-1', 'media-2', 'media-3'],
      favourite: true,
    });

    await user.click(
      screen.getByRole('button', { name: /Copy paths for Premiere/ }),
    );
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        'C:\\Master\\media-1.mp4\r\nC:\\Master\\media-2.mp4\r\nC:\\Master\\media-3.mp4',
      ),
    );

    await user.pointer({
      keys: '[MouseRight]',
      target: screen.getByText('two.mp4'),
    });
    expect(
      screen.getByRole('menu', { name: 'Recording context menu' }),
    ).toBeVisible();
  });

  it('explains optional feature loss when FFmpeg tools are missing', async () => {
    mocks.mediaToolsStatus.mockResolvedValue({
      ffmpegAvailable: false,
      ffprobeAvailable: false,
    });
    render(<App />);
    expect(
      await screen.findByText(/FFmpeg thumbnails are disabled/),
    ).toHaveTextContent(
      'Verified ingest, catalogue, search and metadata remain available',
    );
    expect(screen.getByText('winget install Gyan.FFmpeg')).toBeVisible();
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
    expect(mocks.updateIngestFileMetadata).toHaveBeenCalledWith(
      'job-1',
      'file-1',
      { notes: 'Lead lap' },
    );
    await user.click(screen.getByRole('checkbox', { name: 'Favourite' }));
    expect(mocks.updateIngestFileMetadata).toHaveBeenLastCalledWith(
      'job-1',
      'file-1',
      { favourite: true },
    );
    await user.click(screen.getByRole('button', { name: 'Library' }));
    expect(screen.getByRole('heading', { name: 'Library' })).toBeVisible();
  });

  it('keeps an unsaved favourite edit across interleaved progress and completion updates', async () => {
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
            metadata: { tags: [], projectIds: [], notes: '', favourite: false },
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
    let resolveWrite: ((job: import('./bridge').IngestJob) => void) | undefined;
    mocks.updateIngestFileMetadata.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveWrite = resolve;
        }),
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: /1 active/ }));
    const favourite = screen.getByRole('checkbox', { name: 'Favourite' });
    await user.click(favourite);
    expect(favourite).toBeChecked();

    const staleCompletion = structuredClone(mocks.jobs);
    staleCompletion[0].state = 'completed';
    staleCompletion[0].files[0].state = 'completed';
    staleCompletion[0].files[0].bytesTransferred = 100;
    await act(async () => mocks.jobListener?.(staleCompletion));
    expect(screen.getByRole('checkbox', { name: 'Favourite' })).toBeChecked();

    await act(async () => resolveWrite?.(staleCompletion[0]));
  });

  it('reports a metadata persistence error without reverting the local edit', async () => {
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
            metadata: { tags: [], projectIds: [], notes: '', favourite: false },
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
    mocks.updateIngestFileMetadata.mockRejectedValue(
      new Error('simulated metadata write failure'),
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: /1 active/ }));
    await user.click(screen.getByRole('checkbox', { name: 'Favourite' }));
    expect(screen.getByRole('checkbox', { name: 'Favourite' })).toBeChecked();
    expect(
      await screen.findByText('simulated metadata write failure'),
    ).toBeVisible();
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

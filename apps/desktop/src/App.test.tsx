// @vitest-environment jsdom
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Catalogue, DeviceSettings, IngestJob } from './bridge';

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
    hoverPreviewsEnabled: true,
    previewCacheLimitBytes: 536870912,
    ingestDateOrganization: 'original-file-dates' as const,
    dateFolderPattern: 'yyyymmdd',
    initializedCataloguePath: 'C:\\Master\\.pimp',
    projectPaths: {},
    startWithWindows: false,
    closeToTray: true,
    startupPage: 'Library' as const,
    checkForUpdates: true,
    theme: 'graphite' as const,
    density: 'comfortable' as const,
    showAttachedAudioSeparately: false,
    sidebarCollapsed: false,
    libraryFolderSort: 'newest' as const,
    libraryFolderOrder: [],
  } as DeviceSettings,
  catalogue: { media: [], projects: [], updatedAt: '' } as Catalogue,
  saveSettings: vi.fn(),
  loadCatalogue: vi.fn(),
  catalogueSignature: vi.fn(),
  jobs: [] as import('./bridge').IngestJob[],
  initializeIngestJobs: vi.fn(),
  clearCompletedIngestJobs: vi.fn(),
  updateIngestFileMetadata: vi.fn(),
  jobListener: undefined as
    ((jobs: import('./bridge').IngestJob[]) => void) | undefined,
  refreshMediaAvailability: vi.fn(),
  rescan: vi.fn(),
  discoveryListener: undefined as
    ((status: import('./bridge').DiscoveryStatus) => void) | undefined,
  resolveMediaPath: vi.fn(),
  revealMediaInExplorer: vi.fn(),
  openMediaFolder: vi.fn(),
  mediaToolsStatus: vi.fn(),
  chooseFiles: vi.fn(),
  inventorySources: vi.fn(),
  updateMedia: vi.fn(),
  openSupportLink: vi.fn(),
  updaterConfiguration: vi.fn(),
  checkApprovedBetaUpdate: vi.fn(),
  installApprovedBetaUpdate: vi.fn(),
  retryProjectSynchronization: vi.fn(),
  inventoryProject: vi.fn(),
  updateProject: vi.fn(),
  openProjectFolder: vi.fn(),
  revealProjectAsset: vi.fn(),
  loadHoverPreview: vi.fn(),
  hoverPreviewCacheUsage: vi.fn(),
  clearHoverPreviewCache: vi.fn(),
  loadThumbnailSources: vi.fn(),
  startIngest: vi.fn(),
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
    clearCompletedIngestJobs: mocks.clearCompletedIngestJobs,
    updateIngestFileMetadata: mocks.updateIngestFileMetadata,
    pauseIngest: vi.fn(),
    resumeIngest: vi.fn(),
    retryIngest: vi.fn(),
    onProgress: vi.fn(async () => () => undefined),
    onIngestJobsChanged: vi.fn(async (handler) => {
      mocks.jobListener = handler;
      return () => undefined;
    }),
    onDiscoveryProgress: vi.fn(async (handler) => {
      mocks.discoveryListener = handler;
      return () => undefined;
    }),
    onCatalogueChanged: vi.fn(async () => () => undefined),
    getDiscoveryStatus: vi.fn(async () => ({
      running: false,
      full: false,
      filesProcessed: 0,
      totalFiles: 0,
      registered: 0,
      changed: 0,
      pending: 0,
      errors: 0,
    })),
    logUiError: vi.fn(),
    appInfo: vi.fn(async () => ({
      version: '0.1.1-1',
      buildCommit: 'test-commit',
      edition: 'Kairix PIMP Light',
      releaseChannel: 'Beta',
    })),
    openSupportLink: mocks.openSupportLink,
    openLogsFolder: vi.fn(),
    updaterConfiguration: mocks.updaterConfiguration,
    onFileDrop: vi.fn(async () => () => undefined),
    chooseFolder: vi.fn(),
    chooseFiles: mocks.chooseFiles,
    inventorySources: mocks.inventorySources,
    startIngest: mocks.startIngest,
    cancelIngest: vi.fn(),
    updateMedia: mocks.updateMedia,
    updateProject: mocks.updateProject,
    createProject: vi.fn(),
    retryProjectSynchronization: mocks.retryProjectSynchronization,
    inventoryProject: mocks.inventoryProject,
    openProjectFolder: mocks.openProjectFolder,
    revealProjectAsset: mocks.revealProjectAsset,
    rescan: mocks.rescan,
    enrichMedia: vi.fn(async () => 0),
    mediaToolsStatus: mocks.mediaToolsStatus,
    resolveMediaPath: mocks.resolveMediaPath,
    revealMediaInExplorer: mocks.revealMediaInExplorer,
    openMediaFolder: mocks.openMediaFolder,
    loadThumbnail: vi.fn(async () => null),
    loadThumbnailSources: mocks.loadThumbnailSources,
    loadHoverPreview: mocks.loadHoverPreview,
    hoverPreviewCacheUsage: mocks.hoverPreviewCacheUsage,
    clearHoverPreviewCache: mocks.clearHoverPreviewCache,
    thumbnailUrl: (path: string) => path,
  },
}));

vi.mock('./updates', () => ({
  checkApprovedBetaUpdate: mocks.checkApprovedBetaUpdate,
  installApprovedBetaUpdate: mocks.installApprovedBetaUpdate,
}));

import { App } from './App';
import { datePatternPreview } from './date-pattern';

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
};

const testJob = (id: string, state: IngestJob['state']): IngestJob => ({
  schemaVersion: 3,
  id,
  state,
  createdAt: '2026-10-28T00:00:00Z',
  updatedAt: '2026-10-28T01:00:00Z',
  batchTimestamp: '2026-10-28T00:00:00Z',
  files: [
    {
      id: `${id}-file`,
      mediaId: `${id}-media`,
      sourcePath: `C:\\Card\\${id}.mp4`,
      destinationPath: `C:\\Master\\${id}.mp4`,
      relativePath: `${id}.mp4`,
      originalFilename: `${id}.mp4`,
      sizeBytes: 1024,
      bytesTransferred: state === 'completed' ? 1024 : 256,
      state,
      metadata: { tags: [], projectIds: [], notes: '', favourite: false },
    },
  ],
  unsupported: [],
  failures: state === 'failed' ? ['Copy failed'] : [],
  bytesTotal: 1024,
  bytesTransferred: state === 'completed' ? 1024 : 256,
  filesCompleted: state === 'completed' ? 1 : 0,
});

describe('native application integration', () => {
  it('previews date, 24-hour and 12-hour folder tokens', () => {
    const now = new Date(2026, 9, 23, 17, 42, 9);
    expect(datePatternPreview('yyyymmdd', now)).toBe('20261023');
    expect(datePatternPreview('yyyy-MM-dd HH-mm', now)).toBe(
      '2026-10-23 17-42',
    );
    expect(datePatternPreview('yyyy MM dd hh-mm a', now)).toBe(
      '2026 10 23 05-42 PM',
    );
  });
  beforeEach(() => {
    mocks.native = true;
    mocks.settings = {
      deviceId: 'device-1',
      masterBinPath: 'C:\\Master',
      workingProjectsPath: 'C:\\Projects',
      catalogueInMasterBin: true,
      sharedCataloguePath: '',
      thumbnailCachePath: 'C:\\Cache',
      thumbnailCacheLimitBytes: 2147483648,
      hoverPreviewsEnabled: true,
      previewCacheLimitBytes: 536870912,
      ingestDateOrganization: 'original-file-dates',
      dateFolderPattern: 'yyyymmdd',
      initializedCataloguePath: 'C:\\Master\\.pimp',
      projectPaths: {},
      startWithWindows: false,
      closeToTray: true,
      startupPage: 'Library',
      checkForUpdates: true,
      theme: 'graphite',
      density: 'comfortable',
      showAttachedAudioSeparately: false,
      sidebarCollapsed: false,
      libraryFolderSort: 'newest',
      libraryFolderOrder: [],
    };
    mocks.saveSettings.mockReset();
    mocks.loadCatalogue.mockReset();
    mocks.loadCatalogue.mockImplementation(async () => mocks.catalogue);
    mocks.catalogueSignature.mockReset();
    mocks.catalogueSignature.mockResolvedValue('initial-signature');
    mocks.jobs = [];
    mocks.initializeIngestJobs.mockReset();
    mocks.initializeIngestJobs.mockImplementation(async () => mocks.jobs);
    mocks.clearCompletedIngestJobs.mockReset();
    mocks.clearCompletedIngestJobs.mockImplementation(
      async (jobId?: string) => {
        mocks.jobs = mocks.jobs.filter(
          (job) => job.state !== 'completed' || (jobId && job.id !== jobId),
        );
        return mocks.jobs;
      },
    );
    mocks.updateIngestFileMetadata.mockReset();
    mocks.jobListener = undefined;
    mocks.refreshMediaAvailability.mockReset();
    mocks.refreshMediaAvailability.mockResolvedValue(false);
    mocks.rescan.mockReset();
    mocks.rescan.mockResolvedValue({
      scanned: 0,
      registered: 0,
      changed: 0,
      pending: 0,
      errors: 0,
      full: false,
      completedAt: '2026-10-26T00:00:00Z',
    });
    mocks.resolveMediaPath.mockReset();
    mocks.resolveMediaPath.mockResolvedValue('C:\\Master\\20261024\\clip.mp4');
    mocks.revealMediaInExplorer.mockReset();
    mocks.revealMediaInExplorer.mockResolvedValue(undefined);
    mocks.openMediaFolder.mockReset();
    mocks.openMediaFolder.mockResolvedValue(undefined);
    mocks.updaterConfiguration.mockReset();
    mocks.updaterConfiguration.mockResolvedValue({
      configured: false,
      channel: 'Beta',
      endpoint:
        'https://github.com/Dubcodes/kairix-pimp/releases/download/beta-channel/latest.json',
      reason:
        'Signed updates are not configured in this build. Use the reviewed NSIS installer from GitHub Releases.',
    });
    mocks.checkApprovedBetaUpdate.mockReset();
    mocks.installApprovedBetaUpdate.mockReset();
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
    mocks.openSupportLink.mockReset();
    mocks.retryProjectSynchronization.mockReset();
    mocks.retryProjectSynchronization.mockResolvedValue([]);
    mocks.inventoryProject.mockReset();
    mocks.updateProject.mockReset();
    mocks.updateProject.mockImplementation(async () => mocks.catalogue);
    mocks.openProjectFolder.mockReset();
    mocks.openProjectFolder.mockResolvedValue(undefined);
    mocks.revealProjectAsset.mockReset();
    mocks.revealProjectAsset.mockResolvedValue(undefined);
    mocks.loadHoverPreview.mockReset();
    mocks.hoverPreviewCacheUsage.mockReset();
    mocks.hoverPreviewCacheUsage.mockResolvedValue(0);
    mocks.clearHoverPreviewCache.mockReset();
    mocks.clearHoverPreviewCache.mockResolvedValue(0);
    mocks.loadThumbnailSources.mockReset();
    mocks.loadThumbnailSources.mockResolvedValue({});
    mocks.startIngest.mockReset();
    mocks.startIngest.mockResolvedValue({
      schemaVersion: 3,
      id: 'job-batch-tags',
      state: 'queued',
      createdAt: '2026-10-28T00:00:00Z',
      updatedAt: '2026-10-28T00:00:00Z',
      batchTimestamp: '2026-10-28T00:00:00Z',
      files: [],
      unsupported: [],
      failures: [],
      bytesTotal: 0,
      bytesTransferred: 0,
      filesCompleted: 0,
    });
    mocks.catalogue = { media: [], projects: [], updatedAt: '' };
  });

  it('separates active, completed and attention jobs and clears one completed entry', async () => {
    mocks.jobs = [
      testJob('active', 'copying'),
      testJob('complete', 'completed'),
      testJob('failed', 'failed'),
    ];
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole('button', { name: /1 active · 25%/ }),
    );
    expect(screen.getByText('Active 1')).toBeVisible();
    expect(screen.getByText('Completed 1')).toBeVisible();
    expect(screen.getByText('Needs attention 1')).toBeVisible();
    expect(screen.queryByText(/copied and verified/)).not.toBeInTheDocument();

    await user.click(
      screen.getByRole('button', { name: 'Completed history (1)' }),
    );
    expect(screen.getByText(/1 file copied and verified/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Clear' }));
    await waitFor(() =>
      expect(mocks.clearCompletedIngestJobs).toHaveBeenCalledWith('complete'),
    );
    expect(screen.queryByText('Completed 1')).not.toBeInTheDocument();
    expect(screen.getByText('Completed 0')).toBeVisible();
    expect(screen.getByText('Needs attention 1')).toBeVisible();
  });

  it('shows all ingests complete and removes the indicator after clearing history', async () => {
    mocks.jobs = [
      testJob('complete-1', 'completed'),
      testJob('complete-2', 'completed'),
    ];
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole('button', { name: 'All ingests complete' }),
    );
    expect(screen.getByText('Completed 2')).toBeVisible();
    expect(screen.getAllByText('All ingests complete')).toHaveLength(2);
    await user.click(screen.getByRole('button', { name: 'Clear Completed' }));
    await waitFor(() =>
      expect(mocks.clearCompletedIngestJobs).toHaveBeenCalledWith(undefined),
    );
    expect(
      screen.queryByRole('button', { name: 'All ingests complete' }),
    ).not.toBeInTheDocument();
  });

  it('offers an explicit retry for pending project manifest synchronization', async () => {
    mocks.catalogue = {
      media: [],
      projects: [
        {
          id: 'project-a',
          name: 'Simple Vlog 057',
          workingDirectory: 'C:\\Projects\\Simple Vlog 057',
          mediaIds: [],
          notes: '',
          tags: [],
          createdAt: '2026-10-28T00:00:00Z',
        },
      ],
      issues: [
        {
          code: 'project-manifest-pending',
          source: 'project-a',
          detail: 'Manifest synchronization is pending.',
        },
      ],
      updatedAt: '2026-10-28T00:00:00Z',
    };
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Projects' }));
    await user.click(
      screen.getByRole('button', { name: 'Retry project synchronization' }),
    );
    await waitFor(() =>
      expect(mocks.retryProjectSynchronization).toHaveBeenCalledWith(
        'project-a',
      ),
    );
    expect(
      await screen.findByText('Project manifest synchronization completed.'),
    ).toBeVisible();
  });

  it('opens a read-only project workspace and manages linked logical recordings', async () => {
    mocks.catalogue = {
      media: [
        {
          id: 'video',
          relativePath: '20261028/DJI_0001.MP4',
          originalFilename: 'DJI_0001.MP4',
          sha256: 'video',
          captureDate: '2026-10-28T00:00:00Z',
          captureDateProvenance: 'filesystem',
          ingestDate: '2026-10-28',
          fileType: 'mp4',
          tags: ['vlog'],
          projectIds: ['project-a'],
          notes: '',
          favourite: true,
          technical: { sizeBytes: 100 },
          attachedAudioId: 'audio',
          available: true,
          needsReview: false,
        },
        {
          id: 'audio',
          relativePath: '20261028/DJI_0001.WAV',
          originalFilename: 'DJI_0001.WAV',
          sha256: 'audio',
          captureDateProvenance: 'filesystem',
          ingestDate: '2026-10-28',
          fileType: 'wav',
          tags: [],
          projectIds: ['project-a'],
          notes: '',
          technical: { sizeBytes: 20 },
          available: true,
          needsReview: false,
        },
        {
          id: 'graphic',
          relativePath: '20261028/title.png',
          originalFilename: 'title.png',
          sha256: 'graphic',
          captureDateProvenance: 'filesystem',
          ingestDate: '2026-10-28',
          fileType: 'png',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 10 },
          available: true,
          needsReview: false,
        },
      ],
      projects: [
        {
          id: 'project-a',
          name: 'Simple Vlog 057',
          workingDirectory: 'C:\\Projects\\Simple Vlog 057',
          mediaIds: ['video'],
          notes: 'Original notes',
          tags: ['vlog'],
          createdAt: '2026-10-28T00:00:00Z',
        },
      ],
      updatedAt: '2026-10-28T00:00:00Z',
    };
    mocks.inventoryProject.mockResolvedValue({
      projectId: 'project-a',
      rootPath: 'C:\\Projects\\Simple Vlog 057',
      itemCount: 3,
      sizeBytes: 300,
      mainPremiereProject: 'Simple Vlog 057.prproj',
      scannedAt: '2026-10-28T01:00:00Z',
      items: [
        {
          relativePath: 'Simple Vlog 057.prproj',
          name: 'Simple Vlog 057.prproj',
          category: 'premiere-project',
          sizeBytes: 100,
        },
        {
          relativePath: 'Adobe Premiere Pro Auto-Save/backup.prproj',
          name: 'backup.prproj',
          category: 'premiere-auto-save',
          sizeBytes: 100,
        },
        {
          relativePath: 'Exports/final.mp4',
          name: 'final.mp4',
          category: 'exports',
          sizeBytes: 100,
        },
      ],
    });
    mocks.updateMedia.mockImplementation(async () => mocks.catalogue);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Projects' }));
    await user.click(
      screen.getByRole('button', { name: /Simple Vlog 057.*1 assigned/ }),
    );

    expect(
      (await screen.findAllByText('Simple Vlog 057.prproj'))[0],
    ).toBeVisible();
    expect(screen.getByText('Premiere auto-saves')).toBeVisible();
    expect(screen.getByText('Exports')).toBeVisible();
    await user.click(
      screen.getByRole('button', { name: /Open project folder/ }),
    );
    expect(mocks.openProjectFolder).toHaveBeenCalledWith('project-a');

    await user.click(screen.getByRole('checkbox', { name: /DJI_0001.MP4/ }));
    await user.click(screen.getByRole('button', { name: 'Mark favourite' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['video', 'audio'],
      favourite: true,
    });
    const recordingNotes = screen.getByRole('textbox', {
      name: 'Recording description',
    });
    await user.type(recordingNotes, 'Primary clip note');
    await user.click(screen.getByRole('button', { name: 'Save description' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['video'],
      notes: 'Primary clip note',
    });
    await user.click(screen.getAllByRole('button', { name: 'Reveal' })[0]);
    expect(mocks.revealMediaInExplorer).toHaveBeenCalledWith('video');
    expect(
      screen.getByRole('checkbox', { name: /DJI_0001.MP4/ }),
    ).toBeChecked();
    await user.click(
      screen.getByRole('button', { name: 'Copy original paths' }),
    );
    await waitFor(() =>
      expect(mocks.resolveMediaPath).toHaveBeenCalledWith('audio'),
    );
    await user.click(screen.getByRole('button', { name: 'Remove assignment' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['video', 'audio'],
      removeProjectIds: ['project-a'],
    });

    await user.click(screen.getByRole('checkbox', { name: /title.png/ }));
    await user.click(screen.getByRole('button', { name: 'Add to project' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['graphic'],
      addProjectIds: ['project-a'],
    });

    const notes = screen.getByRole('textbox', {
      name: 'Description and notes',
    });
    await user.clear(notes);
    await user.type(notes, 'Updated project notes');
    await user.click(
      screen.getByRole('button', { name: 'Save project information' }),
    );
    expect(mocks.updateProject).toHaveBeenCalledWith({
      projectId: 'project-a',
      notes: 'Updated project notes',
      tags: ['vlog'],
    });

    screen.getAllByRole('button', { name: 'View in Library' })[0].focus();
    await user.keyboard('{Enter}');
    expect(
      screen.getByRole('heading', { name: 'Media library' }),
    ).toBeVisible();
    await waitFor(() =>
      expect(
        screen.getByRole('option', { name: /DJI_0001.MP4/ }),
      ).toHaveFocus(),
    );
    await user.click(screen.getByRole('button', { name: 'Simple Vlog 057' }));
    expect(
      screen.getByRole('heading', { name: 'Simple Vlog 057' }),
    ).toBeVisible();
  });

  it('ignores superseded project inventory results and failures', async () => {
    mocks.catalogue = {
      media: [],
      projects: [
        {
          id: 'project-a',
          name: 'Project A',
          workingDirectory: 'C:\\Projects\\A',
          mediaIds: [],
          notes: '',
          tags: [],
          createdAt: '2026-10-28T00:00:00Z',
        },
        {
          id: 'project-b',
          name: 'Project B',
          workingDirectory: 'C:\\Projects\\B',
          mediaIds: [],
          notes: '',
          tags: [],
          createdAt: '2026-10-28T00:00:00Z',
        },
      ],
      updatedAt: '2026-10-28T00:00:00Z',
    };
    const first = deferred<import('./bridge').ProjectInventory>();
    const second = deferred<import('./bridge').ProjectInventory>();
    mocks.inventoryProject
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Projects' }));
    await user.click(
      screen.getByRole('button', { name: /Project A.*0 assigned/ }),
    );
    await user.click(screen.getByRole('button', { name: /Back to Projects/ }));
    await user.click(
      screen.getByRole('button', { name: /Project B.*0 assigned/ }),
    );

    second.resolve({
      projectId: 'project-b',
      rootPath: 'C:\\Projects\\B',
      itemCount: 1,
      sizeBytes: 10,
      mainPremiereProject: 'Project B.prproj',
      scannedAt: '2026-10-28T01:00:00Z',
      items: [],
    });
    expect(await screen.findByText('Project B.prproj')).toBeVisible();
    first.reject(new Error('Project A drive disappeared'));
    await act(async () => first.promise.catch(() => undefined));
    expect(screen.getByText('Project B.prproj')).toBeVisible();
    expect(
      screen.queryByText(/Project A drive disappeared/),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Refresh inventory' }),
    ).toBeEnabled();
  });

  it('shows a useful persisted empty library without demo media', async () => {
    render(<App />);
    expect(await screen.findByText('Your Master Bin is ready')).toBeVisible();
    expect(screen.queryByText('PIMP_MASTER_001.MP4')).not.toBeInTheDocument();
  });

  it('scrubs cached hover frames locally and restores the normal thumbnail', async () => {
    mocks.catalogue = {
      media: [
        {
          id: 'hover-video',
          relativePath: '20261028/hover.mp4',
          originalFilename: 'hover.mp4',
          sha256: 'hover-sha',
          captureDateProvenance: 'filesystem',
          ingestDate: '2026-10-28',
          fileType: 'mp4',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 100, durationSeconds: 100 },
          thumbnailPath: 'hover-video.jpg',
          available: true,
          needsReview: false,
          observedSize: 100,
          observedModifiedMillis: 123,
        },
      ],
      projects: [],
      updatedAt: '2026-10-28T00:00:00Z',
    };
    mocks.loadHoverPreview.mockResolvedValue({
      mediaId: 'hover-video',
      durationSeconds: 100,
      frameTimes: [0, 50, 100],
      frames: [
        'data:image/jpeg;base64,first',
        'data:image/jpeg;base64,middle',
        'data:image/jpeg;base64,last',
      ],
    });
    const user = userEvent.setup();
    const { container } = render(<App />);
    await screen.findByRole('option', { name: /hover.mp4/ });
    const thumbnail = container.querySelector(
      '.media-thumbnail',
    ) as HTMLElement;
    vi.spyOn(thumbnail, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      width: 100,
      top: 0,
      right: 100,
      bottom: 60,
      height: 60,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent.pointerEnter(thumbnail);
    await waitFor(() =>
      expect(mocks.loadHoverPreview).toHaveBeenCalledTimes(1),
    );
    await screen.findByLabelText('Move horizontally to preview video frames');
    fireEvent.pointerMove(thumbnail, { clientX: 99 });
    expect(container.querySelector('.media-thumbnail img')).toHaveAttribute(
      'src',
      'data:image/jpeg;base64,last',
    );
    expect(container.querySelector('.preview-timecode')).toHaveTextContent(
      '1:40',
    );
    fireEvent.pointerLeave(thumbnail);
    expect(
      container.querySelector('.preview-timecode'),
    ).not.toBeInTheDocument();
    fireEvent.pointerEnter(thumbnail);
    expect(mocks.loadHoverPreview).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Appearance' }));
    await user.click(
      screen.getByRole('button', { name: 'Clear preview cache' }),
    );
    await waitFor(() =>
      expect(mocks.clearHoverPreviewCache).toHaveBeenCalledTimes(1),
    );
    await user.click(screen.getByRole('button', { name: 'Library' }));
    const refreshedThumbnail = container.querySelector(
      '.media-thumbnail',
    ) as HTMLElement;
    fireEvent.pointerEnter(refreshedThumbnail);
    await waitFor(() =>
      expect(mocks.loadHoverPreview).toHaveBeenCalledTimes(2),
    );
  });

  it.each([100, 700, 3000])(
    'batches thumbnail source lookup once for %i recordings',
    async (count) => {
      mocks.catalogue = {
        media: Array.from({ length: count }, (_, index) => ({
          id: `media-${count}-${index}`,
          relativePath: `20261028/clip-${index}.mp4`,
          originalFilename: `clip-${index}.mp4`,
          sha256: `sha-${index}`,
          captureDateProvenance: 'filesystem' as const,
          ingestDate: '2026-10-28',
          fileType: 'mp4',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 100 },
          thumbnailPath: `media-${count}-${index}.jpg`,
          available: true,
          needsReview: false,
        })),
        projects: [],
        updatedAt: '2026-10-28T00:00:00Z',
      };
      render(<App />);
      await waitFor(
        () =>
          expect(
            mocks.loadThumbnailSources.mock.calls.some(
              ([mediaIds]) => mediaIds.length === count,
            ),
          ).toBe(true),
        { timeout: 10_000 },
      );
      expect(
        mocks.loadThumbnailSources.mock.calls.filter(
          ([mediaIds]) => mediaIds.length === count,
        ),
      ).toHaveLength(1);
    },
    15_000,
  );

  it('persists sidebar collapse and accessible logical folder ordering', async () => {
    mocks.catalogue = {
      media: [
        {
          id: 'alpha',
          relativePath: 'Alpha/one.mp4',
          originalFilename: 'one.mp4',
          sha256: 'alpha',
          captureDate: '2026-10-20T00:00:00Z',
          captureDateProvenance: 'filesystem',
          ingestDate: '2026-10-20',
          fileType: 'mp4',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 10 },
          available: true,
          needsReview: false,
        },
        ...['two', 'three'].map((name) => ({
          id: name,
          relativePath: `Zeta/${name}.mp4`,
          originalFilename: `${name}.mp4`,
          sha256: name,
          captureDate: '2026-10-27T00:00:00Z',
          captureDateProvenance: 'filesystem' as const,
          ingestDate: '2026-10-27',
          fileType: 'mp4',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 10 },
          available: true,
          needsReview: false,
        })),
      ],
      projects: [],
      updatedAt: '2026-10-27T00:00:00Z',
    };
    mocks.saveSettings.mockImplementation(async (settings) => settings);
    const user = userEvent.setup();
    render(<App />);

    expect(
      await screen.findByRole('button', { name: /^Zeta/ }),
    ).toHaveTextContent('2 recordings');
    const folderButtons = () =>
      [
        ...document.querySelectorAll<HTMLButtonElement>(
          '.folder-row > .folder',
        ),
      ].map((button) => button.textContent);
    expect(folderButtons()[0]).toContain('Zeta');

    const orderSelect = screen.getByRole('combobox', {
      name: 'Folder order',
    });
    for (const [order, firstFolder] of [
      ['oldest', 'Alpha'],
      ['name-asc', 'Alpha'],
      ['name-desc', 'Zeta'],
      ['most-recordings', 'Zeta'],
      ['fewest-recordings', 'Alpha'],
      ['newest', 'Zeta'],
      ['name-asc', 'Alpha'],
    ] as const) {
      await user.selectOptions(orderSelect, order);
      expect(folderButtons()[0]).toContain(firstFolder);
    }
    expect(mocks.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ libraryFolderSort: 'name-asc' }),
    );

    await user.selectOptions(orderSelect, 'custom');
    await user.click(screen.getByRole('button', { name: 'Move Zeta up' }));
    await waitFor(() =>
      expect(mocks.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({
          libraryFolderSort: 'custom',
          libraryFolderOrder: ['Zeta', 'Alpha'],
        }),
      ),
    );

    await user.click(
      screen.getByRole('button', { name: 'Collapse application sidebar' }),
    );
    expect(
      screen.getByRole('button', { name: 'Expand application sidebar' }),
    ).toBeVisible();
    await waitFor(() =>
      expect(mocks.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ sidebarCollapsed: true }),
      ),
    );
  });

  it('runs an explicit manual rescan from the real library', async () => {
    mocks.rescan.mockResolvedValue({
      scanned: 3,
      registered: 2,
      changed: 0,
      pending: 1,
      errors: 0,
      full: false,
      completedAt: '2026-10-26T00:00:00Z',
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole('button', { name: /Scan for new files/ }),
    );
    expect(mocks.rescan).toHaveBeenCalledWith(false);
    expect(
      await screen.findByText(
        'Scan complete: 2 registered, 0 changed, 1 pending, 0 errors.',
      ),
    ).toBeVisible();
  });

  it('starts conservative discovery automatically for a configured library', async () => {
    render(<App />);
    await waitFor(() => expect(mocks.rescan).toHaveBeenCalledWith(false));
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
    await user.click(screen.getByText('Review 1 planned destination path'));
    expect(screen.getByTitle('XDROOT/Clip/A001.MXF')).toBeVisible();
    expect(screen.getByTitle('20261026/XDROOT/Clip/A001.MXF')).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Start verified ingest' }),
    ).toBeEnabled();
  });

  it('renders 63 long planned paths as bounded semantic rows', async () => {
    const files = Array.from({ length: 63 }, (_, index) => {
      const name = `DJI_20261028_${String(index).padStart(4, '0')}_VERY_LONG_CAMERA_RECORDING_NAME.MP4`;
      const sourceRelativePath = `DCIM/100MEDIA/NESTED/CAMERA/PACKAGE/${name}`;
      return {
        sourcePath: `E:\\DCIM\\100MEDIA\\NESTED\\CAMERA\\PACKAGE\\${name}`,
        sourceRelativePath,
        plannedRelativePath: `20261028/Card-E/DCIM/100MEDIA/NESTED/CAMERA/PACKAGE/${name}`,
        sizeBytes: 4096,
        kind: 'video' as const,
        analysisSupported: true,
        preserveHierarchy: true,
      };
    });
    mocks.chooseFiles.mockResolvedValue(files.map((file) => file.sourcePath));
    mocks.inventorySources.mockResolvedValue({
      files,
      excluded: [
        {
          path: 'E:\\DCIM\\.DS_Store',
          reason: 'temporary metadata',
          sizeBytes: 1,
        },
      ],
      issues: ['One unreadable sidecar was retained for review'],
      totalFileCount: 64,
      totalBytes: 63 * 4096 + 1,
      copyFileCount: 63,
      copyBytes: 63 * 4096,
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Ingest' }));
    await user.click(screen.getByRole('button', { name: 'Choose files' }));
    await user.click(
      await screen.findByText('Review 63 planned destination paths'),
    );
    const table = screen.getByRole('table', {
      name: 'Planned ingest destination paths',
    });
    expect(table).toHaveClass('planned-paths');
    expect(screen.getAllByRole('row')).toHaveLength(64);
    expect(screen.getByText(/63 of 64 files will be copied/)).toBeVisible();
    expect(screen.getByText(/Excluded:.*temporary metadata/)).toBeVisible();
    expect(
      screen.getByText(/Inventory issue:.*unreadable sidecar/),
    ).toBeVisible();
    expect(
      screen.getByTitle(files[62].plannedRelativePath),
    ).toBeInTheDocument();
  });

  it('persists newly created batch tags in the ingest request', async () => {
    mocks.chooseFiles.mockResolvedValue(['E:\\DCIM\\clip.mp4']);
    mocks.inventorySources.mockResolvedValue({
      files: [
        {
          sourcePath: 'E:\\DCIM\\clip.mp4',
          sourceRelativePath: 'DCIM/clip.mp4',
          plannedRelativePath: '20261028/DCIM/clip.mp4',
          sizeBytes: 100,
          kind: 'video',
          analysisSupported: true,
          preserveHierarchy: true,
        },
      ],
      excluded: [],
      issues: [],
      totalFileCount: 1,
      totalBytes: 100,
      copyFileCount: 1,
      copyBytes: 100,
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Ingest' }));
    await user.click(screen.getByRole('button', { name: 'Choose files' }));
    const tags = screen.getByRole('combobox', {
      name: 'Tags for all files in this ingest',
    });
    await user.type(tags, 'Race Day');
    await user.click(await screen.findByText('Create “Race Day”'));
    await user.click(
      screen.getByRole('button', { name: 'Start verified ingest' }),
    );
    expect(mocks.startIngest).toHaveBeenCalledWith(
      expect.objectContaining({ batchTags: ['Race Day'] }),
    );
  });

  it('exposes persisted configuration rather than hard-coded paths', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Library & Storage' }));
    expect(screen.getByDisplayValue('C:\\Master')).toBeVisible();
    expect(screen.getByText(/Catalogue location is locked/)).toBeVisible();
  });

  it('finishes saving settings without waiting for catalogue indexing', async () => {
    mocks.saveSettings.mockResolvedValue(mocks.settings);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Library & Storage' }));
    await user.click(
      screen.getByRole('button', { name: 'Save configuration' }),
    );
    expect(
      await screen.findByText(
        'Configuration saved. Library indexing will continue in the background.',
      ),
    ).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Save configuration' }),
    ).toBeEnabled();
  });

  it('persists close-to-tray and applies theme preferences', async () => {
    mocks.saveSettings.mockImplementation(async (settings) => settings);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    const closeToTray = screen.getByRole('switch', {
      name: /Close to system tray/,
    });
    expect(closeToTray).toBeChecked();
    await user.click(closeToTray);
    expect(mocks.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ closeToTray: false }),
    );

    await user.click(screen.getByRole('button', { name: 'Appearance' }));
    await user.click(screen.getByRole('radio', { name: /Midnight/ }));
    expect(document.documentElement.dataset.theme).toBe('midnight');
    expect(mocks.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ theme: 'midnight' }),
    );
  });

  it('applies every supported theme and opens only named native support destinations', async () => {
    mocks.saveSettings.mockImplementation(async (next) => next);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Appearance' }));
    const themes = [
      ['Graphite', 'graphite'],
      ['Adobe-style Dark', 'adobe-dark'],
      ['Light', 'light'],
      ['Midnight', 'midnight'],
      ['System', 'graphite'],
      ['High Contrast', 'high-contrast'],
    ];
    for (const [label, applied] of themes) {
      await user.click(screen.getByRole('radio', { name: new RegExp(label) }));
      expect(document.documentElement.dataset.theme).toBe(applied);
    }

    await user.click(screen.getByRole('button', { name: 'About & Support' }));
    await user.click(screen.getByRole('button', { name: 'GitHub' }));
    await user.click(screen.getByRole('button', { name: 'Buy Me a Coffee' }));
    await user.click(screen.getByRole('button', { name: 'Project website' }));
    expect(mocks.openSupportLink).toHaveBeenNthCalledWith(1, 'github');
    expect(mocks.openSupportLink).toHaveBeenNthCalledWith(2, 'coffee');
    expect(mocks.openSupportLink).toHaveBeenNthCalledWith(3, 'website');
  });

  it('serializes rapid preferences without letting an older save replace newer edits', async () => {
    let finishFirstSave!: (value: typeof mocks.settings) => void;
    const firstSave = new Promise<typeof mocks.settings>((resolve) => {
      finishFirstSave = resolve;
    });
    mocks.saveSettings.mockImplementation(async (next) => {
      if (mocks.saveSettings.mock.calls.length === 1) return firstSave;
      mocks.settings = next;
      return next;
    });
    const user = userEvent.setup();
    const app = render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));

    await user.click(
      screen.getByRole('switch', { name: /Close to system tray/ }),
    );
    await user.selectOptions(
      screen.getByLabelText('Default startup page'),
      'Ingest',
    );
    await user.click(
      screen.getByRole('switch', { name: /Check for beta updates/ }),
    );
    await user.click(screen.getByRole('button', { name: 'Appearance' }));
    await user.click(screen.getByRole('radio', { name: /Midnight/ }));
    await user.selectOptions(
      screen.getByLabelText('Layout density'),
      'compact',
    );

    expect(mocks.saveSettings).toHaveBeenCalledTimes(1);
    finishFirstSave({
      ...mocks.settings,
      closeToTray: false,
    });
    await waitFor(() => expect(mocks.saveSettings).toHaveBeenCalledTimes(5));
    expect(mocks.saveSettings.mock.calls.at(-1)?.[0]).toEqual(
      expect.objectContaining({
        closeToTray: false,
        startupPage: 'Ingest',
        checkForUpdates: false,
        theme: 'midnight',
        density: 'compact',
      }),
    );
    await waitFor(() =>
      expect(screen.getByText('Preference saved.')).toBeVisible(),
    );
    expect(document.documentElement.dataset.theme).toBe('midnight');
    expect(document.documentElement.dataset.density).toBe('compact');

    app.unmount();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    expect(
      screen.getByRole('switch', { name: /Close to system tray/ }),
    ).not.toBeChecked();
    expect(screen.getByLabelText('Default startup page')).toHaveValue('Ingest');
    await user.click(screen.getByRole('button', { name: 'Appearance' }));
    expect(screen.getByLabelText('Layout density')).toHaveValue('compact');
  });

  it('keeps a failed preference visible and retryable', async () => {
    mocks.saveSettings
      .mockRejectedValueOnce(new Error('settings file temporarily unavailable'))
      .mockImplementationOnce(async (next) => next);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(
      screen.getByRole('switch', { name: /Close to system tray/ }),
    );
    expect(
      await screen.findByText(
        /Settings were not saved.*temporarily unavailable/,
      ),
    ).toBeVisible();
    await user.click(
      screen.getByRole('button', { name: 'Retry saving preferences' }),
    );
    expect(
      await screen.findByText('Settings saved after retry.'),
    ).toBeVisible();
    expect(mocks.saveSettings).toHaveBeenCalledTimes(2);
  });

  it('shows manual build links without starting a check when signing is not configured', async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Updates' }));
    expect(
      await screen.findByText('Automatic updates not configured'),
    ).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Check for updates' }),
    ).toBeDisabled();
    expect(mocks.checkApprovedBetaUpdate).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'GitHub Releases' }));
    await user.click(
      screen.getByRole('button', { name: 'View available test builds' }),
    );
    expect(mocks.openSupportLink).toHaveBeenNthCalledWith(1, 'releases');
    expect(mocks.openSupportLink).toHaveBeenNthCalledWith(2, 'test-builds');
  });

  it('reports checking, available and up-to-date states for a configured signer', async () => {
    mocks.updaterConfiguration.mockResolvedValue({
      configured: true,
      channel: 'Beta',
      endpoint: 'https://example.invalid/latest.json',
    });
    mocks.settings = { ...mocks.settings, checkForUpdates: false };
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Updates' }));
    const pending = deferred<null>();
    mocks.checkApprovedBetaUpdate.mockReturnValueOnce(pending.promise);
    await user.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(
      screen.getByText('Checking for a signed Beta update…'),
    ).toBeVisible();
    await act(async () => pending.resolve(null));
    expect(
      await screen.findByText(/No newer signed Beta was available/),
    ).toBeVisible();

    mocks.checkApprovedBetaUpdate.mockResolvedValueOnce({
      currentVersion: '0.1.1-2',
      version: '0.1.1-4',
    });
    await user.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(
      await screen.findByText('Signed Beta 0.1.1-4 is available.'),
    ).toBeVisible();

    mocks.checkApprovedBetaUpdate.mockRejectedValueOnce(
      new Error('manifest unavailable'),
    );
    await user.click(screen.getByRole('button', { name: 'Check for updates' }));
    expect(
      await screen.findByText('Update check failed: manifest unavailable'),
    ).toBeVisible();
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

  it('presents paired video and WAV as one searchable logical recording', async () => {
    mocks.catalogue = {
      media: [
        {
          id: 'video',
          relativePath: '20261024/DJI_0001.MP4',
          originalFilename: 'DJI_0001.MP4',
          sha256: 'video-hash',
          captureDateProvenance: 'unknown',
          ingestDate: '2026-10-24',
          fileType: 'mp4',
          tags: [],
          projectIds: [],
          notes: '',
          technical: { sizeBytes: 2_000, hasAudio: false },
          attachedAudioId: 'audio',
          available: true,
          needsReview: false,
        },
        {
          id: 'audio',
          relativePath: '20261024/dji_0001.wav',
          originalFilename: 'dji_0001.wav',
          sha256: 'audio-hash',
          captureDateProvenance: 'unknown',
          ingestDate: '2026-10-24',
          fileType: 'wav',
          tags: ['lav'],
          projectIds: ['project'],
          notes: '',
          technical: { sizeBytes: 1_000 },
          available: true,
          needsReview: false,
        },
      ],
      projects: [
        {
          id: 'project',
          name: 'Sound Unit',
          workingDirectory: '.',
          mediaIds: ['audio'],
          notes: '',
          tags: [],
          createdAt: '2026-10-24T00:00:00Z',
        },
      ],
      updatedAt: '2026-10-24T00:00:00Z',
    };
    mocks.resolveMediaPath.mockImplementation(
      async (mediaId: string) => `C:\\Master\\${mediaId}`,
    );
    const user = userEvent.setup();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
    render(<App />);

    expect(await screen.findByText('DJI_0001.MP4')).toBeVisible();
    expect(screen.queryByRole('option', { name: /dji_0001.wav/i })).toBeNull();
    expect(screen.getByText(/2 associated files/)).toBeVisible();
    expect(screen.getByText('lav')).toBeVisible();
    expect(screen.getByText('Sound Unit')).toBeVisible();
    await user.type(
      screen.getByPlaceholderText(/Search filenames/),
      'dji_0001.wav',
    );
    await user.click(screen.getByRole('option', { name: /DJI_0001.MP4/ }));

    const associatedFiles = screen.getByRole('group', {
      name: 'Associated physical files',
    });
    expect(associatedFiles).toHaveTextContent('dji_0001.wav');
    expect(associatedFiles).toHaveTextContent(
      /DJI_0001\.MP4.*Integrity verified.*dji_0001\.wav.*Integrity verified/i,
    );
    await user.click(screen.getByRole('button', { name: 'Favourite' }));
    expect(mocks.updateMedia).toHaveBeenCalledWith({
      mediaIds: ['video', 'audio'],
      favourite: true,
    });
    await user.click(
      screen.getByRole('button', { name: /Copy paths for Premiere/ }),
    );
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(
        'C:\\Master\\video\r\nC:\\Master\\audio',
      ),
    );
    await user.click(
      screen.getByRole('button', {
        name: 'Reveal dji_0001.wav in Explorer',
      }),
    );
    expect(mocks.revealMediaInExplorer).toHaveBeenCalledWith('audio');
    await user.click(screen.getByRole('button', { name: 'Projects' }));
    expect(screen.getByText('1 assigned recordings')).toBeVisible();
  });

  it('persists the attached-audio card preference and can re-evaluate pairing', async () => {
    mocks.saveSettings.mockImplementation(async (settings) => settings);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole('button', { name: 'Settings' }));
    await user.click(screen.getByRole('button', { name: 'Library & Storage' }));
    await user.click(
      screen.getByRole('switch', {
        name: /Show attached audio as separate cards/,
      }),
    );
    expect(mocks.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ showAttachedAudioSeparately: true }),
    );
    await user.click(
      screen.getByRole('button', { name: 'Re-evaluate audio pairing' }),
    );
    expect(mocks.loadCatalogue).toHaveBeenCalled();
    expect(
      await screen.findByText(
        'External-audio pairing re-evaluated from the current catalogue.',
      ),
    ).toBeVisible();
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

    expect(
      fireEvent.mouseDown(screen.getByText('two.mp4'), { shiftKey: true }),
    ).toBe(false);
    expect(
      fireEvent.mouseDown(
        screen.getByPlaceholderText(/Search filenames, folders/),
        { shiftKey: true },
      ),
    ).toBe(true);
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

    const content = document.querySelector('main') as HTMLElement;
    const scrollTo = vi.fn();
    Object.defineProperty(content, 'scrollTop', {
      configurable: true,
      value: 600,
      writable: true,
    });
    Object.defineProperty(content, 'scrollTo', {
      configurable: true,
      value: scrollTo,
    });
    fireEvent.scroll(content);
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({ matches: true })),
    );
    await user.click(
      await screen.findByRole('button', { name: 'Scroll Library to top' }),
    );
    expect(scrollTo).toHaveBeenCalledWith({ top: 0, behavior: 'auto' });
    vi.unstubAllGlobals();
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
        expect(mocks.rescan).toHaveBeenCalledWith(false);
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
      expect(mocks.rescan).toHaveBeenCalledWith(false);
    } finally {
      vi.useRealTimers();
    }
  });
});

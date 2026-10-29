import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open, save } from '@tauri-apps/plugin-dialog';

export type DateMode = 'today' | 'original-file-dates';
export type ThumbnailHoldFrame =
  | 'first'
  | '10-percent'
  | '20-percent'
  | 'middle'
  | '80-percent'
  | 'last'
  | 'last-hovered';
export type PreviewPreloading = 'hovered' | 'visible-nearby' | 'idle';
export interface DeviceSettings {
  deviceId: string;
  masterBinPath: string;
  workingProjectsPath: string;
  catalogueInMasterBin: boolean;
  sharedCataloguePath: string;
  thumbnailCachePath: string;
  thumbnailCacheLimitBytes: number;
  hoverPreviewsEnabled: boolean;
  previewCacheLimitBytes: number;
  thumbnailHoldFrame: ThumbnailHoldFrame;
  previewPreloading: PreviewPreloading;
  previewRamCacheLimitBytes: number;
  ingestDateOrganization: DateMode;
  dateFolderPattern: string;
  initializedCataloguePath: string;
  projectPaths: Record<string, string>;
  startWithWindows: boolean;
  closeToTray: boolean;
  startupPage: 'Ingest' | 'Library' | 'Projects' | 'Archive';
  checkForUpdates: boolean;
  theme:
    | 'graphite'
    | 'adobe-dark'
    | 'light'
    | 'midnight'
    | 'system'
    | 'high-contrast';
  density: 'comfortable' | 'compact';
  showAttachedAudioSeparately: boolean;
  sidebarCollapsed?: boolean;
  libraryFolderSort?:
    | 'newest'
    | 'oldest'
    | 'name-asc'
    | 'name-desc'
    | 'most-recordings'
    | 'fewest-recordings'
    | 'custom';
  libraryFolderOrder?: string[];
  sdGrabberEnabled: boolean;
  sdGrabberAutoDetect: boolean;
  sdGrabberExtensions: string[];
  sdGrabberSearchSubfolders: boolean;
  sdGrabberRememberDevices: boolean;
  sdGrabberRecognizedDeviceIds: string[];
}
export interface AppInfo {
  version: string;
  buildCommit: string;
  edition: string;
  releaseChannel: string;
}
export interface UpdaterConfiguration {
  configured: boolean;
  channel: 'Beta';
  endpoint: string;
  reason?: string;
}
export interface MediaItem {
  id: string;
  relativePath: string;
  sourceRelativePath?: string;
  originalFilename: string;
  sha256: string;
  captureDate?: string;
  captureDateProvenance: 'embedded' | 'filesystem' | 'unknown';
  ingestDate: string;
  fileType: string;
  tags: string[];
  projectIds: string[];
  notes: string;
  favourite?: boolean;
  technical: {
    sizeBytes: number;
    durationSeconds?: number;
    width?: number;
    height?: number;
    codec?: string;
    frameRate?: number;
    hasAudio?: boolean;
  };
  thumbnailPath?: string;
  attachedAudioId?: string;
  available: boolean;
  needsReview: boolean;
  observedSize?: number;
  observedModifiedMillis?: number;
}
export interface ProjectRecord {
  id: string;
  name: string;
  workingDirectory: string;
  mediaIds: string[];
  notes: string;
  tags: string[];
  createdAt: string;
}
export interface ProjectAsset {
  relativePath: string;
  name: string;
  category:
    | 'premiere-project'
    | 'premiere-auto-save'
    | 'video'
    | 'audio-music'
    | 'images-graphics'
    | 'exports'
    | 'notes-documentation'
    | 'other';
  sizeBytes: number;
  modifiedAt?: string;
}
export interface ProjectInventory {
  projectId: string;
  rootPath: string;
  itemCount: number;
  sizeBytes: number;
  mainPremiereProject?: string;
  scannedAt: string;
  items: ProjectAsset[];
}
export interface BackupAsset {
  relativePath: string;
  sizeBytes: number;
  sha256: string;
}
export interface BackupManifest {
  schemaVersion: number;
  backupId: string;
  projectId: string;
  createdAt: string;
  sourceFileCount: number;
  sourceBytes: number;
  collectionComplete: boolean;
  assets: BackupAsset[];
}
export type BackupJobState =
  | 'prepared'
  | 'copying'
  | 'verifying'
  | 'verified'
  | 'interrupted'
  | 'failed'
  | 'needs-attention';
export interface BackupJob {
  schemaVersion: number;
  id: string;
  projectId: string;
  projectName: string;
  sourceRoot: string;
  destinationRoot: string;
  generationPath: string;
  generationRelativePath: string;
  destinationVolumeId: string;
  state: BackupJobState;
  createdAt: string;
  updatedAt: string;
  filesCompleted: number;
  bytesCompleted: number;
  currentFile?: string;
  error?: string;
  manifest: BackupManifest;
}
export interface RestoreTestResult {
  restorePath: string;
  filesVerified: number;
  bytesVerified: number;
  verifiedAt: string;
}
export interface HoverPreview {
  mediaId: string;
  durationSeconds: number;
  frameTimes: number[];
  frames: string[];
}
export interface Catalogue {
  media: MediaItem[];
  projects: ProjectRecord[];
  updatedAt: string;
  conflicts?: Array<{
    entityId: string;
    field: string;
    eventIds: string[];
    values: unknown[];
  }>;
  issues?: Array<{ code: string; source: string; detail: string }>;
}
export interface RescanResult {
  scanned: number;
  registered: number;
  changed: number;
  pending: number;
  errors: number;
  full: boolean;
  completedAt: string;
}
export interface DiscoveryStatus {
  running: boolean;
  full: boolean;
  filesProcessed: number;
  totalFiles: number;
  registered: number;
  changed: number;
  pending: number;
  errors: number;
  startedAt?: string;
  lastCompletedAt?: string;
}
export interface MediaToolsStatus {
  ffmpegAvailable: boolean;
  ffprobeAvailable: boolean;
  ffmpegVersion?: string;
  ffprobeVersion?: string;
}
export interface IngestProgress {
  jobId: string;
  phase: string;
  currentFile: string;
  filesComplete: number;
  filesTotal: number;
  bytesComplete: number;
  bytesTotal: number;
}
export interface IngestResult {
  imported: MediaItem[];
  duplicates: string[];
  conflicts: { sourcePath: string; destinationPath: string; reason: string }[];
  unsupported: string[];
  failures: string[];
  cancelled: boolean;
}
export interface InventoryFile {
  sourcePath: string;
  sourceRelativePath: string;
  plannedRelativePath: string;
  sizeBytes: number;
  kind: 'video' | 'audio' | 'image' | 'camera-support' | 'unknown';
  analysisSupported: boolean;
  preserveHierarchy: boolean;
  disposition: 'new' | 'already-imported' | 'attention';
  existingRelativePath?: string;
  detail?: string;
}
export interface InventoryExclusion {
  path: string;
  reason: string;
  sizeBytes?: number;
}
export interface SourceInventory {
  planId: string;
  files: InventoryFile[];
  excluded: InventoryExclusion[];
  issues: string[];
  totalFileCount: number;
  totalBytes: number;
  copyFileCount: number;
  copyBytes: number;
  alreadyImportedFileCount: number;
  alreadyImportedBytes: number;
  attentionFileCount: number;
  attentionBytes: number;
}
export interface RemovableVolume {
  rootPath: string;
  label: string;
  identity: string;
  capacityBytes: number;
  availableBytes: number;
}
export type IngestJobState =
  | 'queued'
  | 'copying'
  | 'verifying'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'interrupted'
  | 'paused';
export interface ProvisionalMetadata {
  tags: string[];
  projectIds: string[];
  notes: string;
  favourite: boolean;
  editedFields?: string[];
}
export type ProvisionalMetadataPatch = Partial<
  Pick<ProvisionalMetadata, 'tags' | 'projectIds' | 'notes' | 'favourite'>
>;
export interface IngestJobFile {
  id: string;
  mediaId: string;
  sourcePath: string;
  sourceRelativePath?: string;
  destinationPath: string;
  relativePath: string;
  originalFilename: string;
  sizeBytes: number;
  bytesTransferred: number;
  state: IngestJobState;
  error?: string;
  metadata: ProvisionalMetadata;
  pendingMetadata?: ProvisionalMetadata;
  metadataError?: string;
}
export interface IngestJob {
  schemaVersion: number;
  id: string;
  approvedPlanId?: string;
  state: IngestJobState;
  createdAt: string;
  updatedAt: string;
  batchTimestamp: string;
  dateOverride?: string;
  files: IngestJobFile[];
  unsupported: string[];
  failures: string[];
  inventoryExcluded?: InventoryExclusion[];
  inventoryIssues?: string[];
  inventoryTotalFileCount?: number;
  inventoryTotalBytes?: number;
  currentFile?: string;
  bytesTotal: number;
  bytesTransferred: number;
  filesCompleted: number;
}

export const nativeAvailable = () => isTauri();
export const emptySettings = (): DeviceSettings => ({
  deviceId: '',
  masterBinPath: '',
  workingProjectsPath: '',
  catalogueInMasterBin: true,
  sharedCataloguePath: '',
  thumbnailCachePath: '',
  thumbnailCacheLimitBytes: 2 * 1024 * 1024 * 1024,
  hoverPreviewsEnabled: true,
  previewCacheLimitBytes: 512 * 1024 * 1024,
  thumbnailHoldFrame: 'first',
  previewPreloading: 'visible-nearby',
  previewRamCacheLimitBytes: 0,
  ingestDateOrganization: 'today',
  dateFolderPattern: 'yyyymmdd',
  initializedCataloguePath: '',
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
  sdGrabberEnabled: true,
  sdGrabberAutoDetect: true,
  sdGrabberExtensions: ['mp4', 'wav'],
  sdGrabberSearchSubfolders: true,
  sdGrabberRememberDevices: true,
  sdGrabberRecognizedDeviceIds: [],
});
export const emptyCatalogue = (): Catalogue => ({
  media: [],
  projects: [],
  updatedAt: '',
});

export const desktopBridge = {
  loadSettings: () => invoke<DeviceSettings>('load_settings'),
  saveSettings: (settings: DeviceSettings) =>
    invoke<DeviceSettings>('save_settings', { settings }),
  loadCatalogue: () => invoke<Catalogue>('load_catalogue'),
  catalogueSignature: () => invoke<string>('catalogue_signature'),
  chooseFiles: async () => {
    const value = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: 'Camera media',
          extensions: [
            'mp4',
            'mov',
            'm4v',
            'mxf',
            'h264',
            'h265',
            'hevc',
            'mts',
            'm2ts',
            'avi',
            'r3d',
            'braw',
            'wav',
            'bwf',
            'aif',
            'aiff',
            'mp3',
            'm4a',
            'jpg',
            'jpeg',
            'png',
            'tif',
            'tiff',
            'dng',
            'arw',
            'cr2',
            'cr3',
          ],
        },
      ],
    });
    return value ? (Array.isArray(value) ? value : [value]) : [];
  },
  chooseFolder: async () => {
    const value = await open({ multiple: false, directory: true });
    return typeof value === 'string' ? value : undefined;
  },
  inventorySources: (sources: string[], dateOverride?: string) =>
    invoke<SourceInventory>('inventory_sources', { sources, dateOverride }),
  inventorySdCard: (sources: string[], dateOverride?: string) =>
    invoke<SourceInventory>('inventory_sd_card', { sources, dateOverride }),
  listRemovableVolumes: () =>
    invoke<RemovableVolume[]>('list_removable_volumes'),
  startIngest: (request: {
    jobId: string;
    sources: string[];
    dateOverride?: string;
    renamedDestinations?: Record<string, string>;
    batchTags?: string[];
    approvedPlanId: string;
    allowDuplicateCopies?: boolean;
  }) => invoke<IngestJob>('start_ingest', { request }),
  initializeIngestJobs: () => invoke<IngestJob[]>('initialize_ingest_jobs'),
  listIngestJobs: () => invoke<IngestJob[]>('list_ingest_jobs'),
  clearCompletedIngestJobs: (jobId?: string) =>
    invoke<IngestJob[]>('clear_completed_ingest_jobs', { jobId }),
  updateIngestFileMetadata: (
    jobId: string,
    fileId: string,
    patch: ProvisionalMetadataPatch,
  ) =>
    invoke<IngestJob>('update_ingest_file_metadata', {
      jobId,
      fileId,
      patch,
    }),
  pauseIngest: (jobId: string) => invoke<void>('pause_ingest', { jobId }),
  resumeIngest: (jobId: string) => invoke<void>('resume_ingest', { jobId }),
  retryIngest: (jobId: string) => invoke<void>('retry_ingest', { jobId }),
  cancelIngest: (jobId: string) => invoke<void>('cancel_ingest', { jobId }),
  refreshMediaAvailability: () => invoke<boolean>('refresh_media_availability'),
  onProgress: (
    handler: (progress: IngestProgress) => void,
  ): Promise<UnlistenFn> =>
    listen<IngestProgress>('ingest-progress', (event) =>
      handler(event.payload),
    ),
  onIngestJobsChanged: (
    handler: (jobs: IngestJob[]) => void,
  ): Promise<UnlistenFn> =>
    listen<IngestJob[]>('ingest-jobs-changed', (event) =>
      handler(event.payload),
    ),
  onBackupJobsChanged: (
    handler: (jobs: BackupJob[]) => void,
  ): Promise<UnlistenFn> =>
    listen<BackupJob[]>('backup-jobs-changed', (event) =>
      handler(event.payload),
    ),
  onDiscoveryProgress: (
    handler: (status: DiscoveryStatus) => void,
  ): Promise<UnlistenFn> =>
    listen<DiscoveryStatus>('discovery-progress', (event) =>
      handler(event.payload),
    ),
  onCatalogueChanged: (handler: () => void): Promise<UnlistenFn> =>
    listen('catalogue-changed', handler),
  onFileDrop: (handler: (paths: string[]) => void): Promise<UnlistenFn> =>
    getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === 'drop') handler(event.payload.paths);
    }),
  updateMedia: (update: {
    mediaIds: string[];
    tags?: string[];
    projectIds?: string[];
    addTags?: string[];
    addProjectIds?: string[];
    removeTags?: string[];
    removeProjectIds?: string[];
    notes?: string;
    favourite?: boolean;
  }) => invoke<Catalogue>('update_media', { update }),
  updateProject: (update: {
    projectId: string;
    notes: string;
    tags: string[];
  }) => invoke<Catalogue>('update_project', { update }),
  createProject: (name: string, workingDirectory: string) =>
    invoke<ProjectRecord>('create_project', { name, workingDirectory }),
  retryProjectSynchronization: (projectId: string) =>
    invoke<NonNullable<Catalogue['issues']>>('retry_project_synchronization', {
      projectId,
    }),
  inventoryProject: (projectId: string) =>
    invoke<ProjectInventory>('inventory_project', { projectId }),
  initializeBackupJobs: () => invoke<BackupJob[]>('initialize_backup_jobs'),
  listBackupJobs: () => invoke<BackupJob[]>('list_backup_jobs'),
  startProjectBackup: (projectId: string, destinationRoot: string) =>
    invoke<BackupJob>('start_project_backup', {
      projectId,
      destinationRoot,
    }),
  retryProjectBackup: (jobId: string, destinationRoot?: string) =>
    invoke<BackupJob>('retry_project_backup', { jobId, destinationRoot }),
  verifyProjectBackup: (jobId: string) =>
    invoke<BackupJob>('verify_project_backup', { jobId }),
  testProjectBackupRestore: (jobId: string, restoreRoot: string) =>
    invoke<RestoreTestResult>('test_project_backup_restore', {
      jobId,
      restoreRoot,
    }),
  openProjectFolder: (projectId: string) =>
    invoke<void>('open_project_folder', { projectId }),
  revealProjectAsset: (projectId: string, relativePath: string) =>
    invoke<void>('reveal_project_asset', { projectId, relativePath }),
  rescan: (full = false) => invoke<RescanResult>('rescan', { full }),
  getDiscoveryStatus: () => invoke<DiscoveryStatus>('get_discovery_status'),
  logUiError: (message: string) => invoke<void>('log_ui_error', { message }),
  appInfo: () => invoke<AppInfo>('app_info'),
  updaterConfiguration: () =>
    invoke<UpdaterConfiguration>('updater_configuration'),
  beginUpdateInstallation: () => invoke<void>('begin_update_installation'),
  endUpdateInstallation: () => invoke<void>('end_update_installation'),
  openSupportLink: (
    destination:
      'github' | 'releases' | 'test-builds' | 'website' | 'coffee' | 'degoo',
  ) => invoke<void>('open_support_link', { destination }),
  openLogsFolder: () => invoke<void>('open_logs_folder'),
  verifyMediaIntegrity: (mediaId: string) =>
    invoke<boolean>('verify_media_integrity', { mediaId }),
  enrichMedia: () => invoke<number>('enrich_media'),
  mediaToolsStatus: () => invoke<MediaToolsStatus>('media_tools_status'),
  resolveMediaPath: (mediaId: string) =>
    invoke<string>('resolve_media_path', { mediaId }),
  revealMediaInExplorer: (mediaId: string) =>
    invoke<void>('reveal_media_in_explorer', { mediaId }),
  openMediaFolder: (mediaId: string) =>
    invoke<void>('open_media_folder', { mediaId }),
  loadThumbnail: (mediaId: string) =>
    invoke<string | null>('load_thumbnail', { mediaId }),
  loadThumbnailSources: async (mediaIds: string[]) => {
    const sources = await invoke<Record<string, string>>(
      'load_thumbnail_sources',
      { mediaIds },
    );
    return Object.fromEntries(
      Object.entries(sources).map(([mediaId, path]) => [
        mediaId,
        convertFileSrc(path),
      ]),
    );
  },
  loadHoverPreview: (mediaId: string) =>
    invoke<HoverPreview>('load_hover_preview', { mediaId }),
  prepareHoverPreview: (mediaId: string) =>
    invoke<void>('prepare_hover_preview', { mediaId }),
  hoverPreviewCacheUsage: () => invoke<number>('hover_preview_cache_usage'),
  clearHoverPreviewCache: () => invoke<number>('clear_hover_preview_cache'),
  choosePremiereManifestDestination: () =>
    save({
      defaultPath: 'kairix-pimp-import.pimp-premiere.json',
      filters: [
        { name: 'Kairix PIMP Premiere manifest', extensions: ['json'] },
      ],
    }),
  exportPremiereManifest: (mediaIds: string[], destinationPath: string) =>
    invoke<number>('export_premiere_manifest', { mediaIds, destinationPath }),
};

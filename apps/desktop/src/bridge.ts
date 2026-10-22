import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';

export type DateMode = 'today' | 'original-file-dates';
export interface DeviceSettings {
  deviceId: string;
  masterBinPath: string;
  workingProjectsPath: string;
  catalogueInMasterBin: boolean;
  sharedCataloguePath: string;
  thumbnailCachePath: string;
  thumbnailCacheLimitBytes: number;
  ingestDateOrganization: DateMode;
  initializedCataloguePath: string;
  projectPaths: Record<string, string>;
}
export interface MediaItem {
  id: string;
  relativePath: string;
  originalFilename: string;
  sha256: string;
  captureDate?: string;
  captureDateProvenance: 'embedded' | 'filesystem' | 'unknown';
  ingestDate: string;
  fileType: string;
  tags: string[];
  projectIds: string[];
  notes: string;
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
  registered: number;
  changed: number;
  pending: number;
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

export const nativeAvailable = () => isTauri();
export const emptySettings = (): DeviceSettings => ({
  deviceId: '',
  masterBinPath: '',
  workingProjectsPath: '',
  catalogueInMasterBin: true,
  sharedCataloguePath: '',
  thumbnailCachePath: '',
  thumbnailCacheLimitBytes: 2 * 1024 * 1024 * 1024,
  ingestDateOrganization: 'original-file-dates',
  initializedCataloguePath: '',
  projectPaths: {},
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
  chooseFiles: async () => {
    const value = await open({
      multiple: true,
      directory: false,
      filters: [
        {
          name: 'Camera media',
          extensions: ['mp4', 'mov', 'm4v', 'h264', 'h265', 'hevc', 'wav'],
        },
      ],
    });
    return value ? (Array.isArray(value) ? value : [value]) : [];
  },
  chooseFolder: async () => {
    const value = await open({ multiple: false, directory: true });
    return typeof value === 'string' ? value : undefined;
  },
  startIngest: (request: {
    jobId: string;
    sources: string[];
    dateOverride?: string;
    renamedDestinations?: Record<string, string>;
  }) => invoke<IngestResult>('start_ingest', { request }),
  cancelIngest: (jobId: string) => invoke<void>('cancel_ingest', { jobId }),
  onProgress: (
    handler: (progress: IngestProgress) => void,
  ): Promise<UnlistenFn> =>
    listen<IngestProgress>('ingest-progress', (event) =>
      handler(event.payload),
    ),
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
    notes?: string;
  }) => invoke<Catalogue>('update_media', { update }),
  createProject: (name: string, workingDirectory: string) =>
    invoke<ProjectRecord>('create_project', { name, workingDirectory }),
  rescan: () => invoke<RescanResult>('rescan'),
  enrichMedia: () => invoke<number>('enrich_media'),
  loadThumbnail: (mediaId: string) =>
    invoke<string | null>('load_thumbnail', { mediaId }),
};

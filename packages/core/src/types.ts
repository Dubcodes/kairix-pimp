export const SCHEMA_VERSION = 2 as const;

export type DateOrganization = 'today' | 'original-file-dates';
export type MetadataOrigin = 'manual' | 'technical' | 'ai-suggestion';

export interface DeviceSettings {
  deviceId: string;
  masterBinPath: string;
  workingProjectsPath: string;
  catalogueInMasterBin?: boolean;
  sharedCataloguePath?: string;
  thumbnailCachePath?: string;
  thumbnailCacheLimitBytes?: number;
  initializedCataloguePath?: string;
  projectPaths?: Record<string, string>;
  dateFolderPattern?: string;
  showAttachedAudioSeparately?: boolean;
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
}

export interface SharedSettings {
  schemaVersion: number;
  ingestDateOrganization: DateOrganization;
  updatedAt: string;
  updatedBy: string;
}

export interface TechnicalMetadata {
  durationSeconds?: number;
  width?: number;
  height?: number;
  codec?: string;
  frameRate?: number;
  sizeBytes: number;
  hasAudio?: boolean;
}

export interface MediaItem {
  id: string;
  relativePath: string;
  sourceRelativePath?: string;
  originalFilename: string;
  sha256: string;
  captureDate?: string;
  captureDateProvenance: 'embedded' | 'filesystem' | 'unknown';
  captureTimezone?: string;
  ingestDate: string;
  fileType?: string;
  tags: string[];
  projectIds: string[];
  technical: TechnicalMetadata;
  notes?: string;
  favourite?: boolean;
  thumbnailPath?: string;
  attachedAudioId?: string;
  available?: boolean;
  needsReview?: boolean;
  observedSize?: number;
  observedModifiedMillis?: number;
  missing?: boolean;
  integrityByDevice?: Record<string, IntegrityEvidence>;
}

export interface IntegrityEvidence {
  observedSize: number;
  observedModifiedMillis?: number;
  observedSha256?: string;
  needsReview: boolean;
}

export interface FolderRecord {
  id: string;
  relativePath: string;
  displayName: string;
  tags: string[];
  notes?: string;
}

export interface ProjectRecord {
  id: string;
  name: string;
  workingDirectory: string;
  mediaIds: string[];
  createdAt: string;
  notes?: string;
  tags?: string[];
}

export type CatalogueEvent =
  | { type: 'media.registered'; media: MediaItem }
  | { type: 'media.tags.added'; mediaId: string; values: string[] }
  | { type: 'media.tags.removed'; mediaId: string; values: string[] }
  | { type: 'media.tags.replaced'; mediaId: string; values: string[] }
  | { type: 'media.projects.added'; mediaId: string; values: string[] }
  | { type: 'media.projects.removed'; mediaId: string; values: string[] }
  | { type: 'media.projects.replaced'; mediaId: string; values: string[] }
  | { type: 'media.legacy-snapshot-replaced'; media: MediaItem }
  | { type: 'media.notes.set'; mediaId: string; value: string }
  | { type: 'media.favourite.set'; mediaId: string; value: boolean }
  | {
      type: 'media.enriched';
      mediaId: string;
      technical: TechnicalMetadata;
      thumbnailRef?: string;
    }
  | {
      type: 'media.evidence-observed';
      mediaId: string;
      observedSize: number;
      observedModifiedMillis?: number;
      observedSha256?: string;
      needsReview: boolean;
    }
  | { type: 'folder.upserted'; folder: FolderRecord }
  | { type: 'folder.moved'; folderId: string; relativePath: string }
  | { type: 'project.upserted'; project: ProjectRecord };

export interface EventEnvelope {
  schemaVersion: typeof SCHEMA_VERSION;
  eventId: string;
  deviceId: string;
  sequence: number;
  occurredAt: string;
  observed: Record<string, number>;
  event: CatalogueEvent;
}

export interface MetadataConflict {
  entityId: string;
  field: string;
  eventIds: string[];
  values: unknown[];
}

export interface CatalogueIssue {
  code: string;
  source: string;
  detail: string;
}

export interface CatalogueState {
  media: Record<string, MediaItem>;
  folders: Record<string, FolderRecord>;
  projects: Record<string, ProjectRecord>;
  conflicts: MetadataConflict[];
  issues: CatalogueIssue[];
  appliedEventIds: string[];
}

export interface AiResult<T> {
  mediaId: string;
  kind: 'transcript' | 'scene' | 'object' | 'activity' | 'sound' | 'tags';
  modelId: string;
  modelVersion: string;
  generatedAt: string;
  confidence?: number;
  startMs?: number;
  endMs?: number;
  value: T;
}

export interface AiProcessor {
  readonly id: string;
  isAvailable(): Promise<boolean>;
  process(
    media: MediaItem,
    signal: AbortSignal,
  ): AsyncIterable<AiResult<unknown>>;
}

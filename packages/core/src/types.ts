export const SCHEMA_VERSION = 1 as const;

export type DateOrganization = 'today' | 'original-file-dates';
export type MetadataOrigin = 'manual' | 'technical' | 'ai-suggestion';

export interface DeviceSettings {
  deviceId: string;
  masterBinPath: string;
  workingProjectsPath: string;
}

export interface SharedSettings {
  schemaVersion: typeof SCHEMA_VERSION;
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
}

export interface MediaItem {
  id: string;
  relativePath: string;
  originalFilename: string;
  sha256: string;
  captureDate?: string;
  captureDateProvenance: 'embedded' | 'filesystem' | 'unknown';
  captureTimezone?: string;
  ingestDate: string;
  tags: string[];
  projectIds: string[];
  technical: TechnicalMetadata;
  missing?: boolean;
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
}

export type CatalogueEvent =
  | { type: 'media.registered'; media: MediaItem }
  | { type: 'media.tags-set'; mediaId: string; tags: string[] }
  | { type: 'media.projects-set'; mediaId: string; projectIds: string[] }
  | { type: 'folder.upserted'; folder: FolderRecord }
  | { type: 'folder.moved'; folderId: string; relativePath: string }
  | { type: 'project.upserted'; project: ProjectRecord };

export interface EventEnvelope {
  schemaVersion: typeof SCHEMA_VERSION;
  eventId: string;
  deviceId: string;
  sequence: number;
  occurredAt: string;
  event: CatalogueEvent;
}

export interface CatalogueState {
  media: Record<string, MediaItem>;
  folders: Record<string, FolderRecord>;
  projects: Record<string, ProjectRecord>;
  conflicts: EventEnvelope[];
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

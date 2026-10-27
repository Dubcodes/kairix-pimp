import type { Catalogue, MediaItem } from './bridge';

export const associatedMedia = (
  item: MediaItem,
  media: MediaItem[],
): MediaItem[] => {
  const attached = item.attachedAudioId
    ? media.find((candidate) => candidate.id === item.attachedAudioId)
    : undefined;
  return attached ? [item, attached] : [item];
};

export const visibleMedia = (
  media: MediaItem[],
  showAttachedAudioSeparately: boolean,
): MediaItem[] => {
  if (showAttachedAudioSeparately) return media;
  const attachedIds = new Set(
    media.flatMap((item) =>
      item.attachedAudioId ? [item.attachedAudioId] : [],
    ),
  );
  return media.filter((item) => !attachedIds.has(item.id));
};

export const expandLogicalMediaIds = (
  selectedIds: string[],
  media: MediaItem[],
): string[] => {
  const byId = new Map(media.map((item) => [item.id, item]));
  return [
    ...new Set(
      selectedIds.flatMap((id) => {
        const item = byId.get(id);
        return item?.attachedAudioId ? [id, item.attachedAudioId] : [id];
      }),
    ),
  ];
};

export const logicalTags = (item: MediaItem, media: MediaItem[]): string[] => [
  ...new Set(associatedMedia(item, media).flatMap((record) => record.tags)),
];

export const logicalProjectIds = (
  item: MediaItem,
  media: MediaItem[],
): string[] => [
  ...new Set(
    associatedMedia(item, media).flatMap((record) => record.projectIds),
  ),
];

export const logicalProjectRecordingCount = (
  projectId: string,
  media: MediaItem[],
): number =>
  visibleMedia(media, false).filter((item) =>
    associatedMedia(item, media).some((record) =>
      record.projectIds.includes(projectId),
    ),
  ).length;

const searchValues = (item: MediaItem, catalogue: Catalogue) => [
  item.originalFilename,
  item.relativePath,
  item.notes,
  item.captureDate,
  item.ingestDate,
  item.fileType,
  item.technical.codec,
  item.technical.width,
  item.technical.height,
  item.technical.frameRate,
  ...item.tags,
  ...item.projectIds.map(
    (id) => catalogue.projects.find((project) => project.id === id)?.name ?? '',
  ),
];

export const logicalMediaMatches = (
  item: MediaItem,
  query: string,
  catalogue: Catalogue,
): boolean => {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  return associatedMedia(item, catalogue.media)
    .flatMap((record) => searchValues(record, catalogue))
    .join(' ')
    .toLowerCase()
    .includes(normalized);
};

import type { CatalogueState, EventEnvelope } from './types';

export const emptyCatalogue = (): CatalogueState => ({
  media: {},
  folders: {},
  projects: {},
  conflicts: [],
  appliedEventIds: [],
});

const normalized = (values: string[]) =>
  [...new Set(values.map((value) => value.trim()).filter(Boolean))].sort(
    (a, b) => a.localeCompare(b),
  );

export function rebuildCatalogue(events: EventEnvelope[]): CatalogueState {
  const state = emptyCatalogue();
  const seen = new Set<string>();
  const fieldVersions = new Map<string, EventEnvelope>();
  const ordered = [...events].sort(
    (a, b) =>
      a.occurredAt.localeCompare(b.occurredAt) ||
      a.deviceId.localeCompare(b.deviceId) ||
      a.sequence - b.sequence,
  );

  for (const envelope of ordered) {
    if (seen.has(envelope.eventId)) continue;
    seen.add(envelope.eventId);
    state.appliedEventIds.push(envelope.eventId);
    const event = envelope.event;
    if (event.type === 'media.registered') {
      state.media[event.media.id] ??= {
        ...event.media,
        tags: normalized(event.media.tags),
        projectIds: normalized(event.media.projectIds),
      };
    } else if (
      event.type === 'media.tags-set' ||
      event.type === 'media.projects-set'
    ) {
      const item = state.media[event.mediaId];
      if (!item) {
        state.conflicts.push(envelope);
        continue;
      }
      const field = event.type === 'media.tags-set' ? 'tags' : 'projectIds';
      const key = `${event.mediaId}:${field}`;
      const previous = fieldVersions.get(key);
      if (
        previous &&
        previous.deviceId !== envelope.deviceId &&
        previous.occurredAt === envelope.occurredAt
      ) {
        state.conflicts.push(envelope);
        continue;
      }
      item[field] = normalized(
        event.type === 'media.tags-set' ? event.tags : event.projectIds,
      );
      fieldVersions.set(key, envelope);
    } else if (event.type === 'folder.upserted') {
      state.folders[event.folder.id] = {
        ...event.folder,
        tags: normalized(event.folder.tags),
      };
    } else if (event.type === 'folder.moved') {
      const folder = state.folders[event.folderId];
      if (!folder) state.conflicts.push(envelope);
      else folder.relativePath = event.relativePath;
    } else if (event.type === 'project.upserted') {
      state.projects[event.project.id] = {
        ...event.project,
        mediaIds: normalized(event.project.mediaIds),
      };
    }
  }
  return state;
}

export function searchCatalogue(
  state: CatalogueState,
  query: string,
): string[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return Object.keys(state.media);
  return Object.values(state.media)
    .filter((media) => {
      const folder = Object.values(state.folders).find((candidate) =>
        media.relativePath.startsWith(`${candidate.relativePath}/`),
      );
      const projects = media.projectIds.map(
        (id) => state.projects[id]?.name ?? '',
      );
      return [
        media.originalFilename,
        media.relativePath,
        ...media.tags,
        ...(folder?.tags ?? []),
        ...projects,
      ]
        .join('\n')
        .toLocaleLowerCase()
        .includes(needle);
    })
    .map((media) => media.id);
}

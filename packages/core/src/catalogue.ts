import type { CatalogueState, EventEnvelope, MetadataConflict } from './types';

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
  const fieldVersions = new Map<
    string,
    { envelope: EventEnvelope; value: unknown }
  >();
  const setAdds = new Map<
    string,
    Map<string, { value: string; envelope: EventEnvelope }>
  >();
  const ordered = [...events].sort((a, b) => {
    if (a.deviceId === b.deviceId) return a.sequence - b.sequence;
    if ((a.observed[b.deviceId] ?? 0) >= b.sequence) return 1;
    if ((b.observed[a.deviceId] ?? 0) >= a.sequence) return -1;
    return (
      a.deviceId.localeCompare(b.deviceId) ||
      a.sequence - b.sequence ||
      a.eventId.localeCompare(b.eventId)
    );
  });
  ordered.sort(
    (a, b) =>
      Number(b.event.type === 'media.registered') -
      Number(a.event.type === 'media.registered'),
  );

  const happensBefore = (a: EventEnvelope, b: EventEnvelope) =>
    (a.deviceId === b.deviceId && a.sequence < b.sequence) ||
    (b.observed[a.deviceId] ?? 0) >= a.sequence;
  const updateSet = (mediaId: string, field: 'tags' | 'projectIds') => {
    const item = state.media[mediaId];
    if (!item) return;
    item[field] = normalized(
      [...(setAdds.get(`${mediaId}:${field}`)?.values() ?? [])].map(
        (entry) => entry.value,
      ),
    );
  };
  const addConflict = (conflict: MetadataConflict) => {
    const key = `${conflict.entityId}:${conflict.field}:${conflict.eventIds
      .slice()
      .sort()
      .join(':')}`;
    if (
      !state.conflicts.some(
        (candidate) =>
          `${candidate.entityId}:${candidate.field}:${candidate.eventIds
            .slice()
            .sort()
            .join(':')}` === key,
      )
    )
      state.conflicts.push(conflict);
  };

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
      for (const [field, values] of [
        ['tags', event.media.tags],
        ['projectIds', event.media.projectIds],
      ] as const) {
        const adds = setAdds.get(`${event.media.id}:${field}`) ?? new Map();
        for (const value of values)
          adds.set(`${envelope.eventId}:${value}`, { value, envelope });
        setAdds.set(`${event.media.id}:${field}`, adds);
        updateSet(event.media.id, field);
      }
    } else if (
      event.type === 'media.tags.added' ||
      event.type === 'media.projects.added' ||
      event.type === 'media.tags.removed' ||
      event.type === 'media.projects.removed'
    ) {
      const item = state.media[event.mediaId];
      if (!item) {
        addConflict({
          entityId: event.mediaId,
          field: 'missing-media',
          eventIds: [envelope.eventId],
          values: [event],
        });
        continue;
      }
      const field = event.type.startsWith('media.tags') ? 'tags' : 'projectIds';
      const key = `${event.mediaId}:${field}`;
      const adds = setAdds.get(key) ?? new Map();
      if (event.type.endsWith('.added')) {
        for (const value of normalized(event.values))
          adds.set(`${envelope.eventId}:${value}`, { value, envelope });
      } else {
        const removed = new Set(normalized(event.values));
        for (const [token, addition] of adds)
          if (
            removed.has(addition.value) &&
            happensBefore(addition.envelope, envelope)
          )
            adds.delete(token);
      }
      setAdds.set(key, adds);
      updateSet(event.mediaId, field);
    } else if (event.type === 'media.notes.set') {
      const item = state.media[event.mediaId];
      if (!item) continue;
      const key = `${event.mediaId}:notes`;
      const previous = fieldVersions.get(key);
      if (previous && !happensBefore(previous.envelope, envelope)) {
        addConflict({
          entityId: event.mediaId,
          field: 'notes',
          eventIds: [previous.envelope.eventId, envelope.eventId],
          values: [previous.value, event.value],
        });
        const currentKey = `${previous.envelope.deviceId}:${previous.envelope.sequence}:${previous.envelope.eventId}`;
        const nextKey = `${envelope.deviceId}:${envelope.sequence}:${envelope.eventId}`;
        if (nextKey > currentKey) item.notes = event.value;
      } else {
        item.notes = event.value;
      }
      fieldVersions.set(key, { envelope, value: event.value });
    } else if (event.type === 'media.enriched') {
      const item = state.media[event.mediaId];
      if (!item) continue;
      item.technical = event.technical;
      item.thumbnailPath = event.thumbnailRef;
    } else if (event.type === 'media.evidence-observed') {
      const item = state.media[event.mediaId];
      if (!item) continue;
      item.observedSize = event.observedSize;
      item.observedModifiedMillis = event.observedModifiedMillis;
      item.needsReview = event.needsReview;
    } else if (event.type === 'folder.upserted') {
      const key = `${event.folder.id}:folder`;
      const previous = fieldVersions.get(key);
      if (
        previous &&
        !happensBefore(previous.envelope, envelope) &&
        JSON.stringify(previous.value) !== JSON.stringify(event.folder)
      )
        addConflict({
          entityId: event.folder.id,
          field: 'folder',
          eventIds: [previous.envelope.eventId, envelope.eventId],
          values: [previous.value, event.folder],
        });
      state.folders[event.folder.id] = {
        ...event.folder,
        tags: normalized(event.folder.tags),
      };
      fieldVersions.set(key, { envelope, value: event.folder });
    } else if (event.type === 'folder.moved') {
      const folder = state.folders[event.folderId];
      if (!folder)
        addConflict({
          entityId: event.folderId,
          field: 'missing-folder',
          eventIds: [envelope.eventId],
          values: [event.relativePath],
        });
      else {
        const key = `${event.folderId}:folder-path`;
        const previous = fieldVersions.get(key);
        if (previous && !happensBefore(previous.envelope, envelope))
          addConflict({
            entityId: event.folderId,
            field: 'relativePath',
            eventIds: [previous.envelope.eventId, envelope.eventId],
            values: [previous.value, event.relativePath],
          });
        folder.relativePath = event.relativePath;
        fieldVersions.set(key, { envelope, value: event.relativePath });
      }
    } else if (event.type === 'project.upserted') {
      const key = `${event.project.id}:project`;
      const previous = fieldVersions.get(key);
      if (
        previous &&
        !happensBefore(previous.envelope, envelope) &&
        JSON.stringify(previous.value) !== JSON.stringify(event.project)
      )
        addConflict({
          entityId: event.project.id,
          field: 'project',
          eventIds: [previous.envelope.eventId, envelope.eventId],
          values: [previous.value, event.project],
        });
      state.projects[event.project.id] = {
        ...event.project,
        mediaIds: normalized(event.project.mediaIds),
      };
      fieldVersions.set(key, { envelope, value: event.project });
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

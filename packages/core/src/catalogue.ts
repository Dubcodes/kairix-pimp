import type {
  CatalogueIssue,
  CatalogueState,
  EventEnvelope,
  MetadataConflict,
} from './types';

export const emptyCatalogue = (): CatalogueState => ({
  media: {},
  folders: {},
  projects: {},
  conflicts: [],
  issues: [],
  appliedEventIds: [],
});

const compareCodePoints = (a: string, b: string) => {
  const left = Array.from(a, (value) => value.codePointAt(0)!);
  const right = Array.from(b, (value) => value.codePointAt(0)!);
  for (let index = 0; index < Math.min(left.length, right.length); index += 1)
    if (left[index] !== right[index]) return left[index] - right[index];
  return left.length - right.length;
};
const normalizedKey = (value: string) => value.trim().toLowerCase();
const normalized = (values: string[]) => {
  const sorted = values
    .map((value) => value.trim())
    .filter(Boolean)
    .sort(
      (a, b) =>
        compareCodePoints(normalizedKey(a), normalizedKey(b)) ||
        compareCodePoints(a, b),
    );
  return sorted.filter(
    (value, index) =>
      index === 0 || normalizedKey(value) !== normalizedKey(sorted[index - 1]),
  );
};

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
  const ordered = causalOrder(events, state.issues);

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
      const existing = state.media[event.media.id];
      if (existing && JSON.stringify(existing) !== JSON.stringify(event.media))
        addConflict({
          entityId: event.media.id,
          field: 'registration',
          eventIds: [envelope.eventId],
          values: [existing, event.media],
        });
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
    } else if (event.type === 'media.legacy-snapshot-replaced') {
      const existing = state.media[event.media.id];
      if (existing && JSON.stringify(existing) !== JSON.stringify(event.media))
        addConflict({
          entityId: event.media.id,
          field: 'legacy-snapshot',
          eventIds: [envelope.eventId],
          values: [existing, event.media],
        });
      state.media[event.media.id] = {
        ...event.media,
        tags: normalized(event.media.tags),
        projectIds: normalized(event.media.projectIds),
      };
      for (const [field, values] of [
        ['tags', event.media.tags],
        ['projectIds', event.media.projectIds],
      ] as const) {
        const adds = new Map<
          string,
          { value: string; envelope: EventEnvelope }
        >();
        for (const value of normalized(values))
          adds.set(`${envelope.eventId}:${value}`, { value, envelope });
        setAdds.set(`${event.media.id}:${field}`, adds);
        updateSet(event.media.id, field);
      }
    } else if (
      event.type === 'media.tags.added' ||
      event.type === 'media.projects.added' ||
      event.type === 'media.tags.removed' ||
      event.type === 'media.projects.removed' ||
      event.type === 'media.tags.replaced' ||
      event.type === 'media.projects.replaced'
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
      } else if (event.type.endsWith('.removed')) {
        const removed = new Set(normalized(event.values).map(normalizedKey));
        for (const [token, addition] of adds)
          if (
            removed.has(normalizedKey(addition.value)) &&
            happensBefore(addition.envelope, envelope)
          )
            adds.delete(token);
      } else {
        for (const [token, addition] of adds)
          if (happensBefore(addition.envelope, envelope)) adds.delete(token);
        for (const value of normalized(event.values))
          adds.set(`${envelope.eventId}:${value}`, { value, envelope });
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
    } else if (event.type === 'media.favourite.set') {
      const item = state.media[event.mediaId];
      if (!item) continue;
      const key = `${event.mediaId}:favourite`;
      const previous = fieldVersions.get(key);
      if (previous && !happensBefore(previous.envelope, envelope)) {
        addConflict({
          entityId: event.mediaId,
          field: 'favourite',
          eventIds: [previous.envelope.eventId, envelope.eventId],
          values: [previous.value, event.value],
        });
        const currentKey = `${previous.envelope.deviceId}:${previous.envelope.sequence}:${previous.envelope.eventId}`;
        const nextKey = `${envelope.deviceId}:${envelope.sequence}:${envelope.eventId}`;
        if (nextKey > currentKey) item.favourite = event.value;
      } else {
        item.favourite = event.value;
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
      item.integrityByDevice ??= {};
      item.integrityByDevice[envelope.deviceId] = {
        observedSize: event.observedSize,
        observedModifiedMillis: event.observedModifiedMillis,
        observedSha256: event.observedSha256,
        needsReview: event.needsReview,
      };
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

const concurrentOrder = (a: EventEnvelope, b: EventEnvelope) =>
  eventPriority(a) - eventPriority(b) ||
  a.deviceId.localeCompare(b.deviceId) ||
  a.sequence - b.sequence ||
  a.eventId.localeCompare(b.eventId);

const eventPriority = (event: EventEnvelope) => {
  if (event.event.type === 'media.registered')
    return event.eventId.startsWith('legacy-') ? 0 : 1;
  return event.event.type === 'media.legacy-snapshot-replaced' ? 3 : 2;
};

function causalOrder(
  input: EventEnvelope[],
  issues: CatalogueIssue[],
): EventEnvelope[] {
  const unique = new Map<string, EventEnvelope>();
  for (const event of input) {
    const previous = unique.get(event.eventId);
    if (!previous) {
      unique.set(event.eventId, event);
      continue;
    }
    if (JSON.stringify(previous) !== JSON.stringify(event)) {
      issues.push({
        code: 'duplicate-event-conflict',
        source: event.eventId,
        detail:
          'Duplicate event ID has different content; deterministic copy retained',
      });
      if (JSON.stringify(event) < JSON.stringify(previous))
        unique.set(event.eventId, event);
    }
  }
  const nodes = [...unique.values()].sort(concurrentOrder);
  const byDevice = new Map<string, number[]>();
  nodes.forEach((event, index) => {
    const indexes = byDevice.get(event.deviceId) ?? [];
    indexes.push(index);
    byDevice.set(event.deviceId, indexes);
  });
  for (const indexes of byDevice.values())
    indexes.sort((a, b) => nodes[a].sequence - nodes[b].sequence);

  const outgoing = nodes.map(() => new Set<number>());
  const indegree = nodes.map(() => 0);
  const addEdge = (from: number, to: number) => {
    if (from === to || outgoing[from].has(to)) return;
    outgoing[from].add(to);
    indegree[to] += 1;
  };
  for (const [deviceId, indexes] of byDevice) {
    for (let position = 1; position < indexes.length; position += 1) {
      const previous = nodes[indexes[position - 1]];
      const current = nodes[indexes[position]];
      addEdge(indexes[position - 1], indexes[position]);
      if (current.sequence !== previous.sequence + 1)
        issues.push({
          code: 'missing-causal-dependency',
          source: current.eventId,
          detail: `${deviceId} sequence ${current.sequence} follows ${previous.sequence}`,
        });
    }
  }
  nodes.forEach((event, index) => {
    for (const [deviceId, observedSequence] of Object.entries(event.observed)) {
      if (deviceId === event.deviceId && observedSequence >= event.sequence) {
        issues.push({
          code: 'invalid-causal-metadata',
          source: event.eventId,
          detail: 'An event cannot observe its own sequence or a future one',
        });
        continue;
      }
      if (observedSequence === 0) continue;
      const candidates = byDevice.get(deviceId) ?? [];
      const dependency = candidates
        .filter((candidate) => nodes[candidate].sequence <= observedSequence)
        .at(-1);
      if (
        dependency === undefined ||
        nodes[dependency].sequence < observedSequence
      )
        issues.push({
          code: 'missing-causal-dependency',
          source: event.eventId,
          detail: `Observed ${deviceId} sequence ${observedSequence}, which is not available`,
        });
      if (dependency !== undefined) addEdge(dependency, index);
    }
  });

  const ready = indegree
    .map((degree, index) => ({ degree, index }))
    .filter(({ degree }) => degree === 0)
    .map(({ index }) => index);
  const result: EventEnvelope[] = [];
  const emitted = new Set<number>();
  while (ready.length) {
    ready.sort((a, b) => concurrentOrder(nodes[a], nodes[b]));
    const index = ready.shift()!;
    if (emitted.has(index)) continue;
    emitted.add(index);
    result.push(nodes[index]);
    for (const dependent of outgoing[index]) {
      indegree[dependent] -= 1;
      if (indegree[dependent] === 0) ready.push(dependent);
    }
  }
  if (result.length !== nodes.length) {
    issues.push({
      code: 'causal-cycle',
      source: 'event-streams',
      detail:
        'Cyclic causal metadata was detected; affected events use deterministic fallback order',
    });
    result.push(
      ...nodes.filter((_, index) => !emitted.has(index)).sort(concurrentOrder),
    );
  }
  return result;
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

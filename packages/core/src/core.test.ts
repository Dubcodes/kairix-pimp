import { describe, expect, it } from 'vitest';
import {
  dateFolder,
  inferFolderMetadata,
  rebuildCatalogue,
  searchCatalogue,
  selectCaptureDate,
  type EventEnvelope,
  type MediaItem,
} from './index';

const media: MediaItem = {
  id: 'm1',
  relativePath: 'media/2026-10-18/clip.mp4',
  originalFilename: 'clip.mp4',
  sha256: 'abc',
  captureDateProvenance: 'unknown',
  ingestDate: '2026-10-18T00:00:00Z',
  tags: [],
  projectIds: [],
  technical: { sizeBytes: 2 },
};
const envelope = (
  event: EventEnvelope['event'],
  id = 'e1',
  deviceId = 'a',
  occurredAt = '2026-10-18T00:00:00Z',
): EventEnvelope => ({
  schemaVersion: 2,
  eventId: id,
  deviceId,
  sequence: 1,
  occurredAt,
  observed: {},
  event,
});

describe('dates', () => {
  it('uses exactly the selected organization rule', () => {
    const now = new Date(2026, 9, 22);
    const capture = new Date(2026, 9, 18);
    expect(dateFolder('today', now, capture)).toBe('2026-10-22');
    expect(dateFolder('original-file-dates', now, capture)).toBe('2026-10-18');
  });
  it('rejects implausible camera dates and records provenance', () => {
    const selected = selectCaptureDate({
      embedded: new Date('1970-01-01'),
      filesystem: new Date('2026-10-18'),
    });
    expect(selected.provenance).toBe('filesystem');
  });
  it('suggests rather than asserts folder-derived tags', () =>
    expect(inferFolderMetadata('2026-10-18 Go Kart Repairs')).toEqual({
      date: '2026-10-18',
      suggestedTags: ['Go', 'Kart', 'Repairs'],
    }));
});

describe('catalogue reducer', () => {
  it('is idempotent for duplicate events', () => {
    const event = envelope({ type: 'media.registered', media });
    expect(Object.keys(rebuildCatalogue([event, event]).media)).toEqual(['m1']);
  });
  it('selects divergent duplicate event IDs deterministically and reports them', () => {
    const first = envelope(
      { type: 'media.registered', media: { ...media, notes: 'A' } },
      'duplicate-id',
      'device-a',
    );
    const second = envelope(
      { type: 'media.registered', media: { ...media, notes: 'B' } },
      'duplicate-id',
      'device-b',
    );
    const forward = rebuildCatalogue([first, second]);
    const reverse = rebuildCatalogue([second, first]);
    expect(forward.media).toEqual(reverse.media);
    expect(forward.issues.map((issue) => issue.code)).toContain(
      'duplicate-event-conflict',
    );
    expect(reverse.issues.map((issue) => issue.code)).toContain(
      'duplicate-event-conflict',
    );
  });
  it('converges independent same-path registrations and aliases later metadata', () => {
    const first: MediaItem = {
      ...media,
      id: 'media-a',
      relativePath: '20261023/DJI_0001.mp4',
      sha256: 'same-content',
      tags: ['Camera A'],
      projectIds: ['project-a'],
      notes: 'note A',
    };
    const second: MediaItem = {
      ...first,
      id: 'media-b',
      tags: ['Camera B'],
      projectIds: ['project-b'],
      notes: 'note B',
      favourite: true,
    };
    const events: EventEnvelope[] = [
      envelope({ type: 'media.registered', media: first }, 'a-1', 'device-a'),
      envelope({ type: 'media.registered', media: second }, 'b-1', 'device-b'),
      {
        ...envelope(
          {
            type: 'media.notes.set',
            mediaId: 'media-a',
            value: 'offline A',
          },
          'a-2',
          'device-a',
        ),
        sequence: 2,
        observed: { 'device-a': 1 },
      },
      {
        ...envelope(
          {
            type: 'media.notes.set',
            mediaId: 'media-b',
            value: 'offline B',
          },
          'b-2',
          'device-b',
        ),
        sequence: 2,
        observed: { 'device-b': 1 },
      },
      envelope(
        {
          type: 'media.tags.added',
          mediaId: 'media-b',
          values: ['Synced later'],
        },
        'c-1',
        'device-c',
      ),
      envelope(
        {
          type: 'project.upserted',
          project: {
            id: 'project-a',
            name: 'A',
            workingDirectory: '.',
            mediaIds: [],
            createdAt: '2026-10-23T00:00:00Z',
          },
        },
        'p-a',
        'project-device-a',
      ),
      envelope(
        {
          type: 'project.upserted',
          project: {
            id: 'project-b',
            name: 'B',
            workingDirectory: '.',
            mediaIds: [],
            createdAt: '2026-10-23T00:00:00Z',
          },
        },
        'p-b',
        'project-device-b',
      ),
    ];
    const states = [
      rebuildCatalogue(events),
      rebuildCatalogue(events.slice().reverse()),
      rebuildCatalogue([
        events[4],
        events[1],
        events[6],
        events[0],
        events[3],
        events[5],
        events[2],
        events[0],
      ]),
    ];
    expect(states[1]).toEqual(states[0]);
    expect(states[2]).toEqual(states[0]);
    expect(Object.keys(states[0].media)).toEqual(['media-a']);
    expect(states[0].media['media-a'].tags).toEqual([
      'Camera A',
      'Camera B',
      'Synced later',
    ]);
    expect(states[0].media['media-a'].projectIds).toEqual([
      'project-a',
      'project-b',
    ]);
    expect(states[0].media['media-a'].favourite).toBe(true);
    expect(states[0].conflicts).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ entityId: 'media-a', field: 'notes' }),
      ]),
    );
    expect(states[0].projects['project-a'].mediaIds).toEqual(['media-a']);
    expect(states[0].projects['project-b'].mediaIds).toEqual(['media-a']);
  });
  it('does not merge by hash alone or merge matching paths with different hashes', () => {
    const legacy = {
      ...envelope(
        {
          type: 'media.registered' as const,
          media: {
            ...media,
            id: 'legacy-id',
            relativePath: '20261023/clip.mp4',
            sha256: 'hash-one',
          },
        },
        'legacy-media-legacy-id',
        'migration',
      ),
    };
    const state = rebuildCatalogue([
      envelope(
        {
          type: 'media.registered',
          media: {
            ...media,
            id: 'new-id',
            relativePath: '20261023/clip.mp4',
            sha256: 'hash-one',
          },
        },
        'new',
        'device-b',
      ),
      envelope(
        {
          type: 'media.registered',
          media: {
            ...media,
            id: 'other-path',
            relativePath: '20261024/clip.mp4',
            sha256: 'hash-one',
          },
        },
        'other-path',
        'device-c',
      ),
      envelope(
        {
          type: 'media.registered',
          media: {
            ...media,
            id: 'other-hash',
            relativePath: '20261023/clip.mp4',
            sha256: 'hash-two',
          },
        },
        'other-hash',
        'device-d',
      ),
      legacy,
    ]);
    expect(Object.keys(state.media).sort()).toEqual([
      'legacy-id',
      'other-hash',
      'other-path',
    ]);
    expect(state.conflicts).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ field: 'identity-hash' }),
      ]),
    );
  });
  it('persists tagging and multiple project assignment', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        {
          type: 'media.tags.added',
          mediaId: 'm1',
          values: ['Workshop', 'Workshop'],
        },
        'e2',
      ),
      envelope(
        { type: 'media.projects.added', mediaId: 'm1', values: ['p2', 'p1'] },
        'e3',
      ),
    ]);
    expect(state.media.m1.tags).toEqual(['Workshop']);
    expect(state.media.m1.projectIds).toEqual(['p1', 'p2']);
  });
  it('normalizes case, whitespace and Unicode identically in every delivery order', () => {
    const registered = envelope({ type: 'media.registered', media }, 'n1', 'n');
    const variants = {
      ...envelope(
        {
          type: 'media.tags.added' as const,
          mediaId: 'm1',
          values: [' Race ', 'race', 'Ångström'],
        },
        'n2',
        'n',
      ),
      sequence: 2,
      observed: { n: 1 },
    };
    const moreVariants = {
      ...envelope(
        {
          type: 'media.tags.added' as const,
          mediaId: 'm1',
          values: ['RACE', ' ångström ', 'Race'],
        },
        'n3',
        'n',
      ),
      sequence: 3,
      observed: { n: 2 },
    };
    const forward = rebuildCatalogue([
      registered,
      variants,
      moreVariants,
      moreVariants,
    ]);
    const reverse = rebuildCatalogue([
      moreVariants,
      variants,
      registered,
      moreVariants,
    ]);
    expect(forward.media.m1.tags).toEqual(['RACE', 'Ångström']);
    expect(reverse.media.m1.tags).toEqual(forward.media.m1.tags);
  });
  it('matches case-insensitive removals without dropping unrelated Unicode tags', () => {
    const removed = {
      ...envelope(
        {
          type: 'media.tags.removed' as const,
          mediaId: 'm1',
          values: ['race'],
        },
        'n3',
        'n',
      ),
      sequence: 3,
      observed: { n: 2 },
    };
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }, 'n1', 'n'),
      {
        ...envelope(
          {
            type: 'media.tags.added' as const,
            mediaId: 'm1',
            values: ['Race', 'Ångström'],
          },
          'n2',
          'n',
        ),
        sequence: 2,
        observed: { n: 1 },
      },
      removed,
    ]);
    expect(state.media.m1.tags).toEqual(['Ångström']);
  });
  it('merges simultaneous independent set additions', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        { type: 'media.tags.added', mediaId: 'm1', values: ['A'] },
        'e2',
        'device-a',
      ),
      envelope(
        { type: 'media.tags.added', mediaId: 'm1', values: ['B'] },
        'e3',
        'device-b',
      ),
    ]);
    expect(state.media.m1.tags).toEqual(['A', 'B']);
    expect(state.conflicts).toHaveLength(0);
  });
  it('topologically replays three-device causality for every delivery order', () => {
    const z = {
      ...envelope({ type: 'media.registered', media }, 'z1', 'z-device'),
      sequence: 1,
      observed: { 'z-device': 0 },
    };
    const a = {
      ...envelope(
        { type: 'media.tags.added', mediaId: 'm1', values: ['Observed Z'] },
        'a1',
        'a-device',
      ),
      observed: { 'a-device': 0, 'z-device': 1 },
    };
    const b = {
      ...envelope(
        { type: 'media.tags.added', mediaId: 'm1', values: ['Concurrent B'] },
        'b1',
        'b-device',
      ),
      observed: { 'b-device': 0 },
    };
    const permutations = [
      [z, a, b],
      [z, b, a],
      [a, z, b],
      [a, b, z],
      [b, z, a],
      [b, a, z],
    ];
    const projections = permutations.map((events) => rebuildCatalogue(events));
    for (const state of projections) {
      expect(state.media.m1.tags).toEqual(['Concurrent B', 'Observed Z']);
      expect(state.appliedEventIds.indexOf('z1')).toBeLessThan(
        state.appliedEventIds.indexOf('a1'),
      );
      expect(state.issues).toEqual([]);
    }
    expect(
      projections.map(({ media, conflicts, appliedEventIds }) => ({
        media,
        conflicts,
        appliedEventIds,
      })),
    ).toEqual(
      Array.from({ length: 6 }, () => ({
        media: projections[0].media,
        conflicts: projections[0].conflicts,
        appliedEventIds: projections[0].appliedEventIds,
      })),
    );
  });
  it('reports unavailable causal dependencies while retaining valid events', () => {
    const dependent = {
      ...envelope({ type: 'media.registered', media }, 'a1', 'a-device'),
      observed: { 'a-device': 0, 'z-device': 3 },
    };
    const state = rebuildCatalogue([dependent]);
    expect(state.media.m1).toBeDefined();
    expect(state.issues[0].code).toBe('missing-causal-dependency');
  });
  it('retains concurrent scalar values as a conflict despite clock skew', () => {
    const registered = envelope({ type: 'media.registered', media });
    const first = envelope(
      { type: 'media.notes.set', mediaId: 'm1', value: 'Desktop note' },
      'e2',
      'desktop',
      '2030-01-01T00:00:00Z',
    );
    const second = envelope(
      { type: 'media.notes.set', mediaId: 'm1', value: 'Laptop note' },
      'e3',
      'laptop',
      '2020-01-01T00:00:00Z',
    );
    const state = rebuildCatalogue([second, registered, first]);
    expect(state.conflicts).toHaveLength(1);
    expect(state.conflicts[0].values).toEqual(['Desktop note', 'Laptop note']);
  });
  it('projects favourite events independently of registration defaults', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        { type: 'media.favourite.set', mediaId: 'm1', value: true },
        'e2',
      ),
    ]);
    expect(state.media.m1.favourite).toBe(true);
  });
  it('only removes set additions observed by the removing device', () => {
    const registered = envelope({ type: 'media.registered', media });
    const desktopAdd = envelope(
      { type: 'media.tags.added', mediaId: 'm1', values: ['Track'] },
      'e2',
      'desktop',
    );
    const laptopAdd = envelope(
      { type: 'media.tags.added', mediaId: 'm1', values: ['Night'] },
      'e3',
      'laptop',
    );
    const remove = {
      ...envelope(
        {
          type: 'media.tags.removed',
          mediaId: 'm1',
          values: ['Track', 'Night'],
        },
        'e4',
        'desktop',
      ),
      sequence: 2,
      observed: { desktop: 1 },
    };
    expect(
      rebuildCatalogue([laptopAdd, remove, registered, desktopAdd]).media.m1
        .tags,
    ).toEqual(['Night']);
  });
  it('replays legacy replacement events with exact set and record semantics', () => {
    const registered = envelope({
      type: 'media.registered',
      media: { ...media, tags: ['old', 'keep'], projectIds: ['old-project'] },
    });
    const tags = {
      ...envelope(
        { type: 'media.tags.replaced', mediaId: 'm1', values: ['keep'] },
        'e2',
      ),
      sequence: 2,
      observed: { a: 1 },
    };
    const projects = {
      ...envelope(
        { type: 'media.projects.replaced', mediaId: 'm1', values: [] },
        'e3',
      ),
      sequence: 3,
      observed: { a: 2 },
    };
    const snapshot = {
      ...envelope(
        {
          type: 'media.legacy-snapshot-replaced',
          media: {
            ...media,
            relativePath: 'final.mp4',
            tags: ['keep'],
            projectIds: [],
            notes: 'final note',
          },
        },
        'e4',
      ),
      sequence: 4,
      observed: { a: 3 },
    };
    const state = rebuildCatalogue([snapshot, projects, registered, tags]);
    expect(state.media.m1).toMatchObject({
      relativePath: 'final.mp4',
      tags: ['keep'],
      projectIds: [],
      notes: 'final note',
    });
  });
  it('replays enrichment and changed-file evidence from the canonical contract', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        {
          type: 'media.enriched',
          mediaId: 'm1',
          technical: { sizeBytes: 2, width: 1920, height: 1080 },
          thumbnailRef: 'm1.jpg',
        },
        'e2',
      ),
      envelope(
        {
          type: 'media.evidence-observed',
          mediaId: 'm1',
          observedSize: 3,
          observedModifiedMillis: 42,
          needsReview: true,
        },
        'e3',
      ),
    ]);
    expect(state.media.m1).toMatchObject({
      technical: { sizeBytes: 2, width: 1920, height: 1080 },
      thumbnailPath: 'm1.jpg',
      observedSize: 3,
      observedModifiedMillis: 42,
      needsReview: true,
    });
  });
  it('does not clear an integrity warning during ordinary metadata edits', () => {
    const state = rebuildCatalogue([
      envelope({
        type: 'media.registered',
        media: { ...media, needsReview: true },
      }),
      envelope(
        { type: 'media.tags.added', mediaId: 'm1', values: ['Track'] },
        'e2',
      ),
      envelope(
        { type: 'media.notes.set', mediaId: 'm1', value: 'Keep warning' },
        'e3',
      ),
    ]);
    expect(state.media.m1.needsReview).toBe(true);
    expect(state.media.m1.tags).toEqual(['Track']);
    expect(state.media.m1.notes).toBe('Keep warning');
  });
  it('retains integrity evidence independently for each device', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        {
          type: 'media.evidence-observed',
          mediaId: 'm1',
          observedSize: 3,
          observedSha256: 'changed',
          needsReview: true,
        },
        'desktop-evidence',
        'desktop',
      ),
      envelope(
        {
          type: 'media.evidence-observed',
          mediaId: 'm1',
          observedSize: 2,
          observedSha256: 'abc',
          needsReview: false,
        },
        'laptop-evidence',
        'laptop',
      ),
    ]);
    expect(state.media.m1.integrityByDevice?.desktop.needsReview).toBe(true);
    expect(state.media.m1.integrityByDevice?.laptop).toMatchObject({
      observedSha256: 'abc',
      needsReview: false,
    });
  });
  it('surfaces concurrent project record changes', () => {
    const project = {
      id: 'p1',
      name: 'One',
      workingDirectory: 'one',
      mediaIds: ['m1'],
      createdAt: '2026-10-18T00:00:00Z',
    };
    const state = rebuildCatalogue([
      envelope({ type: 'project.upserted', project }, 'p-a', 'desktop'),
      envelope(
        {
          type: 'project.upserted',
          project: { ...project, name: 'Two' },
        },
        'p-b',
        'laptop',
      ),
    ]);
    expect(state.conflicts).toHaveLength(1);
    expect(state.conflicts[0].field).toBe('project');
  });
  it('searches filename, folder tags and projects', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        {
          type: 'folder.upserted',
          folder: {
            id: 'f1',
            relativePath: 'media/2026-10-18',
            displayName: 'Repairs',
            tags: ['Go Kart'],
          },
        },
        'e2',
      ),
      envelope(
        {
          type: 'project.upserted',
          project: {
            id: 'p1',
            name: 'Simple Vlog 055',
            workingDirectory: 'v55',
            mediaIds: ['m1'],
            createdAt: '2026-10-18T00:00:00Z',
          },
        },
        'e3',
      ),
      envelope(
        { type: 'media.projects.added', mediaId: 'm1', values: ['p1'] },
        'e4',
      ),
    ]);
    expect(searchCatalogue(state, 'kart')).toEqual(['m1']);
    expect(searchCatalogue(state, 'vlog')).toEqual(['m1']);
  });
});

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

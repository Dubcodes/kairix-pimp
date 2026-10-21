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
  schemaVersion: 1,
  eventId: id,
  deviceId,
  sequence: 1,
  occurredAt,
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
          type: 'media.tags-set',
          mediaId: 'm1',
          tags: ['Workshop', 'Workshop'],
        },
        'e2',
      ),
      envelope(
        { type: 'media.projects-set', mediaId: 'm1', projectIds: ['p2', 'p1'] },
        'e3',
      ),
    ]);
    expect(state.media.m1.tags).toEqual(['Workshop']);
    expect(state.media.m1.projectIds).toEqual(['p1', 'p2']);
  });
  it('surfaces simultaneous independent edits instead of silently overwriting', () => {
    const state = rebuildCatalogue([
      envelope({ type: 'media.registered', media }),
      envelope(
        { type: 'media.tags-set', mediaId: 'm1', tags: ['A'] },
        'e2',
        'device-a',
      ),
      envelope(
        { type: 'media.tags-set', mediaId: 'm1', tags: ['B'] },
        'e3',
        'device-b',
      ),
    ]);
    expect(state.conflicts).toHaveLength(1);
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
        { type: 'media.projects-set', mediaId: 'm1', projectIds: ['p1'] },
        'e4',
      ),
    ]);
    expect(searchCatalogue(state, 'kart')).toEqual(['m1']);
    expect(searchCatalogue(state, 'vlog')).toEqual(['m1']);
  });
});

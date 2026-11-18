import { describe, expect, it } from 'vitest';
import type { MediaItem } from './bridge';
import { sortLibraryMedia, type LibraryMediaSort } from './library-sort';

const item = (
  id: string,
  filename: string,
  ingestDate: string,
  captureDate?: string,
): MediaItem => ({
  id,
  relativePath: `20261118/${filename}`,
  originalFilename: filename,
  sha256: id.padEnd(64, '0'),
  captureDate,
  captureDateProvenance: captureDate ? 'embedded' : 'unknown',
  ingestDate,
  fileType: 'mp4',
  tags: [],
  projectIds: [],
  notes: '',
  technical: { sizeBytes: 1 },
  available: true,
  needsReview: false,
});

describe('Library media ordering', () => {
  const media = [
    item('b', 'Clip 10.mp4', '2026-11-17T10:00:00Z', '2026-11-10'),
    item('a', 'clip 2.mp4', '2026-11-18T10:00:00Z', '2026-11-12'),
    item('c', 'unknown.mp4', '2026-11-16T10:00:00Z'),
  ];

  it('defaults to newest ingested and supports every independent media order', () => {
    const ids = (sort: LibraryMediaSort) =>
      sortLibraryMedia(media, sort).map((entry) => entry.id);
    expect(ids('ingested-newest')).toEqual(['a', 'b', 'c']);
    expect(ids('ingested-oldest')).toEqual(['c', 'b', 'a']);
    expect(ids('recorded-newest')).toEqual(['a', 'b', 'c']);
    expect(ids('recorded-oldest')).toEqual(['b', 'a', 'c']);
    expect(ids('filename-asc')).toEqual(['a', 'b', 'c']);
    expect(ids('filename-desc')).toEqual(['c', 'b', 'a']);
  });

  it('keeps missing recorded dates last and breaks equal dates deterministically', () => {
    const tied = [
      item('z', 'same.mp4', '2026-11-18', '2026-11-12'),
      item('a', 'Same.mp4', '2026-11-18', '2026-11-12'),
      item('m', 'missing.mp4', '2026-11-18'),
    ];
    expect(
      sortLibraryMedia(tied, 'recorded-newest').map(({ id }) => id),
    ).toEqual(['a', 'z', 'm']);
    expect(
      sortLibraryMedia(tied, 'recorded-oldest').map(({ id }) => id),
    ).toEqual(['a', 'z', 'm']);
  });

  it('orders filtered folder, search, tag, project, and unavailable subsets by ingest time', () => {
    const older = {
      ...item('older', 'older.mp4', '2026-11-01T10:00:00Z'),
      relativePath: 'older/older.mp4',
      tags: ['archive'],
      projectIds: ['project-a'],
      available: false,
    };
    const newer = {
      ...item('newer', 'newer.mp4', '2026-11-18T10:00:00Z'),
      relativePath: 'current/newer.mp4',
      tags: ['selected'],
      projectIds: ['project-a'],
    };
    const newest = {
      ...item('newest', 'matching.mp4', '2026-11-19T10:00:00Z'),
      relativePath: 'current/matching.mp4',
      tags: ['selected'],
      projectIds: ['project-b'],
    };
    const all = [older, newer, newest];
    const ids = (subset: MediaItem[]) =>
      sortLibraryMedia(subset, 'ingested-newest').map(({ id }) => id);

    expect(ids(all)).toEqual(['newest', 'newer', 'older']);
    expect(
      ids(all.filter((entry) => entry.relativePath.startsWith('current/'))),
    ).toEqual(['newest', 'newer']);
    expect(
      ids(all.filter((entry) => entry.originalFilename.includes('match'))),
    ).toEqual(['newest']);
    expect(ids(all.filter((entry) => entry.tags.includes('selected')))).toEqual(
      ['newest', 'newer'],
    );
    expect(
      ids(all.filter((entry) => entry.projectIds.includes('project-a'))),
    ).toEqual(['newer', 'older']);
    expect(ids(all.filter((entry) => !entry.available))).toEqual(['older']);
  });
});

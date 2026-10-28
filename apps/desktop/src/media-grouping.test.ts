import { describe, expect, it } from 'vitest';
import type { Catalogue, MediaItem } from './bridge';
import {
  associatedMedia,
  expandLogicalMediaIds,
  logicalMediaMatches,
  logicalProjectIds,
  logicalProjectRecordingCount,
  logicalTags,
  visibleMedia,
} from './media-grouping';

const item = (id: string, filename: string, type: string): MediaItem => ({
  id,
  relativePath: `20261027/${filename}`,
  originalFilename: filename,
  sha256: `${id}-hash`,
  captureDateProvenance: 'unknown',
  ingestDate: '2026-10-27',
  fileType: type,
  tags: [],
  projectIds: [],
  notes: '',
  technical: { sizeBytes: 10 },
  available: true,
  needsReview: false,
});

const video = {
  ...item('video', 'CLIP001.MP4', 'mp4'),
  attachedAudioId: 'audio',
};
const audio = {
  ...item('audio', 'clip001.wav', 'wav'),
  tags: ['boom'],
  projectIds: ['project'],
};
const independent = item('independent', 'room-tone.wav', 'wav');
const catalogue: Catalogue = {
  media: [video, audio, independent],
  projects: [
    {
      id: 'project',
      name: 'Sound Unit',
      workingDirectory: '.',
      mediaIds: ['audio'],
      notes: '',
      tags: [],
      createdAt: '2026-10-27T00:00:00Z',
    },
  ],
  updatedAt: '2026-10-27T00:00:00Z',
};

describe('logical media grouping', () => {
  it('hides only attached audio unless separate cards are requested', () => {
    expect(visibleMedia(catalogue.media, false).map(({ id }) => id)).toEqual([
      'video',
      'independent',
    ]);
    expect(visibleMedia(catalogue.media, true)).toEqual(catalogue.media);
  });

  it('returns both physical records and expands logical operations once', () => {
    expect(associatedMedia(video, catalogue.media).map(({ id }) => id)).toEqual(
      ['video', 'audio'],
    );
    expect(
      expandLogicalMediaIds(['video', 'audio', 'independent'], catalogue.media),
    ).toEqual(['video', 'audio', 'independent']);
    expect(expandLogicalMediaIds(['audio'], catalogue.media)).toEqual([
      'audio',
    ]);
    expect(associatedMedia(audio, catalogue.media)).toEqual([audio]);
  });

  it('finds the primary card by attached WAV filename, tags, and project', () => {
    expect(logicalMediaMatches(video, 'clip001.wav', catalogue)).toBe(true);
    expect(logicalMediaMatches(video, 'boom', catalogue)).toBe(true);
    expect(logicalMediaMatches(video, 'Sound Unit', catalogue)).toBe(true);
    expect(logicalMediaMatches(video, 'missing', catalogue)).toBe(false);
  });

  it('surfaces attached WAV metadata and counts a pair once per project', () => {
    expect(logicalTags(video, catalogue.media)).toEqual(['boom']);
    expect(logicalProjectIds(video, catalogue.media)).toEqual(['project']);
    expect(logicalProjectRecordingCount('project', catalogue.media)).toBe(1);
    expect(audio.tags).toEqual(['boom']);
    expect(audio.projectIds).toEqual(['project']);
  });
});

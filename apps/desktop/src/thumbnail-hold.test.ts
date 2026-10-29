// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { HoverPreview } from './bridge';
import {
  holdFrameIndex,
  readLastHoveredRatio,
  writeLastHoveredRatio,
} from './thumbnail-hold';

const preview: HoverPreview = {
  mediaId: 'clip-a',
  durationSeconds: 100,
  frameTimes: [10.01, 22.5, 49.8, 81.2, 99.4],
  frames: ['first', '20', 'middle', '80', 'last'],
};

describe('thumbnail hold frames', () => {
  beforeEach(() => localStorage.clear());

  it('selects the nearest actual sampled timestamp', () => {
    expect(holdFrameIndex(preview, 'first')).toBe(0);
    expect(holdFrameIndex(preview, '20-percent')).toBe(1);
    expect(holdFrameIndex(preview, 'middle')).toBe(2);
    expect(holdFrameIndex(preview, '80-percent')).toBe(3);
    expect(holdFrameIndex(preview, 'last')).toBe(4);
  });

  it('persists bounded per-media last-hover positions', () => {
    vi.spyOn(Date, 'now').mockReturnValue(1000);
    writeLastHoveredRatio('clip-a', 0.81);
    writeLastHoveredRatio('clip-b', 0.2);
    expect(readLastHoveredRatio('clip-a')).toBe(0.81);
    expect(readLastHoveredRatio('clip-b')).toBe(0.2);
    expect(holdFrameIndex(preview, 'last-hovered', 0.81)).toBe(3);
    vi.restoreAllMocks();
  });
});

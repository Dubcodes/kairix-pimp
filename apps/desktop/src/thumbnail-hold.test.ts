// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { HoverPreview } from './bridge';
import {
  holdFrameIndex,
  readLastHoveredRatio,
  resetLastHoveredRatioMemoryCache,
  writeLastHoveredRatio,
} from './thumbnail-hold';

const preview: HoverPreview = {
  mediaId: 'clip-a',
  durationSeconds: 100,
  frameTimes: [10.01, 22.5, 49.8, 81.2, 99.4],
  frames: ['first', '20', 'middle', '80', 'last'],
};

describe('thumbnail hold frames', () => {
  beforeEach(() => {
    localStorage.clear();
    resetLastHoveredRatioMemoryCache();
  });

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

  it('parses storage once across hundreds of card reads and survives remount', () => {
    const stored = Object.fromEntries(
      Array.from({ length: 500 }, (_, index) => [
        `clip-${index}`,
        { ratio: index / 500, touchedAt: index },
      ]),
    );
    localStorage.setItem(
      'pimp-thumbnail-last-hover-v1',
      JSON.stringify(stored),
    );
    resetLastHoveredRatioMemoryCache();
    const getItem = vi.spyOn(Storage.prototype, 'getItem');
    for (let index = 0; index < 500; index += 1)
      expect(readLastHoveredRatio(`clip-${index}`)).toBe(index / 500);
    expect(getItem).toHaveBeenCalledTimes(1);

    resetLastHoveredRatioMemoryCache();
    expect(readLastHoveredRatio('clip-405')).toBe(0.81);
    expect(getItem).toHaveBeenCalledTimes(2);
  });

  it('avoids unchanged writes, bounds entries and recovers from malformed data', () => {
    localStorage.setItem('pimp-thumbnail-last-hover-v1', '{broken');
    resetLastHoveredRatioMemoryCache();
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    expect(readLastHoveredRatio('missing')).toBeUndefined();
    expect(writeLastHoveredRatio('clip-a', 0.4)).toBe(true);
    expect(writeLastHoveredRatio('clip-a', 0.4)).toBe(false);
    for (let index = 0; index < 550; index += 1)
      writeLastHoveredRatio(`clip-${index}`, index / 550);
    expect(setItem).toHaveBeenCalledTimes(551);
    resetLastHoveredRatioMemoryCache();
    expect(readLastHoveredRatio('clip-549')).toBe(549 / 550);
    expect(readLastHoveredRatio('clip-a')).toBeUndefined();
  });
});

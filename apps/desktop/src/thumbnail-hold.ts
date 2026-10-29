import type { HoverPreview, ThumbnailHoldFrame } from './bridge';

const STORAGE_KEY = 'pimp-thumbnail-last-hover-v1';
const MAX_ENTRIES = 500;

type StoredPosition = { ratio: number; touchedAt: number };

let positionCache: Record<string, StoredPosition> | undefined;

const fixedRatios: Record<
  Exclude<ThumbnailHoldFrame, 'last-hovered'>,
  number
> = {
  first: 0,
  '10-percent': 0.1,
  '20-percent': 0.2,
  middle: 0.5,
  '80-percent': 0.8,
  last: 1,
};

export function closestPreviewFrame(preview: HoverPreview, ratio: number) {
  if (!preview.frames.length) return undefined;
  const duration = preview.durationSeconds;
  if (!Number.isFinite(duration) || duration <= 0) {
    return Math.min(
      preview.frames.length - 1,
      Math.round(Math.max(0, Math.min(1, ratio)) * (preview.frames.length - 1)),
    );
  }
  const target = Math.max(0, Math.min(1, ratio)) * duration;
  let closest = 0;
  for (let index = 1; index < preview.frameTimes.length; index += 1) {
    if (
      Math.abs(preview.frameTimes[index] - target) <
      Math.abs(preview.frameTimes[closest] - target)
    )
      closest = index;
  }
  return Math.min(closest, preview.frames.length - 1);
}

export function holdFrameIndex(
  preview: HoverPreview,
  mode: ThumbnailHoldFrame,
  lastHoveredRatio?: number,
) {
  const ratio =
    mode === 'last-hovered' ? (lastHoveredRatio ?? 0) : fixedRatios[mode];
  return closestPreviewFrame(preview, ratio);
}

function readPositions(): Record<string, StoredPosition> {
  if (positionCache) return positionCache;
  try {
    const parsed = JSON.parse(
      localStorage.getItem(STORAGE_KEY) ?? '{}',
    ) as Record<string, StoredPosition>;
    positionCache = parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    positionCache = {};
  }
  return positionCache;
}

export function readLastHoveredRatio(mediaId: string) {
  const ratio = readPositions()[mediaId]?.ratio;
  return Number.isFinite(ratio) ? Math.max(0, Math.min(1, ratio)) : undefined;
}

export function writeLastHoveredRatio(mediaId: string, ratio: number) {
  const positions = readPositions();
  const normalizedRatio = Math.max(0, Math.min(1, ratio));
  if (positions[mediaId]?.ratio === normalizedRatio) return false;
  const updated = {
    ...positions,
    [mediaId]: {
      ratio: normalizedRatio,
      touchedAt: Date.now(),
    },
  };
  positionCache = Object.fromEntries(
    Object.entries(updated)
      .sort((left, right) => right[1].touchedAt - left[1].touchedAt)
      .slice(0, MAX_ENTRIES),
  );
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(positionCache));
  } catch {
    // Persistence is best-effort; the in-memory card state still retains it.
  }
  return true;
}

export function resetLastHoveredRatioMemoryCache() {
  positionCache = undefined;
}

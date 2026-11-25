import { useCallback, useEffect, useRef, useState } from 'react';
import {
  desktopBridge,
  type DeviceSettings,
  type HoverPreview,
} from '../bridge';
import { hoverPreviewRequestCache } from '../hover-preview-cache';
import { idlePreviewScheduler } from '../idle-preview-scheduler';
import {
  previewWorkQueue,
  staticThumbnailWorkQueue,
  type PreviewPriority,
} from '../preview-work-queue';
import {
  holdFrameIndex,
  readLastHoveredRatio,
  writeLastHoveredRatio,
} from '../thumbnail-hold';
import {
  StaticThumbnailBackoffError,
  StaticThumbnailRequestRegistry,
} from '../static-thumbnail-requests';

const staticThumbnailRequests = new StaticThumbnailRequestRegistry();
const staticThumbnailQueueKey = (key: string, cacheGeneration: number) =>
  `static:${cacheGeneration}:${key}`;
const requestStaticThumbnail = (
  key: string,
  mediaId: string,
  priority: PreviewPriority,
  cacheGeneration: number,
) => {
  const requestKey = `${cacheGeneration}:${key}`;
  staticThumbnailWorkQueue.promote(
    staticThumbnailQueueKey(key, cacheGeneration),
    priority,
  );
  return staticThumbnailRequests.request(requestKey, () =>
    staticThumbnailWorkQueue.schedule(
      staticThumbnailQueueKey(key, cacheGeneration),
      priority,
      'Static thumbnail',
      () => desktopBridge.prepareMediaThumbnail(mediaId),
    ),
  );
};

const previewableVideoTypes = new Set([
  'mp4',
  'mov',
  'm4v',
  'mxf',
  'mts',
  'm2ts',
  'avi',
]);

const previewDuration = (seconds?: number) =>
  seconds == null
    ? '—'
    : `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, '0')}`;

export function MediaThumbnail({
  mediaId,
  fileType,
  thumbnailSource,
  sourceAvailable = false,
  previewKey,
  previewCacheEpoch = 0,
  enableHoverPreview = false,
  holdFrame = 'first',
  preloading = 'visible-nearby',
  allowBackgroundPreparation = true,
  onThumbnailPrepared,
  previewStored = false,
  onPreviewPrepared,
}: {
  mediaId: string;
  fileType: string;
  thumbnailSource?: string;
  sourceAvailable?: boolean;
  previewKey?: string;
  previewCacheEpoch?: number;
  enableHoverPreview?: boolean;
  holdFrame?: DeviceSettings['thumbnailHoldFrame'];
  preloading?: DeviceSettings['previewPreloading'];
  allowBackgroundPreparation?: boolean;
  onThumbnailPrepared?: (mediaId: string, source: string) => void;
  previewStored?: boolean;
  onPreviewPrepared?: (mediaId: string) => void;
}) {
  const thumbnailRef = useRef<HTMLDivElement>(null);
  const [nearViewport, setNearViewport] = useState(
    () => typeof IntersectionObserver === 'undefined',
  );
  const [previewEntry, setPreviewEntry] = useState<{
    key: string;
    epoch: number;
    preview: HoverPreview;
  }>();
  const [previewIndex, setPreviewIndex] = useState<number>();
  const [lastHoveredRatio, setLastHoveredRatio] = useState(() =>
    readLastHoveredRatio(mediaId),
  );
  const latestHoveredRatio = useRef(lastHoveredRatio);
  const [isHovering, setIsHovering] = useState(false);
  const [failedImageSource, setFailedImageSource] = useState<string>();
  const [staticImageRetry, setStaticImageRetry] = useState<{
    source: string;
    attempt: number;
  }>();
  const staticImageRetryTimer = useRef<number | undefined>(undefined);
  const requestGeneration = useRef(0);
  const staticRequestGeneration = useRef(0);
  const releasePreview = useCallback(() => {
    requestGeneration.current += 1;
    setPreviewIndex(undefined);
    setPreviewEntry(undefined);
  }, []);
  const finishHover = () => {
    setIsHovering(false);
    setPreviewIndex(undefined);
    if (latestHoveredRatio.current != null)
      writeLastHoveredRatio(mediaId, latestHoveredRatio.current);
  };
  useEffect(() => {
    requestGeneration.current += 1;
  }, [previewCacheEpoch]);
  useEffect(() => {
    return () => {
      if (staticImageRetryTimer.current != null)
        window.clearTimeout(staticImageRetryTimer.current);
    };
  }, [thumbnailSource]);
  const preview =
    previewEntry &&
    previewEntry.key === previewKey &&
    previewEntry.epoch === previewCacheEpoch
      ? previewEntry.preview
      : undefined;
  const previewEligible =
    Boolean(previewKey) &&
    (sourceAvailable || previewStored) &&
    enableHoverPreview &&
    previewableVideoTypes.has(fileType.toLowerCase()) &&
    !window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  const requestPreview = useCallback(
    (priority: PreviewPriority = 2) => {
      if (preview || !previewKey || !previewEligible) return;
      previewWorkQueue.promote(`load:${previewKey}`, priority);
      const generation = ++requestGeneration.current;
      const request = hoverPreviewRequestCache.request(previewKey, () =>
        desktopBridge.loadHoverPreview(mediaId),
      );
      void request
        .then((result) => {
          if (
            generation === requestGeneration.current &&
            result.mediaId === mediaId
          ) {
            setPreviewEntry({
              key: previewKey,
              epoch: previewCacheEpoch,
              preview: result,
            });
            onPreviewPrepared?.(mediaId);
          }
        })
        .catch(() => undefined);
    },
    [
      mediaId,
      preview,
      previewCacheEpoch,
      previewEligible,
      previewKey,
      onPreviewPrepared,
    ],
  );
  useEffect(() => {
    const element = thumbnailRef.current;
    if (!element || typeof IntersectionObserver === 'undefined') return;
    const generation = ++staticRequestGeneration.current;
    let nearby = false;
    let attempts = 0;
    let retryTimer: number | undefined;
    const request = (priority: PreviewPriority) => {
      if (
        !nearby ||
        thumbnailSource ||
        !sourceAvailable ||
        !previewKey ||
        !previewableVideoTypes.has(fileType.toLowerCase())
      )
        return;
      void requestStaticThumbnail(
        previewKey,
        mediaId,
        priority,
        previewCacheEpoch,
      )
        .then((source) => {
          if (generation === staticRequestGeneration.current)
            onThumbnailPrepared?.(mediaId, source);
        })
        .catch((reason) => {
          if (
            generation !== staticRequestGeneration.current ||
            !nearby ||
            attempts >= 4
          )
            return;
          attempts += 1;
          const delay =
            reason instanceof StaticThumbnailBackoffError
              ? Math.max(50, reason.retryAt - Date.now())
              : Math.min(30_000, 500 * 2 ** (attempts - 1));
          retryTimer = window.setTimeout(() => request(priority), delay);
        });
    };
    const nearObserver = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          nearby = true;
          setNearViewport(true);
          setFailedImageSource((failed) =>
            failed === thumbnailSource ? undefined : failed,
          );
          request(1);
        } else {
          nearby = false;
          setNearViewport(false);
          if (retryTimer != null) window.clearTimeout(retryTimer);
          if (previewKey)
            staticThumbnailWorkQueue.cancel(
              staticThumbnailQueueKey(previewKey, previewCacheEpoch),
            );
        }
      },
      { rootMargin: '1200px' },
    );
    const visibleObserver = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          nearby = true;
          request(0);
        }
      },
      { rootMargin: '0px' },
    );
    nearObserver.observe(element);
    visibleObserver.observe(element);
    return () => {
      staticRequestGeneration.current += 1;
      nearby = false;
      if (retryTimer != null) window.clearTimeout(retryTimer);
      nearObserver.disconnect();
      visibleObserver.disconnect();
      if (previewKey)
        staticThumbnailWorkQueue.cancel(
          staticThumbnailQueueKey(previewKey, previewCacheEpoch),
        );
    };
  }, [
    fileType,
    mediaId,
    onThumbnailPrepared,
    previewKey,
    previewCacheEpoch,
    sourceAvailable,
    thumbnailSource,
  ]);
  useEffect(() => {
    const element = thumbnailRef.current;
    if (!element || typeof IntersectionObserver === 'undefined') return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) releasePreview();
      },
      { rootMargin: '100px' },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [releasePreview]);
  useEffect(() => {
    const element = thumbnailRef.current;
    if (
      !element ||
      preview ||
      !thumbnailSource ||
      !previewEligible ||
      preloading === 'hovered' ||
      preloading === 'off' ||
      typeof IntersectionObserver === 'undefined'
    )
      return;
    const nearby = new IntersectionObserver(
      (entries) => {
        const isNearby = entries.some((entry) => entry.isIntersecting);
        if (isNearby && allowBackgroundPreparation && previewKey)
          void previewWorkQueue
            .schedule(`prepare:${previewKey}`, 4, fileType.toUpperCase(), () =>
              desktopBridge.prepareHoverPreview(mediaId),
            )
            .then(() => onPreviewPrepared?.(mediaId))
            .catch(() => undefined);
        else if (
          !isNearby &&
          preloading === 'idle' &&
          allowBackgroundPreparation &&
          previewKey
        )
          void idlePreviewScheduler
            .schedule(`prepare:${previewKey}`, () =>
              previewWorkQueue
                .schedule(
                  `prepare:${previewKey}`,
                  5,
                  fileType.toUpperCase(),
                  () => desktopBridge.prepareHoverPreview(mediaId),
                )
                .then(() => onPreviewPrepared?.(mediaId)),
            )
            .catch(() => undefined);
      },
      { rootMargin: preloading === 'visible' ? '0px' : '400px' },
    );
    const visible = new IntersectionObserver(
      (entries) => {
        if (
          allowBackgroundPreparation &&
          entries.some((entry) => entry.isIntersecting)
        )
          requestPreview(3);
      },
      { rootMargin: '0px' },
    );
    nearby.observe(element);
    visible.observe(element);
    return () => {
      nearby.disconnect();
      visible.disconnect();
      if (previewKey) idlePreviewScheduler.cancel(`prepare:${previewKey}`);
    };
  }, [
    allowBackgroundPreparation,
    fileType,
    mediaId,
    preloading,
    preview,
    previewEligible,
    previewKey,
    onPreviewPrepared,
    requestPreview,
    thumbnailSource,
  ]);
  const displayedFrame =
    preview &&
    (previewIndex ?? holdFrameIndex(preview, holdFrame, lastHoveredRatio)) !=
      null
      ? preview.frames[
          previewIndex ?? holdFrameIndex(preview, holdFrame, lastHoveredRatio)!
        ]
      : undefined;
  const displayedIndex =
    previewIndex ??
    (preview
      ? holdFrameIndex(preview, holdFrame, lastHoveredRatio)
      : undefined);
  const imageSource =
    displayedFrame && displayedFrame !== failedImageSource
      ? displayedFrame
      : thumbnailSource && thumbnailSource !== failedImageSource
        ? thumbnailSource
        : undefined;
  const renderedImageSource =
    imageSource &&
    imageSource === thumbnailSource &&
    staticImageRetry?.source === thumbnailSource
      ? `${imageSource}${imageSource.includes('?') ? '&' : '?'}pimp_retry=${staticImageRetry.attempt}`
      : imageSource;
  return (
    <div
      ref={thumbnailRef}
      className="media-thumbnail"
      onPointerEnter={() => {
        if (previewKey) idlePreviewScheduler.cancel(`prepare:${previewKey}`);
        setIsHovering(true);
        requestPreview(0);
      }}
      onPointerMove={(event) => {
        idlePreviewScheduler.markInteraction();
        if (!preview?.frames.length) return;
        const bounds = event.currentTarget.getBoundingClientRect();
        const ratio = bounds.width
          ? Math.min(
              1,
              Math.max(0, (event.clientX - bounds.left) / bounds.width),
            )
          : 0;
        const index = Math.min(
          preview.frames.length - 1,
          Math.floor(ratio * preview.frames.length),
        );
        setPreviewIndex(index);
        setLastHoveredRatio(ratio);
        latestHoveredRatio.current = ratio;
      }}
      onPointerLeave={finishHover}
      aria-label={
        preview && isHovering
          ? 'Move horizontally to preview video frames'
          : undefined
      }
    >
      {renderedImageSource ? (
        <img
          src={renderedImageSource}
          alt=""
          draggable={false}
          loading={nearViewport ? 'eager' : 'lazy'}
          decoding="async"
          onDragStart={(event) => event.preventDefault()}
          onError={() => {
            if (imageSource !== thumbnailSource || !thumbnailSource) {
              setFailedImageSource(imageSource);
              return;
            }
            const attempt =
              staticImageRetry?.source === thumbnailSource
                ? staticImageRetry.attempt
                : 0;
            if (attempt >= 2) {
              setFailedImageSource(thumbnailSource);
              void desktopBridge.logUiError(
                `Static thumbnail failed after bounded retries for media ${mediaId}`,
              );
              return;
            }
            if (staticImageRetryTimer.current != null)
              window.clearTimeout(staticImageRetryTimer.current);
            staticImageRetryTimer.current = window.setTimeout(
              () => {
                setStaticImageRetry({
                  source: thumbnailSource,
                  attempt: attempt + 1,
                });
                staticImageRetryTimer.current = undefined;
              },
              attempt === 0 ? 250 : 1_000,
            );
          }}
        />
      ) : (
        <span className="file-kind">{fileType.toUpperCase()}</span>
      )}
      {preview && (
        <span
          className="preview-ready-indicator"
          title="Scrubbable preview ready"
          aria-label="Scrubbable preview ready"
        />
      )}
      {!preview && previewStored && (
        <span className="sr-only">
          Shared scrubbable preview stored in Master Bin
        </span>
      )}
      {isHovering &&
        displayedIndex != null &&
        preview?.frameTimes[displayedIndex] != null && (
          <span className="preview-timecode">
            {previewDuration(preview.frameTimes[displayedIndex])}
          </span>
        )}
    </div>
  );
}

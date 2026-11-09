import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from 'react';
import { getCurrentWindow, ProgressBarStatus } from '@tauri-apps/api/window';
import {
  Archive,
  ArrowLeft,
  ArrowUp,
  ChevronDown,
  ChevronUp,
  Clock3,
  Coffee,
  Copy,
  FileText,
  Folder,
  FolderOpen,
  HardDrive,
  Github,
  Inbox,
  Library,
  LocateFixed,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Star,
  Tag,
  UploadCloud,
  UsersRound,
  Volume2,
  VolumeX,
} from 'lucide-react';
import { SearchableComboBox } from './components/SearchableComboBox';
import {
  desktopBridge,
  emptyCatalogue,
  emptySettings,
  nativeAvailable,
  type NativeDragOutcome,
  type NativeDragThreshold,
  type Catalogue,
  type AppInfo,
  type BackupJob,
  type BackgroundOperation,
  type DeviceSettings,
  type DiscoveryStatus,
  type IngestJob,
  type IngestJobFile,
  type IngestProgress,
  type IngestResult,
  type MediaToolsStatus,
  type HoverPreview,
  type ProjectInventory,
  type ProjectRecord,
  type ProvisionalMetadataPatch,
  type SourceInventory,
  type RemovableVolume,
  type ResourceGovernorStatus,
  type UpdaterConfiguration,
} from './bridge';
import { datePatternPreview } from './date-pattern';
import {
  checkApprovedBetaUpdate,
  installApprovedBetaUpdate,
  type AvailableUpdate,
} from './updates';
import { SettingsWriteQueue } from './settings-write-queue';
import { hoverPreviewRequestCache } from './hover-preview-cache';
import { idlePreviewScheduler } from './idle-preview-scheduler';
import {
  previewWorkQueue,
  staticThumbnailWorkQueue,
  type PreviewPriority,
  type PreviewQueueStatus,
} from './preview-work-queue';
import {
  holdFrameIndex,
  readLastHoveredRatio,
  writeLastHoveredRatio,
} from './thumbnail-hold';
import {
  StaticThumbnailBackoffError,
  StaticThumbnailRequestRegistry,
} from './static-thumbnail-requests';
import {
  associatedMedia,
  expandLogicalMediaIds,
  logicalMediaMatches,
  logicalProjectIds,
  logicalProjectRecordingCount,
  logicalTags,
  visibleMedia,
} from './media-grouping';

type View = 'Ingest' | 'Library' | 'Projects' | 'Archive' | 'Settings';
type SettingsSection =
  | 'General'
  | 'Library & Storage'
  | 'Ingest & Discovery'
  | 'Resource Management'
  | 'Appearance'
  | 'Updates'
  | 'About & Support'
  | 'Diagnostics';
type FolderSort = NonNullable<DeviceSettings['libraryFolderSort']>;
type SelectionToolbarAction = 'tags' | 'projects' | 'favourite';
type SelectionToolbarPosition =
  'bottom-right' | 'bottom-center' | 'bottom-left';
const defaultSelectionToolbarActions: SelectionToolbarAction[] = [
  'tags',
  'projects',
  'favourite',
];

const icons = {
  Ingest: Inbox,
  Library,
  Projects: UsersRound,
  Archive,
  Settings,
};
const projectCategoryLabels: Record<string, string> = {
  'premiere-project': 'Premiere project',
  'premiere-auto-save': 'Premiere auto-saves',
  video: 'Video media',
  'audio-music': 'Audio and music',
  'images-graphics': 'Images and graphics',
  exports: 'Exports',
  'notes-documentation': 'Notes and documentation',
  other: 'Other project assets',
};
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
const size = (bytes: number) =>
  bytes < 1024 * 1024
    ? `${Math.round(bytes / 1024)} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
const duration = (seconds?: number) =>
  seconds == null
    ? '—'
    : `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, '0')}`;
const enrichmentVideoType = (fileType: string) =>
  [
    'mp4',
    'mov',
    'm4v',
    'mxf',
    'mts',
    'm2ts',
    'avi',
    'h264',
    'h265',
    'hevc',
    'r3d',
    'braw',
  ].includes(fileType.toLocaleLowerCase());

const keyboardInputOwner = (target: EventTarget | null) => {
  const selector = [
    'input',
    'textarea',
    'select',
    '[contenteditable]:not([contenteditable="false"])',
    '[role="combobox"]',
    '[role="textbox"]',
    '[role="menu"]',
    '[role="dialog"]',
    '[aria-modal="true"]',
  ].join(', ');
  const element = target instanceof Element ? target : null;
  const active = document.activeElement;
  if (element?.closest(selector) || active?.closest(selector)) return true;
  return Boolean(
    document.querySelector(
      '[aria-modal="true"], .combo-popover, [role="menu"]:not([hidden])',
    ),
  );
};
const errorText = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

const defaultNativeDragThreshold: NativeDragThreshold = {
  horizontal: 4,
  vertical: 4,
};

const nativeDragMovementExceeded = (
  origin: { x: number; y: number },
  current: { x: number; y: number },
  threshold: NativeDragThreshold,
) =>
  Math.abs(current.x - origin.x) >= Math.max(1, threshold.horizontal) ||
  Math.abs(current.y - origin.y) >= Math.max(1, threshold.vertical);

interface PendingNativeDrag {
  pointerId: number;
  origin: { x: number; y: number };
  key: string;
  run: () => Promise<NativeDragOutcome>;
  droppedMessage: string;
}

export function App() {
  const native = nativeAvailable();
  const [settings, setSettings] = useState<DeviceSettings>(emptySettings);
  const [catalogue, setCatalogue] = useState<Catalogue>(emptyCatalogue);
  const [view, setView] = useState<View>('Library');
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>('General');
  const [query, setQuery] = useState('');
  const [tagFilters, setTagFilters] = useState<string[]>([]);
  const [includeUnavailable, setIncludeUnavailable] = useState(false);
  const [folder, setFolder] = useState('All media');
  const [selected, setSelected] = useState<string[]>([]);
  const [nativeDragThreshold, setNativeDragThreshold] =
    useState<NativeDragThreshold>(defaultNativeDragThreshold);
  const [selectedProjectId, setSelectedProjectId] = useState<string>();
  const [projectInventory, setProjectInventory] = useState<ProjectInventory>();
  const [projectInventoryLoading, setProjectInventoryLoading] = useState(false);
  const [projectInventoryError, setProjectInventoryError] = useState('');
  const [selectedPremiereSequenceId, setSelectedPremiereSequenceId] =
    useState<string>();
  const [projectMediaQuery, setProjectMediaQuery] = useState('');
  const [projectAvailableOpen, setProjectAvailableOpen] = useState(false);
  const [projectAvailableQuery, setProjectAvailableQuery] = useState('');
  const [projectAvailableTags, setProjectAvailableTags] = useState<string[]>(
    [],
  );
  const [projectAvailablePage, setProjectAvailablePage] = useState(1);
  const [projectMediaSelection, setProjectMediaSelection] = useState<string[]>(
    [],
  );
  const [projectAddTags, setProjectAddTags] = useState<string[]>([]);
  const [projectRemoveTags, setProjectRemoveTags] = useState<string[]>([]);
  const [projectMediaDescription, setProjectMediaDescription] = useState('');
  const [projectNotes, setProjectNotes] = useState('');
  const [projectTags, setProjectTags] = useState<string[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [dateOverride, setDateOverride] = useState('today');
  const [chosenDate, setChosenDate] = useState('');
  const [progress, setProgress] = useState<IngestProgress>();
  const [jobs, setJobs] = useState<IngestJob[]>([]);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [backgroundOperations, setBackgroundOperations] = useState<
    BackgroundOperation[]
  >([]);
  const [resourceStatus, setResourceStatus] =
    useState<ResourceGovernorStatus>();
  const [completedJobsOpen, setCompletedJobsOpen] = useState(false);
  const [clearingCompletedJobs, setClearingCompletedJobs] = useState(false);
  const [startingIngest, setStartingIngest] = useState(false);
  const [inventory, setInventory] = useState<SourceInventory>();
  const [inventoryLoading, setInventoryLoading] = useState(false);
  const [inventoryError, setInventoryError] = useState('');
  const [sdGrabberMode, setSdGrabberMode] = useState(false);
  const [removableVolumes, setRemovableVolumes] = useState<RemovableVolume[]>(
    [],
  );
  const [showSdGrabber, setShowSdGrabber] = useState(false);
  const [allowDuplicateCopies, setAllowDuplicateCopies] = useState(false);
  const [allowUnverifiedReimport, setAllowUnverifiedReimport] = useState(false);
  const [conflicts, setConflicts] = useState<IngestResult['conflicts']>([]);
  const [renames, setRenames] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [settingsWritePending, setSettingsWritePending] = useState(false);
  const [settingsWriteFailed, setSettingsWriteFailed] = useState(false);
  const [mediaTools, setMediaTools] = useState<MediaToolsStatus>();
  const [thumbnailSources, setThumbnailSources] = useState<
    Record<string, string>
  >({});
  const [thumbnailSourceRequestCount, setThumbnailSourceRequestCount] =
    useState(0);
  const [previewCacheUsage, setPreviewCacheUsage] = useState(0);
  const [previewStoredIds, setPreviewStoredIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [previewMemoryUsage, setPreviewMemoryUsage] = useState(0);
  const [previewMemoryCount, setPreviewMemoryCount] = useState(0);
  const [previewQueueStatus, setPreviewQueueStatus] =
    useState<PreviewQueueStatus>(previewWorkQueue.status);
  const [staticThumbnailQueueStatus, setStaticThumbnailQueueStatus] =
    useState<PreviewQueueStatus>(staticThumbnailWorkQueue.status);
  const [previewCacheEpoch, setPreviewCacheEpoch] = useState(0);
  const [clearingPreviewCache, setClearingPreviewCache] = useState(false);
  const [applicationInfo, setApplicationInfo] = useState<AppInfo>();
  const [updaterConfiguration, setUpdaterConfiguration] =
    useState<UpdaterConfiguration>();
  const [availableUpdate, setAvailableUpdate] = useState<AvailableUpdate>();
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateStatus, setUpdateStatus] = useState(
    'Loading update configuration…',
  );
  const [updateProgress, setUpdateProgress] = useState<{
    downloaded: number;
    total?: number;
  }>();
  const [discoveryStatus, setDiscoveryStatus] = useState<DiscoveryStatus>({
    running: false,
    full: false,
    filesProcessed: 0,
    totalFiles: 0,
    registered: 0,
    changed: 0,
    pending: 0,
    errors: 0,
  });
  const [batchTags, setBatchTags] = useState<string[]>([]);
  const [removeBatchTags, setRemoveBatchTags] = useState<string[]>([]);
  const [batchProjects, setBatchProjects] = useState<string[]>([]);
  const [removeBatchProjects, setRemoveBatchProjects] = useState<string[]>([]);
  const [descriptionDraft, setDescriptionDraft] = useState({
    selectionKey: '',
    value: '',
  });
  const [descriptionSaveStatus, setDescriptionSaveStatus] = useState<{
    selectionKey: string;
    state: 'idle' | 'saving' | 'saved' | 'failed';
    detail?: string;
  }>({ selectionKey: '', state: 'idle' });
  const [ingestBatchTags, setIngestBatchTags] = useState<string[]>([]);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const [viewerMediaId, setViewerMediaId] = useState<string>();
  const [viewerSource, setViewerSource] = useState<string>();
  const [viewerError, setViewerError] = useState('');
  const [viewerProxyBusy, setViewerProxyBusy] = useState(false);
  const [viewerPlaybackMetrics, setViewerPlaybackMetrics] = useState<{
    sourceResolvedMs?: number;
    metadataMs?: number;
    firstFrameMs?: number;
    playingMs?: number;
    lastSeekMs?: number;
    proxyGenerationMs?: number;
  }>({});
  const [thumbnailFrameStatus, setThumbnailFrameStatus] = useState('');
  const [showSelectionDetails, setShowSelectionDetails] = useState(false);
  const [showShortcutHelp, setShowShortcutHelp] = useState(false);
  const [diagnosticNow, setDiagnosticNow] = useState(() => Date.now());
  const [selectionToolbarPosition, setSelectionToolbarPosition] =
    useState<SelectionToolbarPosition>(() => {
      const saved = localStorage.getItem('pimp-selection-toolbar-position');
      return saved === 'bottom-left' || saved === 'bottom-center'
        ? saved
        : 'bottom-right';
    });
  const [selectionToolbarActions, setSelectionToolbarActions] = useState<
    SelectionToolbarAction[]
  >(() => {
    try {
      const saved = JSON.parse(
        localStorage.getItem('pimp-selection-toolbar-actions') ?? 'null',
      );
      return Array.isArray(saved)
        ? defaultSelectionToolbarActions.filter((action) =>
            saved.includes(action),
          )
        : defaultSelectionToolbarActions;
    } catch {
      return defaultSelectionToolbarActions;
    }
  });
  const [contextMenu, setContextMenu] = useState<{
    mediaId: string;
    x: number;
    y: number;
  }>();
  const [projectName, setProjectName] = useState('');
  const [projectDirectory, setProjectDirectory] = useState('');
  const [backupProjectId, setBackupProjectId] = useState<string>();
  const [backupDestination, setBackupDestination] = useState('');
  const [backupJobs, setBackupJobs] = useState<BackupJob[]>([]);
  const [backupBusy, setBackupBusy] = useState(false);
  const catalogueSignature = useRef<string | undefined>(undefined);
  const metadataWrites = useRef(new Map<string, Promise<void>>());
  const pendingMetadataPatches = useRef(
    new Map<string, ProvisionalMetadataPatch>(),
  );
  const selectionAnchor = useRef<string | undefined>(undefined);
  const selectionUndo = useRef<
    Array<{ ids: string[]; anchor: string | undefined }>
  >([]);
  const selectionRedo = useRef<
    Array<{ ids: string[]; anchor: string | undefined }>
  >([]);
  const lastEscapeAt = useRef(0);
  const descriptionSaveGeneration = useRef(0);
  const selectionKeyRef = useRef('');
  const contentRef = useRef<HTMLElement>(null);
  const libraryPanelRef = useRef<HTMLDivElement>(null);
  const librarySearchRef = useRef<HTMLInputElement>(null);
  const viewerVideoRef = useRef<HTMLVideoElement>(null);
  const viewerPlaybackStartedAt = useRef(0);
  const viewerSeekStartedAt = useRef(0);
  const libraryScrollPosition = useRef(0);
  const projectInventoryGeneration = useRef(0);
  const thumbnailSourceGeneration = useRef(0);
  const preparedThumbnailVersion = useRef(0);
  const preparedThumbnailVersions = useRef(new Map<string, number>());
  const inventoryDateOverride = useRef<string | undefined>(undefined);
  const dateSelectionRef = useRef({ dateOverride, chosenDate });
  const activeInventoryPlanId = useRef<string | undefined>(undefined);
  const inventoryApprovalGeneration = useRef(0);
  const dateReplanIntent = useRef(0);
  const dateReplanQueue = useRef<Promise<void>>(Promise.resolve());
  const libraryCardRefs = useRef(new Map<string, HTMLElement>());
  const pendingNativeDrag = useRef<PendingNativeDrag | undefined>(undefined);
  const suppressedNativeClick = useRef<{ key: string } | undefined>(undefined);
  const [libraryLocateId, setLibraryLocateId] = useState<string>();
  const settingsRef = useRef(settings);
  const [savingSettings] = useState(
    () =>
      new SettingsWriteQueue<DeviceSettings>({
        write: (next) => desktopBridge.saveSettings(next),
        onCommitted: (saved, successMessage) => {
          setSettings(saved);
          setSettingsWriteFailed(false);
          setMessage(successMessage);
        },
        onFailed: (reason) => {
          const detail = errorText(reason);
          setSettingsWriteFailed(true);
          setError(`Settings were not saved: ${detail}`);
          void desktopBridge.logUiError(`Settings save failed: ${detail}`);
        },
        onPendingChanged: setSettingsWritePending,
      }),
  );
  const scanActive = useRef(false);
  const automaticUpdateCheckStarted = useRef(false);

  const configured = Boolean(settings.initializedCataloguePath);
  useEffect(() => {
    if (!native) return;
    let current = true;
    void desktopBridge
      .nativeDragThreshold()
      .then((threshold) => {
        if (current) setNativeDragThreshold(threshold);
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [native]);
  useEffect(() => {
    settingsRef.current = settings;
    savingSettings.setLatest(settings);
  }, [savingSettings, settings]);
  useEffect(() => {
    const unsubscribeQueue = previewWorkQueue.subscribe(setPreviewQueueStatus);
    const unsubscribeMemory = hoverPreviewRequestCache.subscribe(
      (usageBytes, readyEntries) => {
        setPreviewMemoryUsage(usageBytes);
        setPreviewMemoryCount(readyEntries);
      },
    );
    return () => {
      unsubscribeQueue();
      unsubscribeMemory();
    };
  }, []);
  useEffect(
    () => staticThumbnailWorkQueue.subscribe(setStaticThumbnailQueueStatus),
    [],
  );
  useEffect(() => {
    const configuredLimit = settings.previewRamCacheLimitBytes ?? 0;
    hoverPreviewRequestCache.configure(
      configuredLimit > 0 ? configuredLimit : 128 * 1024 * 1024,
    );
  }, [settings.previewRamCacheLimitBytes]);
  useEffect(() => {
    if (
      !native ||
      view !== 'Settings' ||
      !['Appearance', 'Diagnostics'].includes(settingsSection)
    )
      return;
    void desktopBridge
      .hoverPreviewCacheUsage()
      .then(setPreviewCacheUsage)
      .catch(() => undefined);
  }, [native, settingsSection, view]);
  useEffect(() => {
    if (view !== 'Settings' || settingsSection !== 'Diagnostics') return;
    const timer = window.setInterval(() => setDiagnosticNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [settingsSection, view]);
  useEffect(() => {
    const preservePendingSettings = (event: BeforeUnloadEvent) => {
      if (!savingSettings.hasUnsavedChanges()) return;
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', preservePendingSettings);
    return () =>
      window.removeEventListener('beforeunload', preservePendingSettings);
  }, [savingSettings]);
  const refresh = useCallback(async () => {
    setCatalogue(await desktopBridge.loadCatalogue());
  }, []);
  const runDiscovery = useCallback(
    async (full = false, announce = false) => {
      if (scanActive.current) return;
      scanActive.current = true;
      try {
        const result = await desktopBridge.rescan(full);
        if (result.registered > 0 || result.changed > 0) {
          await refresh();
          void desktopBridge
            .enrichMedia(announce)
            .then(refresh)
            .catch(() => undefined);
        }
        if (announce) {
          setMessage(
            `${full ? 'Full verification' : 'Scan'} complete: ${result.registered} registered, ${result.changed} changed, ${result.pending} pending, ${result.errors} errors.`,
          );
        }
        return result;
      } catch (reason) {
        if (announce) setError(errorText(reason));
        return undefined;
      } finally {
        scanActive.current = false;
      }
    },
    [refresh],
  );
  const receiveJobs = (incoming: IngestJob[]) =>
    setJobs(
      incoming.map((job) => ({
        ...job,
        files: job.files.map((file) => {
          const pending = pendingMetadataPatches.current.get(
            `${job.id}:${file.id}`,
          );
          return pending
            ? {
                ...file,
                metadata: { ...file.metadata, ...pending },
                pendingMetadata: { ...file.metadata, ...pending },
              }
            : file;
        }),
      })),
    );
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let stopProgress: (() => void) | undefined;
    let stopDrop: (() => void) | undefined;
    let stopJobs: (() => void) | undefined;
    let stopDiscovery: (() => void) | undefined;
    let stopCatalogue: (() => void) | undefined;
    Promise.all([
      desktopBridge.loadSettings(),
      desktopBridge.onProgress(setProgress),
      desktopBridge.onIngestJobsChanged(receiveJobs),
      desktopBridge.onFileDrop((paths) => {
        setInventory(undefined);
        setInventoryError('');
        setInventoryLoading(true);
        setSources((current) => [...new Set([...current, ...paths])]);
      }),
      desktopBridge.mediaToolsStatus(),
      desktopBridge.onDiscoveryProgress(setDiscoveryStatus),
      desktopBridge.onCatalogueChanged(() => {
        void refresh().catch((reason) => {
          void desktopBridge.logUiError(
            `Catalogue refresh event failed: ${errorText(reason)}`,
          );
        });
      }),
      desktopBridge.getDiscoveryStatus(),
      desktopBridge.appInfo(),
      desktopBridge.updaterConfiguration(),
    ])
      .then(
        async ([
          loaded,
          unlistenProgress,
          unlistenJobs,
          unlistenDrop,
          tools,
          unlistenDiscovery,
          unlistenCatalogue,
          loadedDiscoveryStatus,
          loadedApplicationInfo,
          loadedUpdaterConfiguration,
        ]) => {
          if (disposed) {
            unlistenProgress();
            unlistenJobs();
            unlistenDrop();
            unlistenDiscovery();
            unlistenCatalogue();
            return;
          }
          stopProgress = unlistenProgress;
          stopJobs = unlistenJobs;
          stopDrop = unlistenDrop;
          stopDiscovery = unlistenDiscovery;
          stopCatalogue = unlistenCatalogue;
          settingsRef.current = loaded;
          savingSettings.setLatest(loaded);
          setSettings(loaded);
          setDateOverride(
            loaded.ingestDateOrganization === 'original-file-dates'
              ? 'original-file-dates'
              : 'today',
          );
          setMediaTools(tools);
          setApplicationInfo(loadedApplicationInfo);
          setUpdaterConfiguration(loadedUpdaterConfiguration);
          setUpdateStatus(
            loadedUpdaterConfiguration.configured
              ? 'Not checked in this session'
              : 'Automatic updates not configured',
          );
          setDiscoveryStatus(loadedDiscoveryStatus);
          setView((loaded.startupPage as View | undefined) ?? 'Library');
          setJobs(await desktopBridge.initializeIngestJobs());
          setBackupJobs(await desktopBridge.initializeBackupJobs());
          if (loaded.initializedCataloguePath) {
            const before = await desktopBridge.catalogueSignature();
            let nextCatalogue = await desktopBridge.loadCatalogue();
            const after = await desktopBridge.catalogueSignature();
            if (after !== before)
              nextCatalogue = await desktopBridge.loadCatalogue();
            catalogueSignature.current = after;
            setCatalogue(nextCatalogue);
          }
        },
      )
      .catch((reason) => {
        const detail = errorText(reason);
        setError(`Startup failed: ${detail}`);
        void desktopBridge.logUiError(`Startup failed: ${detail}`);
      });
    return () => {
      disposed = true;
      stopProgress?.();
      stopJobs?.();
      stopDrop?.();
      stopDiscovery?.();
      stopCatalogue?.();
    };
  }, [native, refresh, savingSettings]);
  useEffect(() => {
    if (!native) return;
    let stop: (() => void) | undefined;
    void desktopBridge.onBackupJobsChanged(setBackupJobs).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop?.();
  }, [native]);

  useEffect(() => {
    const root = document.documentElement;
    const selected = settings.theme ?? 'graphite';
    const systemDark = window.matchMedia?.('(prefers-color-scheme: dark)');
    const apply = () => {
      root.dataset.theme =
        selected === 'system'
          ? (systemDark?.matches ?? true)
            ? 'graphite'
            : 'light'
          : selected;
      root.dataset.density = settings.density ?? 'comfortable';
    };
    apply();
    systemDark?.addEventListener('change', apply);
    return () => systemDark?.removeEventListener('change', apply);
  }, [settings.density, settings.theme]);

  useEffect(() => {
    dateSelectionRef.current = { dateOverride, chosenDate };
  }, [dateOverride, chosenDate]);

  useEffect(() => {
    if (!native || sources.length === 0) return;
    const approvalGeneration = ++inventoryApprovalGeneration.current;
    ++dateReplanIntent.current;
    activeInventoryPlanId.current = undefined;
    let current = true;
    const operationId = crypto.randomUUID();
    void Promise.resolve().then(() => {
      if (!current) return;
      setInventoryLoading(true);
      setInventory(undefined);
      setInventoryError('');
    });
    const selectedDate = dateSelectionRef.current;
    const previewDateOverride =
      selectedDate.dateOverride === 'choose'
        ? selectedDate.chosenDate || undefined
        : selectedDate.dateOverride;
    inventoryDateOverride.current = previewDateOverride;
    const request = sdGrabberMode
      ? desktopBridge.inventorySdCard(operationId, sources, previewDateOverride)
      : desktopBridge.inventorySources(
          operationId,
          sources,
          previewDateOverride,
        );
    void request
      .then((result) => {
        if (
          current &&
          approvalGeneration === inventoryApprovalGeneration.current
        ) {
          setInventory(result);
          activeInventoryPlanId.current = result.planId;
          setConflicts(result.conflicts ?? []);
          setInventoryError('');
          setAllowDuplicateCopies(false);
          setAllowUnverifiedReimport(false);
        }
      })
      .catch((reason) => {
        if (current && !errorText(reason).toLowerCase().includes('cancelled'))
          setInventoryError(errorText(reason));
      })
      .finally(() => {
        if (current) setInventoryLoading(false);
      });
    return () => {
      current = false;
      void desktopBridge.cancelInventory(operationId).catch(() => undefined);
    };
  }, [
    native,
    sources,
    sdGrabberMode,
    settings.sdGrabberExtensions,
    settings.sdGrabberSearchSubfolders,
  ]);

  useEffect(() => {
    if (!native || !inventory?.planId) return;
    const nextDateOverride =
      dateOverride === 'choose' ? chosenDate || undefined : dateOverride;
    if (!nextDateOverride || inventoryDateOverride.current === nextDateOverride)
      return;
    const intent = ++dateReplanIntent.current;
    const approvalGeneration = inventoryApprovalGeneration.current;
    inventoryDateOverride.current = nextDateOverride;
    setInventoryLoading(true);
    setInventoryError('');
    const run = dateReplanQueue.current
      .catch(() => undefined)
      .then(async () => {
        if (intent !== dateReplanIntent.current) return;
        const planId = activeInventoryPlanId.current ?? inventory.planId;
        try {
          const result = await desktopBridge.replanIngestDestinations(
            planId,
            nextDateOverride,
          );
          if (approvalGeneration !== inventoryApprovalGeneration.current)
            return;
          activeInventoryPlanId.current = result.planId;
          if (intent !== dateReplanIntent.current) return;
          setInventory(result);
          setConflicts(result.conflicts ?? []);
        } catch (reason) {
          if (intent !== dateReplanIntent.current) return;
          inventoryDateOverride.current = undefined;
          setInventoryError(errorText(reason));
        } finally {
          if (intent === dateReplanIntent.current) setInventoryLoading(false);
        }
      });
    dateReplanQueue.current = run;
  }, [native, inventory?.planId, dateOverride, chosenDate]);

  useEffect(() => {
    if (!native || !configured) return;
    let current = true;
    const refresh = () => {
      void desktopBridge
        .listBackgroundOperations()
        .then((operations) => {
          if (current) setBackgroundOperations(operations);
        })
        .catch(() => undefined);
    };
    refresh();
    const timer = window.setInterval(refresh, 500);
    return () => {
      current = false;
      window.clearInterval(timer);
    };
  }, [configured, native]);

  useEffect(() => {
    if (!native || !configured) return;
    let current = true;
    const refresh = () => {
      void desktopBridge
        .getResourceGovernorStatus()
        .then((status) => {
          if (current) setResourceStatus(status);
        })
        .catch(() => undefined);
    };
    refresh();
    const timer = window.setInterval(refresh, 1_000);
    return () => {
      current = false;
      window.clearInterval(timer);
    };
  }, [configured, native]);

  useEffect(() => {
    if (!native || !configured || !('__TAURI_INTERNALS__' in window)) return;
    const activeOperations = backgroundOperations.filter((operation) =>
      ['queued', 'running', 'cancelling'].includes(operation.state),
    );
    const activeIngest = jobs.find((job) =>
      ['queued', 'copying', 'verifying'].includes(job.state),
    );
    const primary = activeOperations[0];
    let title = 'Kairix PIMP';
    let status = ProgressBarStatus.None;
    let progress: number | undefined;
    if (primary) {
      const measured = primary.bytesTotal
        ? (primary.bytesCompleted / primary.bytesTotal) * 100
        : primary.totalItems
          ? (primary.completedItems / primary.totalItems) * 100
          : undefined;
      title = `Kairix PIMP — ${primary.title}${measured == null ? '' : ` ${Math.round(measured)}%`}`;
      status =
        measured == null
          ? ProgressBarStatus.Indeterminate
          : ProgressBarStatus.Normal;
      progress = measured == null ? undefined : Math.round(measured);
    } else if (activeIngest) {
      const measured = activeIngest.bytesTotal
        ? (activeIngest.bytesTransferred / activeIngest.bytesTotal) * 100
        : undefined;
      title = `Kairix PIMP — Ingest${measured == null ? '' : ` ${Math.round(measured)}%`}`;
      status =
        measured == null
          ? ProgressBarStatus.Indeterminate
          : ProgressBarStatus.Normal;
      progress = measured == null ? undefined : Math.round(measured);
    } else if (
      jobs.some((job) =>
        ['failed', 'interrupted', 'needs-attention'].includes(job.state),
      )
    ) {
      title = 'Kairix PIMP — Needs attention';
      status = ProgressBarStatus.Error;
    }
    const appWindow = getCurrentWindow();
    void appWindow.setTitle(title).catch(() => undefined);
    void appWindow.setProgressBar({ status, progress }).catch(() => undefined);
  }, [backgroundOperations, configured, jobs, native]);

  useEffect(() => {
    if (!native || !configured) return;
    void desktopBridge
      .enrichMedia(false)
      .catch((reason) =>
        desktopBridge.logUiError(
          `Startup thumbnail repair failed: ${errorText(reason)}`,
        ),
      );
  }, [configured, native]);

  useEffect(() => {
    if (
      !native ||
      view !== 'Ingest' ||
      !settings.sdGrabberEnabled ||
      !settings.sdGrabberAutoDetect
    )
      return;
    let current = true;
    const refreshVolumes = () => {
      void desktopBridge
        .listRemovableVolumes()
        .then((volumes) => {
          if (current) setRemovableVolumes(volumes);
        })
        .catch(() => undefined);
    };
    refreshVolumes();
    const timer = window.setInterval(refreshVolumes, 5_000);
    return () => {
      current = false;
      window.clearInterval(timer);
    };
  }, [native, settings.sdGrabberAutoDetect, settings.sdGrabberEnabled, view]);
  useEffect(() => {
    if (!native || !configured) return;
    let disposed = false;
    let checking = false;
    const checkForSharedMetadata = async () => {
      if (checking) return;
      checking = true;
      try {
        let observed = await desktopBridge.catalogueSignature();
        if (catalogueSignature.current === undefined) {
          catalogueSignature.current = observed;
          return;
        }
        if (observed === catalogueSignature.current) return;
        let nextCatalogue = await desktopBridge.loadCatalogue();
        const settled = await desktopBridge.catalogueSignature();
        if (settled !== observed) {
          observed = settled;
          nextCatalogue = await desktopBridge.loadCatalogue();
        }
        if (!disposed) {
          catalogueSignature.current = observed;
          setCatalogue(nextCatalogue);
        }
      } catch {
        // A partial Syncthing delivery is retried on the next poll.
      } finally {
        checking = false;
      }
    };
    const timer = window.setInterval(() => {
      void checkForSharedMetadata();
    }, 3_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [configured, native]);
  useEffect(() => {
    if (!native || !configured) return;
    const timer = window.setInterval(() => {
      void desktopBridge
        .refreshMediaAvailability()
        .then(async (changed) => {
          if (changed) setCatalogue(await desktopBridge.loadCatalogue());
        })
        .catch(() => undefined);
    }, 3_000);
    return () => window.clearInterval(timer);
  }, [configured, native]);
  useEffect(() => {
    if (!native || !configured || settings.backgroundProcessingPaused) return;
    let stabilityTimer: number | undefined;
    const startupTimer = window.setTimeout(() => {
      void runDiscovery().then((result) => {
        if (result?.pending) {
          stabilityTimer = window.setTimeout(() => {
            void runDiscovery();
          }, 16_000);
        }
      });
    }, 0);
    const timer = window.setInterval(() => {
      void runDiscovery();
    }, 30_000);
    return () => {
      window.clearInterval(timer);
      window.clearTimeout(startupTimer);
      if (stabilityTimer !== undefined) window.clearTimeout(stabilityTimer);
    };
  }, [configured, native, runDiscovery, settings.backgroundProcessingPaused]);

  useEffect(() => {
    const preventDefaultContextMenu = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable="true"]')) return;
      event.preventDefault();
    };
    document.addEventListener('contextmenu', preventDefaultContextMenu);
    return () =>
      document.removeEventListener('contextmenu', preventDefaultContextMenu);
  }, []);

  const displayedMedia = useMemo(
    () =>
      visibleMedia(
        catalogue.media,
        settings.showAttachedAudioSeparately ?? false,
      ),
    [catalogue.media, settings.showAttachedAudioSeparately],
  );
  useEffect(() => {
    if (!native) return;
    const generation = ++thumbnailSourceGeneration.current;
    const preparedVersionAtStart = preparedThumbnailVersion.current;
    const mediaIds = catalogue.media
      .filter((item) => item.thumbnailPath)
      .map((item) => item.id);
    void desktopBridge
      .loadThumbnailSources(mediaIds)
      .then((sources) => {
        setThumbnailSourceRequestCount((count) => count + 1);
        if (generation === thumbnailSourceGeneration.current) {
          setThumbnailSources((current) => {
            const next = { ...current };
            for (const mediaId of mediaIds) {
              if (
                (preparedThumbnailVersions.current.get(mediaId) ?? 0) >
                preparedVersionAtStart
              )
                continue;
              if (sources[mediaId]) next[mediaId] = sources[mediaId];
            }
            const retainedIds = new Set(catalogue.media.map((item) => item.id));
            for (const mediaId of Object.keys(next))
              if (!retainedIds.has(mediaId)) delete next[mediaId];
            return next;
          });
        }
      })
      .catch((reason) => {
        setThumbnailSourceRequestCount((count) => count + 1);
        if (generation === thumbnailSourceGeneration.current)
          void desktopBridge.logUiError(
            `Thumbnail source loading failed: ${errorText(reason)}`,
          );
      });
  }, [catalogue.media, native]);
  useEffect(() => {
    if (!native) return;
    const mediaIds = catalogue.media.map((item) => item.id);
    let current = true;
    void desktopBridge
      .loadHoverPreviewStatuses(mediaIds)
      .then((ready) => {
        if (current) setPreviewStoredIds(new Set(ready));
      })
      .catch((reason) => {
        if (current)
          void desktopBridge.logUiError(
            `Hover preview status loading failed: ${errorText(reason)}`,
          );
      });
    return () => {
      current = false;
    };
  }, [catalogue.media, native, previewCacheEpoch]);
  const receivePreparedPreview = useCallback((mediaId: string) => {
    setPreviewStoredIds((current) => {
      if (current.has(mediaId)) return current;
      const next = new Set(current);
      next.add(mediaId);
      return next;
    });
  }, []);
  const receivePreparedThumbnail = useCallback(
    (mediaId: string, source: string) => {
      const version = ++preparedThumbnailVersion.current;
      preparedThumbnailVersions.current.set(mediaId, version);
      setThumbnailSources((current) =>
        current[mediaId] === source
          ? current
          : { ...current, [mediaId]: source },
      );
    },
    [],
  );
  const selectedProject = catalogue.projects.find(
    (project) => project.id === selectedProjectId,
  );
  const selectedProjectSyncIssue = catalogue.issues?.find(
    (issue) =>
      issue.code === 'project-manifest-pending' &&
      issue.source === selectedProjectId,
  );
  const projectMedia = useMemo(() => {
    if (!selectedProjectId) return [];
    return displayedMedia.filter((item) =>
      logicalMediaMatches(item, projectMediaQuery, catalogue),
    );
  }, [catalogue, displayedMedia, projectMediaQuery, selectedProjectId]);
  const linkedProjectMedia = useMemo(
    () =>
      projectMedia.filter((item) =>
        logicalProjectIds(item, catalogue.media).includes(
          selectedProjectId ?? '',
        ),
      ),
    [catalogue.media, projectMedia, selectedProjectId],
  );
  const unlinkedProjectMedia = useMemo(
    () =>
      projectMedia.filter(
        (item) =>
          !logicalProjectIds(item, catalogue.media).includes(
            selectedProjectId ?? '',
          ),
      ),
    [catalogue.media, projectMedia, selectedProjectId],
  );
  const availableProjectMedia = useMemo(() => {
    const query = projectAvailableQuery.trim().toLocaleLowerCase();
    return unlinkedProjectMedia.filter((item) => {
      const tags = logicalTags(item, catalogue.media);
      return (
        (!query || logicalMediaMatches(item, query, catalogue)) &&
        projectAvailableTags.every((tag) => tags.includes(tag))
      );
    });
  }, [
    catalogue,
    projectAvailableQuery,
    projectAvailableTags,
    unlinkedProjectMedia,
  ]);
  const availableProjectPageSize = 50;
  const availableProjectPageCount = Math.max(
    1,
    Math.ceil(availableProjectMedia.length / availableProjectPageSize),
  );
  const pagedAvailableProjectMedia = availableProjectMedia.slice(
    (projectAvailablePage - 1) * availableProjectPageSize,
    projectAvailablePage * availableProjectPageSize,
  );
  const premiereSummary = useMemo(() => {
    const index = projectInventory?.premiereIndex;
    if (!index) return undefined;
    const current = index.mediaReferences.filter(
      (reference) =>
        reference.evidence === 'imported-into-saved-project' ||
        reference.evidence === 'used-in-saved-timeline',
    );
    const used = current.filter(
      (reference) => reference.evidence === 'used-in-saved-timeline',
    );
    const timelineCoverageKnown = !index.issues.some((issue) =>
      issue.includes(
        'Timeline track-item media relationships were not established',
      ),
    );
    const logicalCount = (references: typeof current) =>
      new Set(
        references.map(
          (reference) => reference.mediaId ?? `path:${reference.path}`,
        ),
      ).size;
    return {
      imported: logicalCount(current),
      used: timelineCoverageKnown ? logicalCount(used) : undefined,
      unused: timelineCoverageKnown
        ? logicalCount(
            current.filter(
              (reference) =>
                reference.evidence === 'imported-into-saved-project',
            ),
          )
        : undefined,
      unresolved: logicalCount(
        index.mediaReferences.filter((reference) => !reference.mediaId),
      ),
      historical: logicalCount(
        index.mediaReferences.filter(
          (reference) =>
            reference.evidence === 'historical-reference-supported',
        ),
      ),
      unclassified: logicalCount(
        index.mediaReferences.filter(
          (reference) =>
            reference.evidence === 'unclassified-project-reference' ||
            reference.evidence === 'previously-referenced-in-saved-project',
        ),
      ),
    };
  }, [projectInventory]);
  const folders = useMemo(() => {
    const summaries = new Map<
      string,
      { name: string; count: number; newest: string }
    >();
    for (const item of displayedMedia) {
      const name =
        item.relativePath.split('/').slice(0, -1).join('/') ||
        'Master Bin root';
      const current = summaries.get(name) ?? { name, count: 0, newest: '' };
      current.count += 1;
      const recordingDate = item.captureDate ?? item.ingestDate ?? '';
      if (recordingDate > current.newest) current.newest = recordingDate;
      summaries.set(name, current);
    }
    const result = [...summaries.values()];
    const byName = (left: { name: string }, right: { name: string }) =>
      left.name.localeCompare(right.name, undefined, { sensitivity: 'base' });
    const sort = settings.libraryFolderSort ?? 'newest';
    if (sort === 'custom') {
      const positions = new Map(
        (settings.libraryFolderOrder ?? []).map((name, index) => [name, index]),
      );
      return result.sort((left, right) => {
        const leftPosition =
          positions.get(left.name) ?? Number.MAX_SAFE_INTEGER;
        const rightPosition =
          positions.get(right.name) ?? Number.MAX_SAFE_INTEGER;
        return leftPosition - rightPosition || byName(left, right);
      });
    }
    return result.sort((left, right) => {
      if (sort === 'oldest')
        return left.newest.localeCompare(right.newest) || byName(left, right);
      if (sort === 'name-asc') return byName(left, right);
      if (sort === 'name-desc') return byName(right, left);
      if (sort === 'most-recordings')
        return right.count - left.count || byName(left, right);
      if (sort === 'fewest-recordings')
        return left.count - right.count || byName(left, right);
      return right.newest.localeCompare(left.newest) || byName(left, right);
    });
  }, [displayedMedia, settings.libraryFolderOrder, settings.libraryFolderSort]);
  const filtered = useMemo(
    () =>
      displayedMedia.filter((item) => {
        const itemFolder =
          item.relativePath.split('/').slice(0, -1).join('/') ||
          'Master Bin root';
        return (
          (includeUnavailable || item.available) &&
          (folder === 'All media' || folder === itemFolder) &&
          tagFilters.every((filter) =>
            logicalTags(item, catalogue.media).some(
              (tag) => tag.toLocaleLowerCase() === filter.toLocaleLowerCase(),
            ),
          ) &&
          logicalMediaMatches(item, query, catalogue)
        );
      }),
    [catalogue, displayedMedia, folder, includeUnavailable, query, tagFilters],
  );
  const replaceSelection = useCallback(
    (ids: string[], anchor: string | undefined, recordHistory = true) => {
      const known = new Set(catalogue.media.map((item) => item.id));
      const next = [...new Set(ids.filter((id) => known.has(id)))];
      if (
        next.length === selected.length &&
        next.every((id, index) => id === selected[index]) &&
        anchor === selectionAnchor.current
      )
        return;
      if (recordHistory) {
        selectionUndo.current.push({
          ids: [...selected],
          anchor: selectionAnchor.current,
        });
        if (selectionUndo.current.length > 50) selectionUndo.current.shift();
        selectionRedo.current = [];
      }
      selectionAnchor.current = anchor;
      setSelected(next);
    },
    [catalogue.media, selected],
  );
  const undoSelection = useCallback(() => {
    const previous = selectionUndo.current.pop();
    if (!previous) return;
    selectionRedo.current.push({
      ids: [...selected],
      anchor: selectionAnchor.current,
    });
    replaceSelection(previous.ids, previous.anchor, false);
  }, [replaceSelection, selected]);
  const redoSelection = useCallback(() => {
    const next = selectionRedo.current.pop();
    if (!next) return;
    selectionUndo.current.push({
      ids: [...selected],
      anchor: selectionAnchor.current,
    });
    replaceSelection(next.ids, next.anchor, false);
  }, [replaceSelection, selected]);
  useEffect(() => {
    const known = new Set(catalogue.media.map((item) => item.id));
    selectionUndo.current = selectionUndo.current
      .map((snapshot) => ({
        ids: snapshot.ids.filter((id) => known.has(id)),
        anchor:
          snapshot.anchor && known.has(snapshot.anchor)
            ? snapshot.anchor
            : undefined,
      }))
      .slice(-50);
    selectionRedo.current = selectionRedo.current
      .map((snapshot) => ({
        ids: snapshot.ids.filter((id) => known.has(id)),
        anchor:
          snapshot.anchor && known.has(snapshot.anchor)
            ? snapshot.anchor
            : undefined,
      }))
      .slice(-50);
  }, [catalogue.media]);
  const viewerRecording = viewerMediaId
    ? catalogue.media.find((item) => item.id === viewerMediaId)
    : undefined;
  const viewerIndex = viewerRecording
    ? filtered.findIndex((item) => item.id === viewerRecording.id)
    : -1;
  const viewerAssociatedMedia = viewerRecording
    ? associatedMedia(viewerRecording, catalogue.media)
    : [];
  useEffect(() => {
    if (!viewerMediaId || !viewerRecording?.available || !native) return;
    viewerPlaybackStartedAt.current = performance.now();
    let current = true;
    void Promise.resolve().then(() => {
      if (!current) return;
      setViewerPlaybackMetrics({});
      setViewerSource(undefined);
      setViewerError('');
      setViewerProxyBusy(false);
    });
    void desktopBridge
      .resolveMediaPlaybackSource(viewerMediaId)
      .then((source) => {
        if (current) {
          setViewerSource(source);
          setViewerPlaybackMetrics((metrics) => ({
            ...metrics,
            sourceResolvedMs: Math.round(
              performance.now() - viewerPlaybackStartedAt.current,
            ),
          }));
        }
      })
      .catch((reason) => {
        if (current) setViewerError(errorText(reason));
      });
    return () => {
      current = false;
    };
  }, [native, viewerMediaId, viewerRecording?.available]);
  useEffect(() => {
    if (!native || !viewerMediaId) return;
    const mediaId = viewerMediaId;
    return () => {
      void desktopBridge.releasePlaybackProxy(mediaId).catch(() => undefined);
    };
  }, [native, viewerMediaId]);
  const openClipViewer = (mediaId: string) => {
    libraryScrollPosition.current = libraryPanelRef.current?.scrollTop ?? 0;
    replaceSelection([mediaId], mediaId);
    setViewerMediaId(mediaId);
  };
  const closeClipViewer = () => {
    setViewerMediaId(undefined);
    const top = libraryScrollPosition.current;
    window.requestAnimationFrame(() =>
      libraryPanelRef.current?.scrollTo?.({ top }),
    );
  };
  const navigateClipViewer = (offset: -1 | 1) => {
    const next = filtered[viewerIndex + offset];
    if (!next) return;
    replaceSelection([next.id], next.id);
    setViewerMediaId(next.id);
  };
  const saveCurrentViewerFrame = async () => {
    if (!viewerRecording || !viewerVideoRef.current) return;
    setThumbnailFrameStatus('Saving thumbnail…');
    try {
      const source = await desktopBridge.setMediaThumbnailFrame(
        viewerRecording.id,
        viewerVideoRef.current.currentTime,
      );
      setThumbnailSources((current) => ({
        ...current,
        [viewerRecording.id]: source,
      }));
      setPreviewCacheEpoch((current) => current + 1);
      setThumbnailFrameStatus(
        'Thumbnail saved to the shared catalogue. Other devices can reuse it after synchronization.',
      );
    } catch (reason) {
      setThumbnailFrameStatus(`Thumbnail failed: ${errorText(reason)}`);
    }
  };
  const createPlaybackProxy = async () => {
    if (!viewerRecording) return;
    setViewerProxyBusy(true);
    const proxyStartedAt = performance.now();
    try {
      setViewerSource(
        await desktopBridge.preparePlaybackProxy(viewerRecording.id),
      );
      viewerPlaybackStartedAt.current = performance.now();
      setViewerPlaybackMetrics({
        sourceResolvedMs: 0,
        proxyGenerationMs: Math.round(performance.now() - proxyStartedAt),
      });
      setViewerError('');
      setMessage(
        'Compatible preview ready in the device-local cache. The original recording was unchanged.',
      );
    } catch (reason) {
      setViewerError(`Compatible preview failed: ${errorText(reason)}`);
    } finally {
      setViewerProxyBusy(false);
    }
  };
  useEffect(() => {
    if (view !== 'Library' || !libraryLocateId) return;
    const card = libraryCardRefs.current.get(libraryLocateId);
    if (!card) {
      setError(
        'The selected recording is not currently available in this Library view.',
      );
      setLibraryLocateId(undefined);
      return;
    }
    const frame = window.requestAnimationFrame(() => {
      card.scrollIntoView?.({ block: 'center', behavior: 'smooth' });
      card.focus({ preventScroll: true });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [filtered, libraryLocateId, view]);
  const knownTags = useMemo(
    () =>
      [
        ...new Set([
          ...catalogue.media.flatMap((item) => item.tags),
          ...catalogue.projects.flatMap((project) => project.tags),
        ]),
      ].sort(),
    [catalogue.media, catalogue.projects],
  );
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (view !== 'Library') return;
      const target = event.target as HTMLElement | null;
      const editing = keyboardInputOwner(target);
      const key = event.key.toLowerCase();
      if (editing) {
        if (event.key === 'Escape' && showShortcutHelp) {
          event.preventDefault();
          setShowShortcutHelp(false);
        }
        return;
      }
      if (viewerMediaId && (key === 'k' || event.key === ' ')) {
        event.preventDefault();
        const video = viewerVideoRef.current;
        if (video?.paused) void video.play();
        else video?.pause();
        return;
      }
      if (event.altKey && event.key === 'ArrowLeft') {
        event.preventDefault();
        if (viewerMediaId) closeClipViewer();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && key === 'f') {
        event.preventDefault();
        librarySearchRef.current?.focus();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && key === 'z') {
        event.preventDefault();
        if (event.shiftKey) redoSelection();
        else undoSelection();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && key === 'y') {
        event.preventDefault();
        redoSelection();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && key === 'a') {
        event.preventDefault();
        replaceSelection(
          filtered.map((item) => item.id),
          filtered.at(-1)?.id,
        );
        return;
      }
      if (event.key === '?' && !event.ctrlKey && !event.metaKey) {
        event.preventDefault();
        setShowShortcutHelp(true);
        return;
      }
      if (event.key === 'Escape') {
        if (showShortcutHelp) {
          event.preventDefault();
          setShowShortcutHelp(false);
          return;
        }
        if (contextMenu) return;
        if (viewerMediaId) {
          event.preventDefault();
          closeClipViewer();
          return;
        }
        const now = Date.now();
        if (now - lastEscapeAt.current < 600 && selected.length) {
          event.preventDefault();
          replaceSelection([], undefined);
        }
        lastEscapeAt.current = now;
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [
    contextMenu,
    filtered,
    redoSelection,
    replaceSelection,
    selected.length,
    showShortcutHelp,
    undoSelection,
    view,
    viewerMediaId,
  ]);
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(undefined);
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    window.addEventListener('click', close);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      window.removeEventListener('click', close);
      window.removeEventListener('keydown', closeOnEscape);
    };
  }, [contextMenu]);
  const saveConfiguration = async () => {
    setError('');
    setBusy(true);
    await savingSettings.save(
      settingsRef.current,
      'Configuration saved. Library indexing will continue in the background.',
    );
    setBusy(false);
  };
  const persistPreference = (patch: Partial<DeviceSettings>) => {
    const next = { ...settingsRef.current, ...patch };
    settingsRef.current = next;
    savingSettings.setLatest(next);
    setSettings(next);
    if (!configured) return;
    setError('');
    void savingSettings.save(next, 'Preference saved.');
  };
  const setFolderOrdering = (libraryFolderSort: FolderSort) => {
    persistPreference({
      libraryFolderSort,
      ...(libraryFolderSort === 'custom'
        ? { libraryFolderOrder: folders.map(({ name }) => name) }
        : {}),
    });
  };
  const moveFolder = (name: string, offset: -1 | 1) => {
    const order = folders.map((summary) => summary.name);
    const index = order.indexOf(name);
    const destination = index + offset;
    if (index < 0 || destination < 0 || destination >= order.length) return;
    [order[index], order[destination]] = [order[destination], order[index]];
    persistPreference({ libraryFolderOrder: order });
  };
  const retrySettingsSave = async () => {
    setError('');
    await savingSettings.retry('Settings saved after retry.');
  };
  const reloadInterface = async () => {
    const saved = await savingSettings.flush();
    if (saved === false) {
      setError(
        'The interface was not reloaded because device settings are still unsaved. Retry the settings save first.',
      );
      return;
    }
    window.location.reload();
  };
  const chooseSettingFolder = async (
    key:
      | 'masterBinPath'
      | 'workingProjectsPath'
      | 'sharedCataloguePath'
      | 'thumbnailCachePath',
  ) => {
    const path = await desktopBridge.chooseFolder();
    if (path) setSettings((current) => ({ ...current, [key]: path }));
  };
  const addFiles = async () => {
    const chosen = await desktopBridge.chooseFiles();
    if (chosen.length) {
      setSdGrabberMode(false);
      setInventory(undefined);
      setInventoryError('');
      setInventoryLoading(true);
      setSources((current) => [...new Set([...current, ...chosen])]);
    }
  };
  const addFolder = async () => {
    const path = await desktopBridge.chooseFolder();
    if (path) {
      setSdGrabberMode(false);
      setInventory(undefined);
      setInventoryError('');
      setInventoryLoading(true);
      setSources((current) => [...new Set([...current, path])]);
    }
  };
  const scanSdSource = (path: string, identity?: string) => {
    if (
      identity &&
      settings.sdGrabberRememberDevices &&
      !settings.sdGrabberRecognizedDeviceIds.includes(identity)
    ) {
      persistPreference({
        sdGrabberRecognizedDeviceIds: [
          ...settings.sdGrabberRecognizedDeviceIds,
          identity,
        ],
      });
    }
    setSdGrabberMode(true);
    setShowSdGrabber(true);
    setInventory(undefined);
    setInventoryError('');
    setInventoryLoading(true);
    setSources([path]);
  };
  const chooseSdSource = async () => {
    const path = await desktopBridge.chooseFolder();
    if (path) scanSdSource(path);
  };
  const refreshRemovableVolumes = async () => {
    try {
      setRemovableVolumes(await desktopBridge.listRemovableVolumes());
    } catch (reason) {
      setInventoryError(errorText(reason));
    }
  };
  const ingest = async (renamedDestinations: Record<string, string> = {}) => {
    if (!sources.length) return;
    if (!inventory?.planId) {
      setError('Scan and review the selected source before starting ingest.');
      return;
    }
    setStartingIngest(true);
    setError('');
    setMessage('');
    const jobId = crypto.randomUUID();
    try {
      const job = await desktopBridge.startIngest({
        jobId,
        sources,
        dateOverride: dateOverride === 'choose' ? chosenDate : dateOverride,
        renamedDestinations,
        batchTags: ingestBatchTags,
        approvedPlanId: inventory.planId,
        allowDuplicateCopies,
        allowUnverifiedReimport,
      });
      setJobs((current) => [...current, job]);
      setSources([]);
      setInventory(undefined);
      setInventoryLoading(false);
      setInventoryError('');
      setConflicts([]);
      setRenames({});
      setIngestBatchTags([]);
      setAllowDuplicateCopies(false);
      setAllowUnverifiedReimport(false);
      setSdGrabberMode(false);
      setJobsOpen(true);
      setMessage(
        `Ingest queued with ${job.files.length} file${job.files.length === 1 ? '' : 's'}. You can keep browsing and edit metadata now.`,
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setStartingIngest(false);
    }
  };
  const reviewRenamedDestinations = async () => {
    if (!inventory?.planId) return;
    const reviewed = Object.fromEntries(
      Object.entries(renames).filter(([, value]) => value.trim()),
    );
    setStartingIngest(true);
    setError('');
    try {
      const next = await desktopBridge.reapproveIngestDestinations(
        inventory.planId,
        reviewed,
      );
      setInventory(next);
      setConflicts([]);
      setRenames({});
      setMessage(
        'Renamed destinations were validated and saved in a new approved ingest plan. Review the paths, then start ingest.',
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setStartingIngest(false);
    }
  };
  const updateJobFile = (
    jobId: string,
    fileId: string,
    update: Partial<IngestJobFile['metadata']>,
  ) => {
    const job = jobs.find((candidate) => candidate.id === jobId);
    const file = job?.files.find((candidate) => candidate.id === fileId);
    if (!file) return;
    const metadata = { ...file.metadata, ...update };
    setJobs((current) =>
      current.map((candidate) =>
        candidate.id !== jobId
          ? candidate
          : {
              ...candidate,
              files: candidate.files.map((candidateFile) =>
                candidateFile.id === fileId
                  ? { ...candidateFile, metadata, pendingMetadata: metadata }
                  : candidateFile,
              ),
            },
      ),
    );
    const key = `${jobId}:${fileId}`;
    pendingMetadataPatches.current.set(key, {
      ...pendingMetadataPatches.current.get(key),
      ...update,
    });
    const previous = metadataWrites.current.get(key) ?? Promise.resolve();
    const next = previous
      .catch(() => undefined)
      .then(() => desktopBridge.updateIngestFileMetadata(jobId, fileId, update))
      .then(() => {
        if (metadataWrites.current.get(key) === next) {
          pendingMetadataPatches.current.delete(key);
          setJobs((current) =>
            current.map((candidate) =>
              candidate.id !== jobId
                ? candidate
                : {
                    ...candidate,
                    files: candidate.files.map((candidateFile) =>
                      candidateFile.id === fileId
                        ? { ...candidateFile, pendingMetadata: undefined }
                        : candidateFile,
                    ),
                  },
            ),
          );
        }
      })
      .catch((reason) => setError(errorText(reason)));
    metadataWrites.current.set(key, next);
    void next.finally(() => {
      if (metadataWrites.current.get(key) === next)
        metadataWrites.current.delete(key);
    });
  };
  const updateSelection = async (next: {
    addTags?: string[];
    addProjectIds?: string[];
    removeTags?: string[];
    removeProjectIds?: string[];
    notes?: string;
    favourite?: boolean;
  }) => {
    if (!selected.length) return;
    try {
      const mediaIds = expandLogicalMediaIds(selected, catalogue.media);
      const updated = await desktopBridge.updateMedia({ mediaIds, ...next });
      setCatalogue(updated);
      const pendingManifests =
        updated.issues?.filter(
          (issue) => issue.code === 'project-manifest-pending',
        ).length ?? 0;
      if (pendingManifests)
        setMessage(
          `Catalogue changes saved. ${pendingManifests} project manifest update${pendingManifests === 1 ? ' is' : 's are'} pending automatic retry.`,
        );
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const updateRecording = async (
    mediaId: string,
    next: { removeTags?: string[] },
  ) => {
    try {
      const mediaIds = expandLogicalMediaIds([mediaId], catalogue.media);
      setCatalogue(await desktopBridge.updateMedia({ mediaIds, ...next }));
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const selectRecording = (
    mediaId: string,
    options: { toggle: boolean; range: boolean },
  ) => {
    if (options.range && selectionAnchor.current) {
      const start = filtered.findIndex(
        (item) => item.id === selectionAnchor.current,
      );
      const end = filtered.findIndex((item) => item.id === mediaId);
      if (start >= 0 && end >= 0) {
        const [from, to] = start < end ? [start, end] : [end, start];
        const range = filtered.slice(from, to + 1).map((item) => item.id);
        replaceSelection(
          options.toggle ? [...new Set([...selected, ...range])] : range,
          selectionAnchor.current,
        );
        return;
      }
    }
    replaceSelection(
      !options.toggle
        ? [mediaId]
        : selected.includes(mediaId)
          ? selected.filter((id) => id !== mediaId)
          : [...selected, mediaId],
      mediaId,
    );
  };
  const selectedRecording =
    selected.length === 1
      ? catalogue.media.find((item) => item.id === selected[0])
      : undefined;
  const selectionKey = selected.join('\u0000');
  useEffect(() => {
    selectionKeyRef.current = selectionKey;
  }, [selectionKey]);
  const descriptionValue =
    descriptionDraft.selectionKey === selectionKey
      ? descriptionDraft.value
      : (selectedRecording?.notes ?? '');
  const visibleDescriptionStatus =
    descriptionSaveStatus.selectionKey === selectionKey
      ? descriptionSaveStatus
      : { selectionKey, state: 'idle' as const };
  const selectedAssociatedMedia = selectedRecording
    ? associatedMedia(selectedRecording, catalogue.media)
    : [];
  const selectedPhysicalIds = expandLogicalMediaIds(selected, catalogue.media);
  const selectedTagState = (tag: string) => {
    const selectedItems = selectedPhysicalIds
      .map((id) => catalogue.media.find((item) => item.id === id))
      .filter((item): item is NonNullable<typeof item> => Boolean(item));
    const count = selectedItems.filter((item) =>
      item.tags.some(
        (candidate) =>
          candidate.toLocaleLowerCase() === tag.toLocaleLowerCase(),
      ),
    ).length;
    return count === 0
      ? 'Present on none selected'
      : count === selectedItems.length
        ? 'Present on all selected'
        : 'Present on some selected';
  };
  const saveSelectionDescription = async () => {
    if (!selectedPhysicalIds.length) return;
    const requestId = ++descriptionSaveGeneration.current;
    const requestSelectionKey = selectionKey;
    const mediaIds = [...selectedPhysicalIds];
    const notes = descriptionValue;
    const currentValues = mediaIds
      .map((id) => catalogue.media.find((item) => item.id === id)?.notes)
      .filter((value): value is string => value !== undefined);
    setDescriptionSaveStatus({
      selectionKey: requestSelectionKey,
      state: 'saving',
    });
    if (
      currentValues.length === mediaIds.length &&
      currentValues.every((value) => value === notes)
    ) {
      setDescriptionSaveStatus({
        selectionKey: requestSelectionKey,
        state: 'saved',
      });
      return;
    }
    try {
      const updated = await desktopBridge.updateMedia({ mediaIds, notes });
      if (requestId !== descriptionSaveGeneration.current) return;
      setCatalogue(updated);
      if (selectionKeyRef.current === requestSelectionKey)
        setDescriptionSaveStatus({
          selectionKey: requestSelectionKey,
          state: 'saved',
        });
    } catch (reason) {
      if (
        requestId === descriptionSaveGeneration.current &&
        selectionKeyRef.current === requestSelectionKey
      )
        setDescriptionSaveStatus({
          selectionKey: requestSelectionKey,
          state: 'failed',
          detail: errorText(reason),
        });
    }
  };
  const contextRecording = contextMenu
    ? catalogue.media.find((item) => item.id === contextMenu.mediaId)
    : undefined;
  const runSelectedFileAction = async (
    action: 'reveal' | 'folder' | 'copy',
    recording = selectedRecording,
  ) => {
    if (!recording?.available) return;
    setError('');
    try {
      if (action === 'reveal') {
        await desktopBridge.revealMediaInExplorer(recording.id);
        setMessage('Recording revealed in Windows Explorer.');
      } else if (action === 'folder') {
        await desktopBridge.openMediaFolder(recording.id);
        setMessage('Containing date folder opened in Windows Explorer.');
      } else {
        const path = await desktopBridge.resolveMediaPath(recording.id);
        await navigator.clipboard.writeText(path);
        setMessage('Absolute recording path copied to the clipboard.');
      }
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const beginNativeDrag = (
    event: ReactPointerEvent<HTMLElement>,
    drag: Omit<PendingNativeDrag, 'pointerId' | 'origin'>,
  ) => {
    if (!native || event.button !== 0 || (event.buttons & 1) === 0) return;
    const target = event.target as HTMLElement;
    const nestedControl = target.closest(
      'input, textarea, select, a, button, [contenteditable="true"], [data-native-drag-ignore="true"]',
    );
    if (nestedControl && nestedControl !== event.currentTarget) return;
    pendingNativeDrag.current = {
      ...drag,
      pointerId: event.pointerId,
      origin: { x: event.clientX, y: event.clientY },
    };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const cancelPendingNativeDrag = (event: ReactPointerEvent<HTMLElement>) => {
    if (pendingNativeDrag.current?.pointerId === event.pointerId)
      pendingNativeDrag.current = undefined;
  };
  const continueNativeDrag = (event: ReactPointerEvent<HTMLElement>) => {
    const pending = pendingNativeDrag.current;
    if (!pending || pending.pointerId !== event.pointerId) return;
    if ((event.buttons & 1) === 0) {
      pendingNativeDrag.current = undefined;
      return;
    }
    if (
      !nativeDragMovementExceeded(
        pending.origin,
        { x: event.clientX, y: event.clientY },
        nativeDragThreshold,
      )
    )
      return;
    pendingNativeDrag.current = undefined;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    event.preventDefault();
    event.stopPropagation();
    const suppressedClick = { key: pending.key };
    suppressedNativeClick.current = suppressedClick;
    window.setTimeout(() => {
      if (suppressedNativeClick.current === suppressedClick)
        suppressedNativeClick.current = undefined;
    }, 750);
    setError('');
    void pending
      .run()
      .then((outcome) => {
        if (outcome === 'dropped') {
          setMessage(pending.droppedMessage);
        } else if (outcome === 'cancelled') {
          setMessage(
            'Windows drag cancelled. Original Library items are unchanged.',
          );
        } else {
          setMessage(
            'The destination did not accept the copy-only Windows drag. Original Library items are unchanged.',
          );
        }
      })
      .catch((reason) => setError(errorText(reason)));
  };
  const suppressClickAfterNativeDrag = (key: string) => {
    const suppressed = suppressedNativeClick.current;
    if (!suppressed || suppressed.key !== key) return false;
    suppressedNativeClick.current = undefined;
    return true;
  };
  const mediaDrag = (recording: Catalogue['media'][number]) => {
    const mediaIds = selected.includes(recording.id)
      ? selected
      : [recording.id];
    return {
      key: `media:${recording.id}`,
      run: () => desktopBridge.startNativeMediaDrag(mediaIds),
      droppedMessage: `${mediaIds.length} original recording${mediaIds.length === 1 ? '' : 's'} was accepted by the destination as a copy-only Windows drag. Confirm the destination completed its own copy or import.`,
    };
  };
  const folderDrag = (relativePath: string) => ({
    key: `folder:${relativePath}`,
    run: () => desktopBridge.startNativeFolderDrag(relativePath),
    droppedMessage:
      'The original Library folder was accepted by the destination as a copy-only Windows drag. Confirm the destination completed its own copy or import.',
  });
  const copySelectedPaths = async () => {
    const recordings = selectedPhysicalIds
      .map((id) => catalogue.media.find((item) => item.id === id))
      .filter((item): item is NonNullable<typeof item> => Boolean(item));
    const available = recordings.filter((item) => item.available);
    if (!available.length) return;
    setError('');
    try {
      const paths = await Promise.all(
        available.map((item) => desktopBridge.resolveMediaPath(item.id)),
      );
      await navigator.clipboard.writeText(paths.join('\r\n'));
      const skipped = recordings.length - available.length;
      setMessage(
        `${paths.length} file path${paths.length === 1 ? '' : 's'} copied as text${skipped ? `; ${skipped} unavailable item${skipped === 1 ? '' : 's'} skipped` : ''}.`,
      );
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const exportPremiereManifest = async () => {
    if (!selectedPhysicalIds.length) return;
    setError('');
    try {
      const destination =
        await desktopBridge.choosePremiereManifestDestination();
      if (!destination) return;
      const count = await desktopBridge.exportPremiereManifest(
        selectedPhysicalIds,
        destination,
      );
      setMessage(
        `${count} verified Master Bin file${count === 1 ? '' : 's'} exported for the Kairix PIMP Premiere Companion.`,
      );
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const reEvaluateAudioPairing = async () => {
    setBusy(true);
    setError('');
    try {
      setCatalogue(await desktopBridge.loadCatalogue());
      setMessage(
        'External-audio pairing re-evaluated from the current catalogue.',
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const doRescan = async () => {
    setBusy(true);
    setError('');
    try {
      await runDiscovery(false, true);
    } finally {
      setBusy(false);
    }
  };
  const doFullRescan = async () => {
    setBusy(true);
    setError('');
    try {
      await runDiscovery(true, true);
    } finally {
      setBusy(false);
    }
  };
  const openSupport = async (
    destination: 'github' | 'releases' | 'test-builds' | 'website' | 'coffee',
  ) => {
    try {
      await desktopBridge.openSupportLink(destination);
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const copyDiagnosticSummary = async () => {
    const jobCounts = jobs.reduce<Record<string, number>>((counts, job) => {
      counts[job.state] = (counts[job.state] ?? 0) + 1;
      return counts;
    }, {});
    await navigator.clipboard.writeText(
      [
        `${applicationInfo?.edition ?? 'Kairix PIMP Light'} ${applicationInfo?.version ?? 'unknown'}`,
        `Build: ${applicationInfo?.buildCommit ?? 'unknown'}`,
        `Classification: ${applicationInfo?.releaseClassification ?? 'Development'}`,
        `Update channel: ${updaterConfiguration?.channel ?? 'Development'}`,
        `Device: ${settings.deviceId}`,
        `Catalogue items: ${catalogue.media.length}`,
        `Last scan: ${discoveryStatus.lastCompletedAt ?? 'never'}`,
        `Scan pending/errors: ${discoveryStatus.pending}/${discoveryStatus.errors}`,
        `Jobs: ${JSON.stringify(jobCounts)}`,
        `FFmpeg: ${mediaTools?.ffmpegAvailable ? 'available' : 'unavailable'}`,
        `FFprobe: ${mediaTools?.ffprobeAvailable ? 'available' : 'unavailable'}`,
        `Thumbnail source batches: ${thumbnailSourceRequestCount}`,
        `Static thumbnail queue: ${staticThumbnailQueueStatus.active ? 1 : 0} active, ${staticThumbnailQueueStatus.queued} queued`,
        `Hover preview queue: ${previewQueueStatus.active ? 1 : 0} active, ${previewQueueStatus.queued} queued`,
        `Resource profile: ${resourceStatus?.profile ?? settings.resourceProfile}`,
        `Volume admission: ${resourceStatus?.volumes.reduce((total, volume) => total + volume.activeReaders, 0) ?? 0} readers, ${resourceStatus?.volumes.reduce((total, volume) => total + volume.activeWriters, 0) ?? 0} writers, ${resourceStatus?.queuedWorkers ?? 0} waiting`,
        `Volume bytes: ${resourceStatus?.volumes.reduce((total, volume) => total + volume.bytesRead, 0) ?? 0} read, ${resourceStatus?.volumes.reduce((total, volume) => total + volume.bytesWritten, 0) ?? 0} written`,
        `Playback timings (ms): ${JSON.stringify(viewerPlaybackMetrics)}`,
      ].join('\r\n'),
    );
    setMessage('Diagnostic summary copied without personal file paths.');
  };
  const clearPreviewCache = async () => {
    setClearingPreviewCache(true);
    setError('');
    previewWorkQueue.clearQueued();
    hoverPreviewRequestCache.invalidate();
    setPreviewStoredIds(new Set());
    setPreviewMemoryUsage(0);
    setPreviewMemoryCount(0);
    setPreviewCacheEpoch((value) => value + 1);
    try {
      setPreviewCacheUsage(await desktopBridge.clearHoverPreviewCache());
      setMessage('Device-local hover preview cache cleared.');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      hoverPreviewRequestCache.invalidate();
      setPreviewMemoryUsage(0);
      setPreviewMemoryCount(0);
      setPreviewCacheEpoch((value) => value + 1);
      setClearingPreviewCache(false);
    }
  };
  const checkForUpdates = useCallback(async () => {
    if (!updaterConfiguration?.configured) {
      setUpdateStatus('Automatic updates not configured');
      return;
    }
    setCheckingUpdate(true);
    setUpdateStatus(
      `Checking the signed ${updaterConfiguration.channel} channel…`,
    );
    setError('');
    try {
      const update = await checkApprovedBetaUpdate();
      const checkedAt = new Date().toLocaleString();
      localStorage.setItem('pimp-last-update-check', checkedAt);
      setAvailableUpdate(update ?? undefined);
      setUpdateStatus(
        update
          ? `${update.classification} ${update.version} is available.`
          : `No newer signed ${updaterConfiguration.channel} release was available at ${checkedAt}.`,
      );
    } catch (reason) {
      setUpdateStatus(`Update check failed: ${errorText(reason)}`);
    } finally {
      setCheckingUpdate(false);
      setUpdateProgress(undefined);
    }
  }, [updaterConfiguration]);
  useEffect(() => {
    if (
      !native ||
      !applicationInfo ||
      !updaterConfiguration?.configured ||
      settings.checkForUpdates === false ||
      automaticUpdateCheckStarted.current
    ) {
      return;
    }
    automaticUpdateCheckStarted.current = true;
    void checkForUpdates();
    const timer = window.setInterval(
      () => void checkForUpdates(),
      6 * 60 * 60 * 1_000,
    );
    return () => window.clearInterval(timer);
  }, [
    applicationInfo,
    checkForUpdates,
    native,
    settings.checkForUpdates,
    updaterConfiguration,
  ]);
  const installUpdate = async () => {
    if (updateBlockingJobs.length > 0) {
      setUpdateStatus(
        'Resolve every active or recoverable ingest job before installing an update.',
      );
      return;
    }
    if (backupBusy || updateBlockingBackupJobs.length > 0) {
      setUpdateStatus(
        'Wait for every active backup, verification, or restore test to finish before installing an update.',
      );
      return;
    }
    if (discoveryStatus.running) {
      setUpdateStatus(
        'Wait for the active Library scan or integrity verification to finish before installing an update.',
      );
      return;
    }
    setCheckingUpdate(true);
    setUpdateProgress({ downloaded: 0 });
    setUpdateStatus('Preparing signed update download…');
    try {
      await installApprovedBetaUpdate((event) => {
        if (event.event === 'Started') {
          setUpdateStatus('Downloading signed update…');
          setUpdateProgress({
            downloaded: 0,
            total: event.data.contentLength,
          });
        } else if (event.event === 'Progress') {
          setUpdateProgress((current) => ({
            downloaded: (current?.downloaded ?? 0) + event.data.chunkLength,
            total: current?.total,
          }));
        } else {
          setUpdateStatus('Download complete. Verifying and installing…');
        }
      });
      setUpdateStatus(
        'The signed installer was launched. PIMP will close and Windows will restart it after installation; confirm the new version after it reopens.',
      );
    } catch (reason) {
      setAvailableUpdate(undefined);
      setUpdateStatus(`Update installation failed: ${errorText(reason)}`);
    } finally {
      setCheckingUpdate(false);
      setUpdateProgress(undefined);
    }
  };
  const createProject = async () => {
    setBusy(true);
    setError('');
    try {
      await desktopBridge.createProject(projectName, projectDirectory);
      setProjectName('');
      setProjectDirectory('');
      await refresh();
      setMessage('Project and portable .pimp-project.json manifest created.');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const retryProjectSynchronization = async (projectId: string) => {
    setBusy(true);
    setError('');
    try {
      const issues = await desktopBridge.retryProjectSynchronization(projectId);
      await refresh();
      if (issues.length) {
        setError(issues.map((issue) => issue.detail).join(' '));
      } else {
        setMessage('Project manifest synchronization completed.');
      }
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const refreshProjectInventory = async (projectId: string) => {
    const generation = ++projectInventoryGeneration.current;
    setProjectInventoryLoading(true);
    setProjectInventoryError('');
    try {
      const inventory = await desktopBridge.inventoryProject(projectId);
      if (
        generation !== projectInventoryGeneration.current ||
        inventory.projectId !== projectId
      )
        return;
      setProjectInventory(inventory);
    } catch (reason) {
      if (generation !== projectInventoryGeneration.current) return;
      setProjectInventory(undefined);
      setProjectInventoryError(errorText(reason));
    } finally {
      if (generation === projectInventoryGeneration.current)
        setProjectInventoryLoading(false);
    }
  };
  const openProject = (project: ProjectRecord) => {
    projectInventoryGeneration.current += 1;
    setSelectedProjectId(project.id);
    setProjectInventory(undefined);
    setProjectInventoryError('');
    setProjectInventoryLoading(false);
    setProjectNotes(project.notes);
    setProjectTags(project.tags);
    setProjectMediaQuery('');
    setProjectAvailableOpen(false);
    setProjectAvailableQuery('');
    setProjectAvailableTags([]);
    setProjectAvailablePage(1);
    setProjectMediaSelection([]);
    setProjectAddTags([]);
    setProjectRemoveTags([]);
    setProjectMediaDescription('');
    void refreshProjectInventory(project.id);
  };
  const saveProjectInformation = async () => {
    if (!selectedProject) return;
    setBusy(true);
    setError('');
    try {
      setCatalogue(
        await desktopBridge.updateProject({
          projectId: selectedProject.id,
          notes: projectNotes,
          tags: projectTags,
        }),
      );
      setMessage('Project information saved.');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const updateProjectMediaAssignments = async (add: boolean) => {
    if (!selectedProject || !projectMediaSelection.length) return;
    setBusy(true);
    setError('');
    try {
      const mediaIds = expandLogicalMediaIds(
        projectMediaSelection,
        catalogue.media,
      );
      const updated = await desktopBridge.updateMedia({
        mediaIds,
        ...(add
          ? { addProjectIds: [selectedProject.id] }
          : { removeProjectIds: [selectedProject.id] }),
      });
      setCatalogue(updated);
      setProjectMediaSelection([]);
      setMessage(
        add
          ? 'Selected recordings linked to the project.'
          : 'Selected recordings removed from the project; no media files were changed.',
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const copyProjectMediaPaths = async () => {
    const ids = expandLogicalMediaIds(projectMediaSelection, catalogue.media);
    const available = ids
      .map((id) => catalogue.media.find((item) => item.id === id))
      .filter((item): item is NonNullable<typeof item> =>
        Boolean(item?.available),
      );
    if (!available.length) return;
    try {
      const paths = await Promise.all(
        available.map((item) => desktopBridge.resolveMediaPath(item.id)),
      );
      await navigator.clipboard.writeText(paths.join('\r\n'));
      setMessage(
        `${paths.length} original Master Bin path${paths.length === 1 ? '' : 's'} copied for Premiere.`,
      );
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const updateProjectMedia = async (
    patch: {
      addTags?: string[];
      removeTags?: string[];
      favourite?: boolean;
      notes?: string;
    },
    grouped = true,
  ) => {
    if (!projectMediaSelection.length) return;
    const mediaIds = grouped
      ? expandLogicalMediaIds(projectMediaSelection, catalogue.media)
      : [projectMediaSelection[0]];
    setBusy(true);
    setError('');
    try {
      setCatalogue(await desktopBridge.updateMedia({ mediaIds, ...patch }));
      setMessage('Selected project recording metadata saved.');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
  };
  const navigateProjectMediaToLibrary = (mediaId: string) => {
    const requested = catalogue.media.find((item) => item.id === mediaId);
    if (!requested) {
      setError('The selected recording is no longer present in the catalogue.');
      return;
    }
    const visible = displayedMedia.find((item) => item.id === mediaId);
    const groupedParent = displayedMedia.find(
      (item) => item.attachedAudioId === mediaId,
    );
    const target = visible ?? groupedParent;
    if (!target) {
      setError(
        'The selected recording cannot be displayed with the current Library settings.',
      );
      return;
    }
    setView('Library');
    setFolder('All media');
    setQuery('');
    replaceSelection([target.id], target.id);
    setLibraryLocateId(target.id);
  };
  const startBackup = async () => {
    if (!backupProjectId || !backupDestination) return;
    setBackupBusy(true);
    setError('');
    try {
      const completed = await desktopBridge.startProjectBackup(
        backupProjectId,
        backupDestination,
      );
      setBackupJobs(await desktopBridge.listBackupJobs());
      setMessage(
        `Verified ${completed.manifest.sourceFileCount} files on the destination drive.`,
      );
    } catch (reason) {
      setBackupJobs(
        await desktopBridge.listBackupJobs().catch(() => backupJobs),
      );
      setError(errorText(reason));
    } finally {
      setBackupBusy(false);
    }
  };
  const retryBackup = async (jobId: string) => {
    const destinationRoot = await desktopBridge.chooseFolder();
    if (!destinationRoot) return;
    setBackupBusy(true);
    setError('');
    try {
      await desktopBridge.retryProjectBackup(jobId, destinationRoot);
      setBackupJobs(await desktopBridge.listBackupJobs());
      setMessage('Backup completed and independently re-verified.');
    } catch (reason) {
      setBackupJobs(
        await desktopBridge.listBackupJobs().catch(() => backupJobs),
      );
      setError(errorText(reason));
    } finally {
      setBackupBusy(false);
    }
  };
  const verifyBackup = async (jobId: string) => {
    setBackupBusy(true);
    setError('');
    try {
      await desktopBridge.verifyProjectBackup(jobId);
      setBackupJobs(await desktopBridge.listBackupJobs());
      setMessage('Every backup file still matches its SHA-256 manifest.');
    } catch (reason) {
      setBackupJobs(
        await desktopBridge.listBackupJobs().catch(() => backupJobs),
      );
      setError(errorText(reason));
    } finally {
      setBackupBusy(false);
    }
  };
  const testBackupRestore = async (jobId: string) => {
    const restoreRoot = await desktopBridge.chooseFolder();
    if (!restoreRoot) return;
    setBackupBusy(true);
    setError('');
    try {
      const result = await desktopBridge.testProjectBackupRestore(
        jobId,
        restoreRoot,
      );
      setMessage(
        `Restore test passed: ${result.filesVerified} files and ${size(result.bytesVerified)} independently verified.`,
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBackupBusy(false);
    }
  };
  const activeJobs = jobs.filter((job) =>
    ['queued', 'copying', 'verifying'].includes(job.state),
  );
  const completedJobs = jobs.filter(jobIsClearableHistory);
  const needsAttentionJobs = jobs.filter(
    (job) => !activeJobs.includes(job) && !completedJobs.includes(job),
  );
  const workJobs = [...activeJobs, ...needsAttentionJobs];
  const visibleBackgroundOperations = backgroundOperations.filter(
    (operation) => operation.state !== 'completed' || inventoryLoading,
  );
  const activeBackgroundOperations = visibleBackgroundOperations.filter(
    (operation) =>
      !['completed', 'cancelled', 'failed'].includes(operation.state),
  );
  const primaryBackgroundOperation = activeBackgroundOperations[0];
  const primaryOperationElapsedSeconds = primaryBackgroundOperation
    ? Math.max(
        0,
        Math.round(
          (diagnosticNow -
            new Date(primaryBackgroundOperation.startedAt).getTime()) /
            1000,
        ),
      )
    : undefined;
  const primaryOperationThroughput =
    primaryBackgroundOperation?.bytesCompleted && primaryOperationElapsedSeconds
      ? primaryBackgroundOperation.bytesCompleted /
        primaryOperationElapsedSeconds
      : undefined;
  const updateBlockingJobs = jobs.filter((job) => !jobIsClearableHistory(job));
  const updateBlockingBackupJobs = backupJobs.filter((job) =>
    ['prepared', 'copying', 'verifying'].includes(job.state),
  );
  const totalJobBytes = activeJobs.reduce(
    (total, job) => total + job.bytesTotal,
    0,
  );
  const transferredJobBytes = activeJobs.reduce(
    (total, job) => total + job.bytesTransferred,
    0,
  );
  const clearCompletedJobs = async (jobId?: string) => {
    setClearingCompletedJobs(true);
    setError('');
    try {
      const remaining = await desktopBridge.clearCompletedIngestJobs(jobId);
      setJobs(remaining);
      if (!remaining.some(jobIsClearableHistory)) setCompletedJobsOpen(false);
      setMessage(
        jobId
          ? 'Completed ingest history entry cleared. Media and metadata were left unchanged.'
          : 'Safely completed ingest history cleared. Media and metadata were left unchanged.',
      );
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setClearingCompletedJobs(false);
    }
  };
  const activeSettingsSection: SettingsSection = configured
    ? settingsSection
    : 'Library & Storage';
  const settingsSections: SettingsSection[] = [
    'General',
    'Library & Storage',
    'Ingest & Discovery',
    'Resource Management',
    'Appearance',
    'Updates',
    'About & Support',
    'Diagnostics',
  ];

  return (
    <div
      className={`app-shell ${settings.sidebarCollapsed ? 'sidebar-collapsed' : ''}`}
    >
      <aside className="sidebar">
        <div className="brand">
          <div className="mark">P</div>
          <div>
            <strong>PIMP</strong>
            <span>
              Production Ingest
              <br />
              &amp; Media Processor
            </span>
          </div>
        </div>
        <button
          type="button"
          className="sidebar-toggle"
          aria-label={
            settings.sidebarCollapsed
              ? 'Expand application sidebar'
              : 'Collapse application sidebar'
          }
          aria-expanded={!settings.sidebarCollapsed}
          title={
            settings.sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'
          }
          onClick={() =>
            persistPreference({
              sidebarCollapsed: !settings.sidebarCollapsed,
            })
          }
        >
          {settings.sidebarCollapsed ? (
            <PanelLeftOpen aria-hidden="true" size={18} />
          ) : (
            <PanelLeftClose aria-hidden="true" size={18} />
          )}
        </button>
        <nav>
          {(Object.keys(icons) as View[]).map((item) => {
            const Icon = icons[item];
            return (
              <button
                key={item}
                className={view === item ? 'active' : ''}
                aria-label={item}
                title={settings.sidebarCollapsed ? item : undefined}
                onClick={() => setView(item)}
              >
                <Icon size={18} />
                <span>{item}</span>
              </button>
            );
          })}
        </nav>
        <div className="storage">
          <div className="storage-title">
            <HardDrive size={15} /> Master Bin
          </div>
          <div className="storage-copy">
            <span>
              {configured
                ? `${catalogue.media.length} catalogue items`
                : 'Not configured'}
            </span>
          </div>
          <small title={settings.masterBinPath}>
            {settings.masterBinPath || 'Choose a location in Settings'}
          </small>
        </div>
        <div className="edition">
          <i /> PIMP LIGHT{' '}
          <span>
            {native ? 'Native filesystem connected' : 'Browser preview only'}
          </span>
        </div>
      </aside>
      <main
        ref={contentRef}
        className={view === 'Library' ? 'library-main' : 'page-main'}
      >
        <header>
          <div>
            <p>
              {configured ? 'MASTER BIN / LOCAL CATALOGUE' : 'SETUP REQUIRED'}
            </p>
            <h1>{view}</h1>
          </div>
          <div className="header-actions">
            {native &&
              configured &&
              (jobs.length > 0 || visibleBackgroundOperations.length > 0) && (
                <button
                  className="job-indicator"
                  aria-expanded={jobsOpen}
                  onClick={() => setJobsOpen((open) => !open)}
                >
                  {activeJobs.length + activeBackgroundOperations.length > 0
                    ? `${activeJobs.length + activeBackgroundOperations.length} active${
                        activeJobs.length > 0
                          ? ` · ${Math.round(
                              (transferredJobBytes /
                                Math.max(totalJobBytes, 1)) *
                                100,
                            )}% ingest`
                          : ''
                      }`
                    : needsAttentionJobs.length > 0
                      ? `${needsAttentionJobs.length} need${needsAttentionJobs.length === 1 ? 's' : ''} attention`
                      : 'All ingests complete'}
                </button>
              )}
            <div className="sync">
              {native ? 'Native desktop' : 'Web preview'}{' '}
              <span>{configured ? 'persisted' : 'not configured'}</span>
            </div>
          </div>
        </header>
        {jobsOpen && (
          <section className="job-panel" aria-label="Background Operations">
            <div className="job-panel-heading">
              <div>
                <strong>Background Operations</strong>
                <small>
                  Inventory cancellation stops at a safe read boundary. Ingest
                  pause takes effect between files.
                </small>
              </div>
              <button onClick={() => setJobsOpen(false)}>Close</button>
            </div>
            <div className="job-counts" role="status">
              <span>Other work {visibleBackgroundOperations.length}</span>
              <span>Active {activeJobs.length}</span>
              <span>Completed {completedJobs.length}</span>
              <span>Needs attention {needsAttentionJobs.length}</span>
            </div>
            <p className="job-complete-status" role="status">
              Resource profile:{' '}
              {resourceStatus?.profile ?? settings.resourceProfile}
              {resourceStatus?.backgroundPaused ? ' · background paused' : ''}
              {resourceStatus?.waiting[0]
                ? ` · ${resourceStatus.waiting[0].operation}: ${resourceStatus.waiting[0].reason}`
                : ' · no resource waits'}
            </p>
            {visibleBackgroundOperations.map((operation) => (
              <article className="job-card" key={operation.id}>
                <header>
                  <div>
                    <strong>{operation.title}</strong>
                    <small>
                      {operation.stage}
                      {operation.currentItem
                        ? ` · ${operation.currentItem}`
                        : ''}
                    </small>
                  </div>
                  <div className="job-actions">
                    {operation.cancellable && (
                      <button
                        onClick={() =>
                          void desktopBridge.cancelBackgroundOperation(
                            operation.id,
                          )
                        }
                      >
                        {operation.kind === 'playback-proxy'
                          ? 'Cancel conversion'
                          : 'Cancel scan'}
                      </button>
                    )}
                  </div>
                </header>
                {operation.bytesTotal ? (
                  <progress
                    value={operation.bytesCompleted}
                    max={operation.bytesTotal}
                  />
                ) : operation.totalItems ? (
                  <progress
                    value={operation.completedItems}
                    max={operation.totalItems}
                  />
                ) : (
                  <progress />
                )}
                <small>
                  {operation.kind === 'playback-proxy' ? (
                    <>
                      {duration(operation.completedItems)}
                      {operation.totalItems === undefined
                        ? ' processed'
                        : ` / ${duration(operation.totalItems)} processed`}
                    </>
                  ) : (
                    <>
                      {operation.completedItems}
                      {operation.totalItems === undefined
                        ? ' discovered'
                        : ` / ${operation.totalItems} files`}
                    </>
                  )}
                  {operation.bytesTotal
                    ? ` · ${size(operation.bytesCompleted)} / ${size(operation.bytesTotal)} read`
                    : ''}
                  {operation.kind ===
                  'playback-proxy' ? null : operation.kind ===
                    'media-enrichment' ? (
                    <>
                      {' · '}
                      {operation.counts.newFiles} thumbnails generated ·{' '}
                      {operation.counts.existingFiles} existing ·{' '}
                      {operation.counts.attemptedFiles} attempted ·{' '}
                      {operation.counts.failedFiles} unavailable or failed ·{' '}
                      {operation.counts.pendingFiles} pending
                    </>
                  ) : (
                    <>
                      {' · '}
                      {operation.counts.newFiles} new ·{' '}
                      {operation.counts.duplicateFiles} duplicate ·{' '}
                      {operation.counts.attentionFiles} attention ·{' '}
                      {operation.counts.failedFiles} failed
                    </>
                  )}
                </small>
                {operation.error && (
                  <small className="job-error">{operation.error}</small>
                )}
              </article>
            ))}
            {activeJobs.length === 0 && needsAttentionJobs.length === 0 && (
              <p className="job-complete-status">All ingests complete</p>
            )}
            {workJobs.map((job) => (
              <article className="job-card" key={job.id}>
                <header>
                  <div>
                    <strong>{job.files.length} file ingest</strong>
                    <small>
                      {job.state} · {job.filesCompleted}/{job.files.length}{' '}
                      complete · {size(job.bytesTransferred)} /{' '}
                      {size(job.bytesTotal)}
                    </small>
                  </div>
                  <div className="job-actions">
                    {['copying', 'queued', 'verifying'].includes(job.state) && (
                      <button onClick={() => desktopBridge.pauseIngest(job.id)}>
                        Pause between files
                      </button>
                    )}
                    {job.state === 'paused' && (
                      <button
                        onClick={() => desktopBridge.resumeIngest(job.id)}
                      >
                        Resume
                      </button>
                    )}
                    {['failed', 'cancelled', 'interrupted'].includes(
                      job.state,
                    ) &&
                      (job.inventoryIssues?.length ?? 0) === 0 && (
                        <button
                          onClick={() => desktopBridge.retryIngest(job.id)}
                        >
                          Retry
                        </button>
                      )}
                    {!['completed', 'cancelled'].includes(job.state) && (
                      <button
                        onClick={() => desktopBridge.cancelIngest(job.id)}
                      >
                        Cancel
                      </button>
                    )}
                  </div>
                </header>
                <progress
                  value={job.bytesTransferred}
                  max={job.bytesTotal || 1}
                />
                {(job.inventoryExcluded?.length ?? 0) > 0 && (
                  <small>
                    {job.inventoryExcluded?.length} deliberately excluded
                    temporary or application file
                    {job.inventoryExcluded?.length === 1 ? '' : 's'}.
                  </small>
                )}
                {job.inventoryIssues?.map((issue) => (
                  <small className="job-error" key={issue}>
                    Inventory incomplete: {issue}. Fix the source and create a
                    new ingest.
                  </small>
                ))}
                <div className="job-files">
                  {job.files.map((file) => (
                    <div className="job-file" key={file.id}>
                      <div className="job-file-title">
                        <strong>{file.originalFilename}</strong>
                        <span>{file.state}</span>
                      </div>
                      {file.error && (
                        <small className="job-error">{file.error}</small>
                      )}
                      {file.metadataError && (
                        <small className="job-error">
                          Metadata not yet saved to the catalogue:{' '}
                          {file.metadataError}
                        </small>
                      )}
                      <label>
                        Tags
                        <input
                          value={file.metadata.tags.join(', ')}
                          onChange={(event) =>
                            updateJobFile(job.id, file.id, {
                              tags: event.target.value
                                .split(',')
                                .map((tag) => tag.trim())
                                .filter(Boolean),
                            })
                          }
                          placeholder="Track, onboard, interview"
                        />
                      </label>
                      <SearchableComboBox
                        label="Projects"
                        mode="multiple"
                        options={catalogue.projects.map((project) => ({
                          id: project.id,
                          label: project.name,
                        }))}
                        selectedIds={file.metadata.projectIds}
                        onSelectionChange={(projectIds) =>
                          updateJobFile(job.id, file.id, { projectIds })
                        }
                        placeholder="Assign projects…"
                      />
                      <label>
                        Description
                        <textarea
                          value={file.metadata.notes}
                          onChange={(event) =>
                            updateJobFile(job.id, file.id, {
                              notes: event.target.value,
                            })
                          }
                        />
                      </label>
                      <label className="job-favourite">
                        <input
                          type="checkbox"
                          checked={file.metadata.favourite}
                          onChange={(event) =>
                            updateJobFile(job.id, file.id, {
                              favourite: event.target.checked,
                            })
                          }
                        />{' '}
                        Favourite
                      </label>
                    </div>
                  ))}
                </div>
              </article>
            ))}
            {completedJobs.length > 0 && (
              <section className="completed-job-history">
                <div className="completed-job-heading">
                  <button
                    type="button"
                    aria-expanded={completedJobsOpen}
                    onClick={() => setCompletedJobsOpen((open) => !open)}
                  >
                    Completed history ({completedJobs.length})
                  </button>
                  <button
                    type="button"
                    disabled={clearingCompletedJobs}
                    onClick={() => void clearCompletedJobs()}
                  >
                    Clear Completed
                  </button>
                </div>
                {completedJobsOpen && (
                  <div className="completed-job-list">
                    {completedJobs.map((job) => {
                      const exceptions =
                        job.unsupported.length +
                        (job.inventoryExcluded?.length ?? 0);
                      return (
                        <article className="job-card completed" key={job.id}>
                          <div>
                            <strong>
                              {job.filesCompleted} file
                              {job.filesCompleted === 1 ? '' : 's'} copied and
                              verified
                            </strong>
                            <small>
                              {size(job.bytesTotal)} ·{' '}
                              {new Date(job.updatedAt).toLocaleString()} ·{' '}
                              {exceptions === 0
                                ? 'No exceptions'
                                : `${exceptions} reported exception${exceptions === 1 ? '' : 's'}`}
                            </small>
                          </div>
                          <button
                            type="button"
                            disabled={clearingCompletedJobs}
                            onClick={() => void clearCompletedJobs(job.id)}
                          >
                            Clear
                          </button>
                        </article>
                      );
                    })}
                  </div>
                )}
              </section>
            )}
          </section>
        )}
        {(message || error) && (
          <section className={`notice ${error ? 'error' : ''}`}>
            <span>{error || message}</span>
            <button
              onClick={() => {
                setMessage('');
                setError('');
              }}
            >
              Dismiss
            </button>
          </section>
        )}
        {mediaTools &&
          (!mediaTools.ffmpegAvailable || !mediaTools.ffprobeAvailable) && (
            <section className="notice tool-warning" role="status">
              <span>
                Optional media tools are missing. Verified ingest, catalogue,
                search and metadata remain available.
                {!mediaTools.ffmpegAvailable &&
                  ' FFmpeg thumbnails are disabled.'}
                {!mediaTools.ffprobeAvailable &&
                  ' FFprobe technical metadata is disabled.'}{' '}
                Install FFmpeg with <code>winget install Gyan.FFmpeg</code>,
                then restart PIMP.
              </span>
            </section>
          )}
        {((catalogue.conflicts?.length ?? 0) > 0 ||
          (catalogue.issues?.length ?? 0) > 0) && (
          <section className="notice error" role="status">
            <span>
              Metadata needs attention: {catalogue.conflicts?.length ?? 0}{' '}
              concurrent conflict(s), {catalogue.issues?.length ?? 0} catalogue
              issue(s). Authoritative event files were left unchanged.
            </span>
          </section>
        )}
        {!native ? (
          <section className="empty">
            <h2>Open the native PIMP application</h2>
            <p>
              The browser build intentionally has no filesystem access and never
              substitutes demonstration media for your catalogue.
            </p>
          </section>
        ) : view === 'Settings' || !configured ? (
          <section className="settings-layout">
            <nav className="settings-nav" aria-label="Settings sections">
              {settingsSections.map((section) => (
                <button
                  key={section}
                  className={activeSettingsSection === section ? 'active' : ''}
                  disabled={!configured && section !== 'Library & Storage'}
                  onClick={() => setSettingsSection(section)}
                >
                  {section}
                </button>
              ))}
            </nav>
            <div className="settings-form">
              <div className="settings-heading">
                <SlidersHorizontal size={20} />
                <div>
                  <h2>{activeSettingsSection}</h2>
                  <p>Device-local preferences for Kairix PIMP Light.</p>
                  {settingsWritePending && (
                    <small role="status">Saving device preferences…</small>
                  )}
                  {settingsWriteFailed && (
                    <button onClick={() => void retrySettingsSave()}>
                      Retry saving preferences
                    </button>
                  )}
                </div>
              </div>

              {activeSettingsSection === 'General' && (
                <>
                  <ToggleField
                    label="Start with Windows"
                    description="Launch PIMP at sign-in so background ingest and discovery can resume."
                    checked={settings.startWithWindows ?? false}
                    onChange={(startWithWindows) =>
                      void persistPreference({ startWithWindows })
                    }
                  />
                  <ToggleField
                    label="Close to system tray"
                    description="Keep PIMP running in the Windows notification area when the main window closes."
                    checked={settings.closeToTray ?? true}
                    onChange={(closeToTray) =>
                      void persistPreference({ closeToTray })
                    }
                  />
                  <label>
                    Default startup page
                    <select
                      value={settings.startupPage ?? 'Library'}
                      onChange={(event) =>
                        void persistPreference({
                          startupPage: event.target
                            .value as DeviceSettings['startupPage'],
                        })
                      }
                    >
                      {['Ingest', 'Library', 'Projects', 'Archive'].map(
                        (page) => (
                          <option key={page}>{page}</option>
                        ),
                      )}
                    </select>
                  </label>
                  <ToggleField
                    label="Check for updates automatically"
                    description="Check after startup and every six hours without delaying normal use."
                    checked={settings.checkForUpdates ?? true}
                    onChange={(checkForUpdates) =>
                      void persistPreference({ checkForUpdates })
                    }
                  />
                  <label>
                    Update channel
                    <select
                      value={settings.updateChannel ?? 'development'}
                      onChange={(event) =>
                        void persistPreference({
                          updateChannel: event.target
                            .value as DeviceSettings['updateChannel'],
                        })
                      }
                    >
                      <option value="development">Development</option>
                      <option value="stable">Stable</option>
                    </select>
                    <small>
                      Development receives approved ongoing releases and newer
                      stable milestones. Stable receives only formally approved
                      stable releases and stable hotfixes. Changing channel
                      never downgrades PIMP.
                    </small>
                  </label>
                </>
              )}

              {activeSettingsSection === 'Library & Storage' && (
                <>
                  <p>
                    Absolute paths stay on this device. Portable catalogue
                    records use paths relative to Master Bin.
                  </p>
                  <PathField
                    label="Master Bin"
                    value={settings.masterBinPath}
                    onChange={(masterBinPath) =>
                      setSettings({ ...settings, masterBinPath })
                    }
                    onBrowse={() => chooseSettingFolder('masterBinPath')}
                  />
                  <PathField
                    label="Working Projects"
                    value={settings.workingProjectsPath}
                    onChange={(workingProjectsPath) =>
                      setSettings({ ...settings, workingProjectsPath })
                    }
                    onBrowse={() => chooseSettingFolder('workingProjectsPath')}
                  />
                  <ToggleField
                    label="Store shared catalogue inside Master Bin"
                    description="Recommended when Syncthing shares the Master Bin between devices."
                    checked={settings.catalogueInMasterBin}
                    onChange={(catalogueInMasterBin) =>
                      setSettings({ ...settings, catalogueInMasterBin })
                    }
                  />
                  <ToggleField
                    label="Show attached audio as separate cards"
                    description="By default, a matching WAV is grouped with its video as one logical recording."
                    checked={settings.showAttachedAudioSeparately ?? false}
                    onChange={(showAttachedAudioSeparately) =>
                      void persistPreference({ showAttachedAudioSeparately })
                    }
                  />
                  <button
                    disabled={busy}
                    onClick={() => void reEvaluateAudioPairing()}
                  >
                    <RefreshCw size={14} /> Re-evaluate audio pairing
                  </button>
                  {!settings.catalogueInMasterBin && (
                    <PathField
                      label="Shared catalogue"
                      value={settings.sharedCataloguePath}
                      onChange={(sharedCataloguePath) =>
                        setSettings({ ...settings, sharedCataloguePath })
                      }
                      onBrowse={() =>
                        chooseSettingFolder('sharedCataloguePath')
                      }
                    />
                  )}
                  <PathField
                    label="Local derivative cache (hover previews and proxies)"
                    value={settings.thumbnailCachePath}
                    placeholder="Automatic local cache"
                    onChange={(thumbnailCachePath) =>
                      setSettings({ ...settings, thumbnailCachePath })
                    }
                    onBrowse={() => chooseSettingFolder('thumbnailCachePath')}
                  />
                  {configured && (
                    <div className="scan-status" role="status">
                      <h3>Library discovery</h3>
                      <p>
                        PIMP indexes existing files. Syncthing independently
                        transfers them between devices.
                      </p>
                      <dl>
                        <div>
                          <dt>Last completed scan</dt>
                          <dd>
                            {discoveryStatus.lastCompletedAt
                              ? new Date(
                                  discoveryStatus.lastCompletedAt,
                                ).toLocaleString()
                              : 'Not completed yet'}
                          </dd>
                        </div>
                        <div>
                          <dt>Progress</dt>
                          <dd>
                            {discoveryStatus.running
                              ? discoveryStatus.totalFiles > 0
                                ? `${discoveryStatus.filesProcessed} / ${discoveryStatus.totalFiles}`
                                : `${discoveryStatus.filesProcessed} processed · discovering total…`
                              : 'Idle'}
                          </dd>
                        </div>
                        <div>
                          <dt>Registered media</dt>
                          <dd>{catalogue.media.length}</dd>
                        </div>
                        <div>
                          <dt>Pending / unverified</dt>
                          <dd>{discoveryStatus.pending}</dd>
                        </div>
                        <div>
                          <dt>Scan errors</dt>
                          <dd>{discoveryStatus.errors}</dd>
                        </div>
                      </dl>
                      <div className="scan-actions">
                        <button
                          disabled={busy || discoveryStatus.running}
                          onClick={doRescan}
                        >
                          Quick inventory (no content reads)
                        </button>
                        <button
                          disabled={busy || discoveryStatus.running}
                          onClick={doFullRescan}
                        >
                          Full library rescan / verify
                        </button>
                      </div>
                    </div>
                  )}
                  {settings.initializedCataloguePath && (
                    <small>
                      Catalogue location is locked until a verified migration
                      workflow is implemented:{' '}
                      {settings.initializedCataloguePath}
                    </small>
                  )}
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={saveConfiguration}
                  >
                    {busy ? 'Saving…' : 'Save configuration'}
                  </button>
                </>
              )}

              {activeSettingsSection === 'Ingest & Discovery' && (
                <>
                  <fieldset>
                    <legend>Remembered date organization</legend>
                    <label>
                      <input
                        type="radio"
                        checked={settings.ingestDateOrganization === 'today'}
                        onChange={() => {
                          setSettings({
                            ...settings,
                            ingestDateOrganization: 'today',
                          });
                          if (!sources.length) setDateOverride('today');
                        }}
                      />{' '}
                      Use today's date
                    </label>
                    <label>
                      <input
                        type="radio"
                        checked={
                          settings.ingestDateOrganization ===
                          'original-file-dates'
                        }
                        onChange={() => {
                          setSettings({
                            ...settings,
                            ingestDateOrganization: 'original-file-dates',
                          });
                          if (!sources.length)
                            setDateOverride('original-file-dates');
                        }}
                      />{' '}
                      Use original file dates
                    </label>
                  </fieldset>
                  <label>
                    Date-folder naming pattern
                    <input
                      value={settings.dateFolderPattern ?? 'yyyymmdd'}
                      onChange={(event) =>
                        setSettings({
                          ...settings,
                          dateFolderPattern: event.target.value,
                        })
                      }
                    />
                    <small>
                      Current preview:{' '}
                      {datePatternPreview(settings.dateFolderPattern)}. MM is
                      month; mm is minute.
                    </small>
                  </label>
                  <fieldset>
                    <legend>SD Card Grabber</legend>
                    <label>
                      <input
                        type="checkbox"
                        checked={settings.sdGrabberEnabled}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            sdGrabberEnabled: event.target.checked,
                          })
                        }
                      />{' '}
                      Enable SD Card Grabber
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        checked={settings.sdGrabberAutoDetect}
                        disabled={!settings.sdGrabberEnabled}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            sdGrabberAutoDetect: event.target.checked,
                          })
                        }
                      />{' '}
                      Detect removable cards automatically
                    </label>
                    <label>
                      Included file extensions
                      <input
                        value={settings.sdGrabberExtensions.join(', ')}
                        disabled={!settings.sdGrabberEnabled}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            sdGrabberExtensions: event.target.value
                              .split(',')
                              .map((value) => value.trim())
                              .filter(Boolean),
                          })
                        }
                      />
                      <small>
                        Defaults to MP4 and WAV; matching ignores case.
                      </small>
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        checked={settings.sdGrabberSearchSubfolders}
                        disabled={!settings.sdGrabberEnabled}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            sdGrabberSearchSubfolders: event.target.checked,
                          })
                        }
                      />{' '}
                      Search subfolders
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        checked={settings.sdGrabberRememberDevices}
                        disabled={!settings.sdGrabberEnabled}
                        onChange={(event) =>
                          setSettings({
                            ...settings,
                            sdGrabberRememberDevices: event.target.checked,
                          })
                        }
                      />{' '}
                      Remember recognised device identities
                    </label>
                    <label title="Source cleanup is not implemented">
                      <input type="checkbox" checked={false} disabled /> Offer
                      source cleanup after independent verification
                    </label>
                    <small>
                      Cleanup remains disabled. The Grabber copies, verifies and
                      registers media without modifying the card.
                    </small>
                  </fieldset>
                  <div className="settings-explainer">
                    New external files require repeated stable observations and
                    SHA-256 verification. PIMP never replaces a destination on
                    collision and never modifies source media.
                  </div>
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={saveConfiguration}
                  >
                    {busy ? 'Saving…' : 'Save ingest preferences'}
                  </button>
                </>
              )}

              {activeSettingsSection === 'Resource Management' && (
                <>
                  <fieldset className="settings-subsection">
                    <legend>Processing profile</legend>
                    {(
                      [
                        [
                          'quiet',
                          'Quiet',
                          'Spaces background admissions apart and permits one background worker across volumes.',
                        ],
                        [
                          'balanced',
                          'Balanced — recommended',
                          'Keeps foreground work responsive with one background worker during normal activity.',
                        ],
                        [
                          'performance',
                          'Performance',
                          'Allows two background workers when they use independent volumes; each volume remains serialized.',
                        ],
                      ] as const
                    ).map(([value, label, description]) => (
                      <label key={value}>
                        <input
                          type="radio"
                          name="resource-profile"
                          checked={settings.resourceProfile === value}
                          onChange={() =>
                            void persistPreference({ resourceProfile: value })
                          }
                        />{' '}
                        <strong>{label}</strong>
                        <small>{description}</small>
                      </label>
                    ))}
                  </fieldset>
                  <ToggleField
                    label="Gentle scanning for older drives"
                    description="Adds a safe delay between automatic background admissions. Full-file access remains limited to one worker per volume."
                    checked={settings.gentleScanning}
                    onChange={(gentleScanning) =>
                      void persistPreference({ gentleScanning })
                    }
                  />
                  <ToggleField
                    label="Pause automatic background processing"
                    description="Pauses automatic discovery, enrichment, and preview preparation at safe file boundaries. Ingest, explicit verification, playback, and visible thumbnails remain available."
                    checked={settings.backgroundProcessingPaused}
                    onChange={(backgroundProcessingPaused) =>
                      void persistPreference({ backgroundProcessingPaused })
                    }
                  />
                  <ToggleField
                    label="Increase processing while safely idle"
                    description="Allows controlled extra work only after the idle delay and when supported Windows CPU and memory evidence also indicate capacity. Input inactivity alone is insufficient."
                    checked={settings.increaseProcessingWhileIdle}
                    onChange={(increaseProcessingWhileIdle) =>
                      void persistPreference({ increaseProcessingWhileIdle })
                    }
                  />
                  <label>
                    Idle delay (minutes)
                    <input
                      type="number"
                      min="1"
                      max="120"
                      value={settings.resourceIdleDelayMinutes}
                      onChange={(event) =>
                        void persistPreference({
                          resourceIdleDelayMinutes: Number(event.target.value),
                        })
                      }
                    />
                  </label>
                  <div className="settings-explainer" role="status">
                    <strong>Current scheduler</strong>
                    <p>
                      {resourceStatus
                        ? `${resourceStatus.activeBackgroundWorkers} active background worker(s), ${resourceStatus.queuedWorkers} waiting · ${resourceStatus.volumes.length} observed volume(s)`
                        : 'Loading device-local resource status…'}
                    </p>
                    <p>
                      PIMP uses stable volume identity where Windows supplies
                      it. Physical-drive grouping behind separate logical
                      volumes is not inferred when Windows cannot prove the
                      relationship.
                    </p>
                    <p>
                      Idle boost:{' '}
                      {resourceStatus?.systemActivity.idleBoostEligible
                        ? 'active'
                        : 'inactive'}{' '}
                      ·{' '}
                      {resourceStatus?.systemActivity.reason ??
                        'Waiting for native activity evidence'}
                    </p>
                  </div>
                  <fieldset className="settings-subsection">
                    <legend>Manual processing</legend>
                    <p>
                      Manual work moves ahead of automatic preparation but never
                      bypasses volume serialization, source validation, or
                      no-overwrite rules.
                    </p>
                    <div className="scan-actions">
                      <button
                        disabled={busy}
                        onClick={() =>
                          void desktopBridge
                            .enrichMedia()
                            .then((count) =>
                              setMessage(
                                `Technical metadata and missing static thumbnails processed for ${count} recording(s).`,
                              ),
                            )
                            .catch((reason) => setError(errorText(reason)))
                        }
                      >
                        Generate missing thumbnails
                      </button>
                      <button
                        disabled={selected.length === 0}
                        onClick={() =>
                          void Promise.all(
                            selected.map((mediaId) =>
                              desktopBridge.loadHoverPreview(mediaId),
                            ),
                          )
                            .then(() =>
                              setMessage(
                                `Prepared scrubbable previews for ${selected.length} selected recording(s).`,
                              ),
                            )
                            .catch((reason) => setError(errorText(reason)))
                        }
                      >
                        Prepare selected previews
                      </button>
                      <button
                        disabled={selected.length === 0}
                        onClick={() =>
                          void Promise.all(
                            selected.map((mediaId) =>
                              desktopBridge.verifyMediaIntegrity(mediaId),
                            ),
                          )
                            .then(() =>
                              setMessage(
                                `Integrity verification completed for ${selected.length} selected recording(s).`,
                              ),
                            )
                            .catch((reason) => setError(errorText(reason)))
                        }
                      >
                        Verify selected media
                      </button>
                      <button
                        disabled={discoveryStatus.running}
                        onClick={doRescan}
                      >
                        Quick Library inventory
                      </button>
                    </div>
                  </fieldset>
                  <small>
                    GPU, battery, and explicit throughput caps are omitted until
                    the native host can enforce and measure them reliably.
                  </small>
                </>
              )}

              {activeSettingsSection === 'Appearance' && (
                <>
                  <div
                    className="theme-grid"
                    role="radiogroup"
                    aria-label="Theme"
                  >
                    {[
                      [
                        'graphite',
                        'Graphite',
                        'Neutral charcoal and cool blue',
                      ],
                      [
                        'adobe-dark',
                        'Adobe-style Dark',
                        'Compact neutral editor greys',
                      ],
                      ['light', 'Light', 'Neutral off-white surfaces'],
                      ['midnight', 'Midnight', 'Deep blue-grey surfaces'],
                      ['system', 'System', 'Follow Windows appearance'],
                      [
                        'high-contrast',
                        'High Contrast',
                        'Maximum separation and focus',
                      ],
                    ].map(([id, label, description]) => (
                      <button
                        key={id}
                        role="radio"
                        aria-checked={(settings.theme ?? 'graphite') === id}
                        className={
                          (settings.theme ?? 'graphite') === id ? 'active' : ''
                        }
                        onClick={() =>
                          void persistPreference({
                            theme: id as DeviceSettings['theme'],
                          })
                        }
                      >
                        <span className={`theme-swatch ${id}`} />
                        <strong>{label}</strong>
                        <small>{description}</small>
                      </button>
                    ))}
                  </div>
                  <label>
                    Layout density
                    <select
                      value={settings.density ?? 'comfortable'}
                      onChange={(event) =>
                        void persistPreference({
                          density: event.target
                            .value as DeviceSettings['density'],
                        })
                      }
                    >
                      <option value="comfortable">Comfortable</option>
                      <option value="compact">Compact</option>
                    </select>
                  </label>
                  <fieldset className="settings-subsection toolbar-settings">
                    <legend>Library selection toolbar</legend>
                    <label>
                      Position
                      <select
                        value={selectionToolbarPosition}
                        onChange={(event) => {
                          const position = event.target
                            .value as SelectionToolbarPosition;
                          setSelectionToolbarPosition(position);
                          localStorage.setItem(
                            'pimp-selection-toolbar-position',
                            position,
                          );
                        }}
                      >
                        <option value="bottom-right">Bottom right</option>
                        <option value="bottom-center">Bottom center</option>
                        <option value="bottom-left">Bottom left</option>
                      </select>
                    </label>
                    <div role="group" aria-label="Primary selection actions">
                      {(
                        [
                          ['tags', 'Add tags'],
                          ['projects', 'Assign projects'],
                          ['favourite', 'Favourite'],
                        ] as Array<[SelectionToolbarAction, string]>
                      ).map(([action, label]) => (
                        <label className="inline-check" key={action}>
                          <input
                            type="checkbox"
                            checked={selectionToolbarActions.includes(action)}
                            onChange={(event) => {
                              const next = event.target.checked
                                ? [...selectionToolbarActions, action]
                                : selectionToolbarActions.filter(
                                    (candidate) => candidate !== action,
                                  );
                              setSelectionToolbarActions(next);
                              localStorage.setItem(
                                'pimp-selection-toolbar-actions',
                                JSON.stringify(next),
                              );
                            }}
                          />
                          {label}
                        </label>
                      ))}
                    </div>
                    <button
                      type="button"
                      onClick={() => {
                        setSelectionToolbarPosition('bottom-right');
                        setSelectionToolbarActions(
                          defaultSelectionToolbarActions,
                        );
                        localStorage.removeItem(
                          'pimp-selection-toolbar-position',
                        );
                        localStorage.removeItem(
                          'pimp-selection-toolbar-actions',
                        );
                      }}
                    >
                      Reset toolbar to defaults
                    </button>
                    <small>
                      Toolbar layout stays on this device and never changes
                      catalogue metadata.
                    </small>
                  </fieldset>
                  <fieldset className="settings-subsection">
                    <legend>Thumbnail &amp; Preview</legend>
                    <label>
                      Thumbnail hold frame
                      <select
                        value={settings.thumbnailHoldFrame ?? 'first'}
                        onChange={(event) =>
                          void persistPreference({
                            thumbnailHoldFrame: event.target
                              .value as DeviceSettings['thumbnailHoldFrame'],
                          })
                        }
                      >
                        <option value="first">First frame</option>
                        <option value="10-percent">10% into the video</option>
                        <option value="20-percent">20% into the video</option>
                        <option value="middle">Middle / 50%</option>
                        <option value="80-percent">80% into the video</option>
                        <option value="last">Last frame</option>
                        <option value="last-hovered">
                          Keep last hovered frame
                        </option>
                      </select>
                    </label>
                    <small>
                      Uses the nearest real sample in the device-local preview
                      cache. Missing previews fall back to the normal thumbnail.
                    </small>
                  </fieldset>
                  <label>
                    Preview preloading
                    <select
                      value={
                        settings.hoverPreviewsEnabled === false
                          ? 'off'
                          : (settings.previewPreloading ?? 'visible-nearby')
                      }
                      onChange={(event) => {
                        const previewPreloading = event.target
                          .value as DeviceSettings['previewPreloading'];
                        void persistPreference({
                          previewPreloading,
                          hoverPreviewsEnabled: previewPreloading !== 'off',
                        });
                      }}
                    >
                      <option value="off">Off (static thumbnails only)</option>
                      <option value="hovered">On hover</option>
                      <option value="visible">Visible recordings</option>
                      <option value="visible-nearby">
                        Visible and nearby recordings (recommended)
                      </option>
                      <option value="idle">
                        Prepare additional recordings when idle
                      </option>
                    </select>
                  </label>
                  <label>
                    RAM preview-cache allowance
                    <select
                      value={settings.previewRamCacheLimitBytes ?? 0}
                      onChange={(event) => {
                        const previewRamCacheLimitBytes = Number(
                          event.target.value,
                        );
                        hoverPreviewRequestCache.configure(
                          previewRamCacheLimitBytes || 128 * 1024 * 1024,
                        );
                        setPreviewMemoryUsage(
                          hoverPreviewRequestCache.usageBytes,
                        );
                        void persistPreference({ previewRamCacheLimitBytes });
                      }}
                    >
                      <option value={0}>Automatic</option>
                      <option value={64 * 1024 * 1024}>64 MB</option>
                      <option value={128 * 1024 * 1024}>128 MB</option>
                      <option value={256 * 1024 * 1024}>256 MB</option>
                      <option value={512 * 1024 * 1024}>512 MB</option>
                    </select>
                  </label>
                  <label>
                    Hover preview cache limit (GB)
                    <input
                      type="number"
                      min="0.1"
                      max="20"
                      step="0.1"
                      value={(
                        (settings.previewCacheLimitBytes ?? 512 * 1024 * 1024) /
                        1024 /
                        1024 /
                        1024
                      ).toFixed(1)}
                      onChange={(event) =>
                        void persistPreference({
                          previewCacheLimitBytes: Math.max(
                            100 * 1024 * 1024,
                            Math.round(
                              Number(event.target.value) * 1024 * 1024 * 1024,
                            ),
                          ),
                        })
                      }
                    />
                  </label>
                  <label>
                    Playback proxy cache limit (GB)
                    <input
                      type="number"
                      min="0.5"
                      max="100"
                      step="0.5"
                      value={(
                        (settings.playbackProxyCacheLimitBytes ??
                          2 * 1024 * 1024 * 1024) /
                        1024 /
                        1024 /
                        1024
                      ).toFixed(1)}
                      onChange={(event) =>
                        void persistPreference({
                          playbackProxyCacheLimitBytes: Math.max(
                            512 * 1024 * 1024,
                            Math.round(
                              Number(event.target.value) * 1024 * 1024 * 1024,
                            ),
                          ),
                        })
                      }
                    />
                  </label>
                  <div className="info-card">
                    <strong>Hover preview cache</strong>
                    <span>
                      {previewMemoryCount} loaded · {size(previewMemoryUsage)}
                      {' in memory'}
                    </span>
                    <span>
                      {previewStoredIds.size} stored · {size(previewCacheUsage)}
                      {' on disk'}
                    </span>
                    <span role="status">
                      {staticThumbnailQueueStatus.active
                        ? `Generating static thumbnail · ${staticThumbnailQueueStatus.queued} queued`
                        : staticThumbnailQueueStatus.queued
                          ? `${staticThumbnailQueueStatus.queued} static thumbnails queued`
                          : 'Static thumbnail queue idle'}
                    </span>
                    <span role="status">
                      {previewQueueStatus.active
                        ? `Preparing ${previewQueueStatus.activeLabel ?? 'preview'} · ${previewQueueStatus.queued} queued`
                        : previewQueueStatus.queued
                          ? `${previewQueueStatus.queued} previews queued`
                          : 'Generation queue idle'}
                    </span>
                    <small>
                      Regenerable frames stay in the device-local cache and are
                      never synchronized.
                    </small>
                    <button
                      type="button"
                      disabled={clearingPreviewCache}
                      onClick={() => void clearPreviewCache()}
                    >
                      {clearingPreviewCache
                        ? 'Waiting for preview generation…'
                        : 'Clear preview cache'}
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        void desktopBridge
                          .hoverPreviewCacheUsage()
                          .then(setPreviewCacheUsage)
                          .catch((reason) => setError(errorText(reason)))
                      }
                    >
                      Refresh cache usage
                    </button>
                  </div>
                </>
              )}

              {activeSettingsSection === 'Updates' && (
                <>
                  <div className="info-card">
                    <strong>
                      {applicationInfo?.edition ?? 'Kairix PIMP Light'}{' '}
                      {applicationInfo?.displayVersion ?? 'unknown'} —{' '}
                      {applicationInfo?.releaseClassification ?? 'Development'}
                    </strong>
                    <span>
                      Update channel{' '}
                      {updaterConfiguration?.channel ?? 'Development'}
                    </span>
                    <span>
                      Release date {applicationInfo?.releaseDate ?? 'unknown'}
                    </span>
                    <span>
                      Build {applicationInfo?.buildCommit ?? 'development'}
                    </span>
                  </div>
                  <div className="info-card" role="status">
                    <strong>Update status</strong>
                    <span>{updateStatus}</span>
                    <small>
                      Last successful check:{' '}
                      {localStorage.getItem('pimp-last-update-check') ??
                        'Never'}
                    </small>
                    {availableUpdate && (
                      <>
                        <span>
                          {availableUpdate.version}
                          {' — '}
                          {availableUpdate.classification}
                          {availableUpdate.sizeBytes
                            ? ` · ${size(availableUpdate.sizeBytes)}`
                            : ''}
                        </span>
                        {availableUpdate.notes && (
                          <p>{availableUpdate.notes}</p>
                        )}
                      </>
                    )}
                    {updateProgress && (
                      <span>
                        Downloaded {size(updateProgress.downloaded)}
                        {updateProgress.total
                          ? ` of ${size(updateProgress.total)}`
                          : ''}
                      </span>
                    )}
                  </div>
                  <div className="info-card">
                    <strong>Latest approved public release</strong>
                    <span>
                      Check the permanent public GitHub Releases page.
                    </span>
                    <small>
                      Public installers are separate from signed
                      automatic-update payloads and may show a Windows
                      SmartScreen warning.
                    </small>
                  </div>
                  <div className="scan-actions">
                    <button
                      disabled={
                        checkingUpdate || !updaterConfiguration?.configured
                      }
                      title={
                        updaterConfiguration?.configured === false
                          ? 'This build has no configured update-signing key'
                          : undefined
                      }
                      onClick={checkForUpdates}
                    >
                      {updateProgress
                        ? 'Update in progress…'
                        : checkingUpdate
                          ? 'Checking…'
                          : 'Check for updates'}
                    </button>
                    <button
                      className="primary"
                      disabled={
                        checkingUpdate ||
                        !availableUpdate ||
                        updateBlockingJobs.length > 0 ||
                        backupBusy ||
                        updateBlockingBackupJobs.length > 0 ||
                        discoveryStatus.running
                      }
                      title={
                        updateBlockingJobs.length
                          ? 'Active or recoverable ingest jobs must be resolved before updating'
                          : backupBusy || updateBlockingBackupJobs.length
                            ? 'Active backup, verification, or restore work must finish before updating'
                            : discoveryStatus.running
                              ? 'The active Library scan must finish before updating'
                              : undefined
                      }
                      onClick={installUpdate}
                    >
                      Download, verify and install
                    </button>
                    <button onClick={() => void openSupport('releases')}>
                      GitHub Releases
                    </button>
                    <button onClick={() => void openSupport('test-builds')}>
                      View available test builds
                    </button>
                  </div>
                  {updaterConfiguration?.configured ? (
                    <small>
                      Only the selected approved channel manifest and artifacts
                      with a valid Tauri signature can install.
                    </small>
                  ) : (
                    <small>
                      Automatic updates are unavailable in this build. Upgrade
                      manually using a reviewed installer from GitHub Releases.
                      Pull-request test builds are unsigned development
                      artifacts and are not approved updates.
                    </small>
                  )}
                </>
              )}

              {activeSettingsSection === 'About & Support' && (
                <>
                  <div className="info-card">
                    <strong>Kairix PIMP Light</strong>
                    <span>Production Ingest &amp; Media Processor</span>
                    <span>
                      Version {applicationInfo?.displayVersion ?? 'unknown'} ·{' '}
                      {applicationInfo?.releaseClassification ?? 'Development'}
                    </span>
                    <span>
                      Build {applicationInfo?.buildCommit ?? 'unknown'} ·{' '}
                      {applicationInfo?.releaseDate ?? 'unpublished'}
                    </span>
                    <p>
                      Complete non-AI media ingest and catalogue edition. The
                      future Full AI edition is not included.
                    </p>
                  </div>
                  <div className="support-links">
                    <button onClick={() => void openSupport('github')}>
                      <Github size={18} /> GitHub
                    </button>
                    <button onClick={() => void openSupport('coffee')}>
                      <Coffee size={18} opacity={0.72} /> Buy Me a Coffee
                    </button>
                    <button onClick={() => void openSupport('website')}>
                      Project website
                    </button>
                  </div>
                  <small>
                    Open-source acknowledgements and licence details are
                    distributed with the repository.
                  </small>
                </>
              )}

              {activeSettingsSection === 'Diagnostics' && (
                <>
                  <dl className="diagnostic-grid">
                    <div>
                      <dt>FFmpeg</dt>
                      <dd>
                        {mediaTools?.ffmpegAvailable
                          ? 'Available'
                          : 'Unavailable'}
                      </dd>
                    </div>
                    <div>
                      <dt>FFprobe</dt>
                      <dd>
                        {mediaTools?.ffprobeAvailable
                          ? 'Available'
                          : 'Unavailable'}
                      </dd>
                    </div>
                    <div>
                      <dt>Device identifier</dt>
                      <dd>{settings.deviceId}</dd>
                    </div>
                    <div>
                      <dt>Last indexing result</dt>
                      <dd>
                        {discoveryStatus.registered} registered ·{' '}
                        {discoveryStatus.pending} pending ·{' '}
                        {discoveryStatus.errors} errors
                      </dd>
                    </div>
                    <div>
                      <dt>Recoverable jobs</dt>
                      <dd>
                        {jobs.filter((job) => job.state !== 'completed').length}
                      </dd>
                    </div>
                    <div>
                      <dt>Thumbnail source batches</dt>
                      <dd>{thumbnailSourceRequestCount}</dd>
                    </div>
                    <div>
                      <dt>Static thumbnails in current scope</dt>
                      <dd>
                        {
                          filtered.filter(
                            (item) => thumbnailSources[item.id] != null,
                          ).length
                        }{' '}
                        ready ·{' '}
                        {
                          filtered.filter(
                            (item) =>
                              item.available &&
                              thumbnailSources[item.id] == null &&
                              enrichmentVideoType(item.fileType),
                          ).length
                        }{' '}
                        missing
                      </dd>
                    </div>
                    <div>
                      <dt>Active native workers</dt>
                      <dd>{activeBackgroundOperations.length}</dd>
                    </div>
                    <div>
                      <dt>Resource profile</dt>
                      <dd>
                        {resourceStatus?.profile ?? settings.resourceProfile}
                        {resourceStatus?.backgroundPaused
                          ? ' · background paused'
                          : ''}
                      </dd>
                    </div>
                    <div>
                      <dt>Volume admission</dt>
                      <dd>
                        {resourceStatus
                          ? `${resourceStatus.volumes.reduce((total, volume) => total + volume.activeReaders, 0)} reader(s) · ${resourceStatus.volumes.reduce((total, volume) => total + volume.activeWriters, 0)} writer(s) · ${resourceStatus.queuedWorkers} waiting`
                          : 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>Volume bytes</dt>
                      <dd>
                        {resourceStatus
                          ? `${size(resourceStatus.volumes.reduce((total, volume) => total + volume.bytesRead, 0))} read · ${size(resourceStatus.volumes.reduce((total, volume) => total + volume.bytesWritten, 0))} written`
                          : 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>Admission wait</dt>
                      <dd>
                        {resourceStatus
                          ? `${resourceStatus.averageWaitMillis} ms average · ${resourceStatus.waiting[0]?.reason ?? 'No queued work'}`
                          : 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>Volume I/O errors</dt>
                      <dd>
                        {resourceStatus
                          ? resourceStatus.volumes.reduce(
                              (total, volume) => total + volume.ioErrors,
                              0,
                            ) || 'None reported'
                          : 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>System activity evidence</dt>
                      <dd>
                        {resourceStatus
                          ? `CPU ${resourceStatus.systemActivity.cpuLoadPercent?.toFixed(1) ?? '—'}% · input idle ${resourceStatus.systemActivity.inputIdleSeconds ?? '—'}s · ${size(resourceStatus.systemActivity.availableMemoryBytes ?? 0)} memory available`
                          : 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>Idle processing</dt>
                      <dd>
                        {resourceStatus?.systemActivity.reason ?? 'Loading…'}
                      </dd>
                    </div>
                    <div>
                      <dt>Static-thumbnail lane</dt>
                      <dd>
                        {staticThumbnailQueueStatus.active
                          ? '1 active'
                          : 'Idle'}{' '}
                        · {staticThumbnailQueueStatus.queued} queued
                      </dd>
                    </div>
                    <div>
                      <dt>Hover-preview lane</dt>
                      <dd>
                        {previewQueueStatus.active ? '1 active' : 'Idle'} ·{' '}
                        {previewQueueStatus.queued} queued
                      </dd>
                    </div>
                    <div>
                      <dt>Active FFmpeg work</dt>
                      <dd>
                        {
                          activeBackgroundOperations.filter((operation) =>
                            [
                              'static-thumbnail',
                              'hover-preview',
                              'playback-proxy',
                              'media-enrichment',
                            ].includes(operation.kind),
                          ).length
                        }{' '}
                        reported workers
                      </dd>
                    </div>
                    <div>
                      <dt>Current operation</dt>
                      <dd>
                        {primaryBackgroundOperation
                          ? `${primaryBackgroundOperation.title} · ${primaryBackgroundOperation.stage} · ${primaryOperationElapsedSeconds}s elapsed`
                          : 'Idle'}
                      </dd>
                    </div>
                    <div>
                      <dt>Measured throughput</dt>
                      <dd>
                        {primaryOperationThroughput
                          ? `${size(primaryOperationThroughput)}/s`
                          : 'Unavailable for the current operation'}
                      </dd>
                    </div>
                    <div>
                      <dt>Preview caches</dt>
                      <dd>
                        {previewMemoryCount} loaded · {size(previewMemoryUsage)}
                        {' RAM / '}
                        {size(
                          settings.previewRamCacheLimitBytes ||
                            128 * 1024 * 1024,
                        )}{' '}
                        budget · {previewStoredIds.size} stored ·{' '}
                        {size(previewCacheUsage)} disk /{' '}
                        {size(settings.previewCacheLimitBytes)} budget
                      </dd>
                    </div>
                    <div>
                      <dt>Last playback timings</dt>
                      <dd>
                        Resolve {viewerPlaybackMetrics.sourceResolvedMs ?? '—'}{' '}
                        ms · metadata {viewerPlaybackMetrics.metadataMs ?? '—'}{' '}
                        ms · first frame{' '}
                        {viewerPlaybackMetrics.firstFrameMs ?? '—'} ms · playing{' '}
                        {viewerPlaybackMetrics.playingMs ?? '—'} ms · seek{' '}
                        {viewerPlaybackMetrics.lastSeekMs ?? '—'} ms
                        {viewerPlaybackMetrics.proxyGenerationMs != null
                          ? ` · proxy ${viewerPlaybackMetrics.proxyGenerationMs} ms`
                          : ''}
                      </dd>
                    </div>
                    <div>
                      <dt>Recent processing errors</dt>
                      <dd>
                        {backgroundOperations.filter(
                          (operation) => operation.error,
                        ).length || 'None reported'}
                      </dd>
                    </div>
                  </dl>
                  <div className="scan-actions">
                    <button
                      onClick={() =>
                        void navigator.clipboard.writeText(settings.deviceId)
                      }
                    >
                      Copy device ID
                    </button>
                    <button onClick={() => void copyDiagnosticSummary()}>
                      Copy diagnostic summary
                    </button>
                    <button onClick={() => void desktopBridge.openLogsFolder()}>
                      Open logs folder
                    </button>
                    <button onClick={() => void reloadInterface()}>
                      Reload interface
                    </button>
                  </div>
                  <small>
                    Diagnostic summaries omit configured roots and personal file
                    lists. Reloading the interface does not alter catalogue
                    data.
                  </small>
                </>
              )}
            </div>
          </section>
        ) : view === 'Library' ? (
          <>
            <section className="toolbar">
              <label className="search">
                <Search size={17} />
                <input
                  ref={librarySearchRef}
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search filenames, folders, tags, notes, projects, dates, codec…"
                />
              </label>
              <div className="library-filter-combo">
                <SearchableComboBox
                  label="Filter by tags"
                  mode="multiple"
                  options={knownTags.map((tag) => ({ id: tag, label: tag }))}
                  selectedIds={tagFilters}
                  onSelectionChange={setTagFilters}
                  placeholder="Filter tags…"
                />
              </div>
              <label
                className="availability-toggle"
                title="Unavailable means not present on this device; it does not mean safely archived"
              >
                <input
                  type="checkbox"
                  role="switch"
                  checked={includeUnavailable}
                  onChange={(event) =>
                    setIncludeUnavailable(event.target.checked)
                  }
                />
                Include unavailable
              </label>
              <button
                disabled={busy || discoveryStatus.running}
                onClick={doRescan}
              >
                <RefreshCw size={15} />
                {discoveryStatus.running
                  ? discoveryStatus.totalFiles > 0
                    ? `Scanning ${discoveryStatus.filesProcessed}/${discoveryStatus.totalFiles}`
                    : `Scanning · ${discoveryStatus.filesProcessed} processed`
                  : 'Quick inventory (no content reads)'}
              </button>
            </section>
            {viewerRecording && (
              <section
                className={`clip-viewer ${
                  viewerRecording.technical.height &&
                  viewerRecording.technical.width &&
                  viewerRecording.technical.height >
                    viewerRecording.technical.width
                    ? 'portrait'
                    : 'landscape'
                }`}
                aria-label={`Clip viewer for ${viewerRecording.originalFilename}`}
              >
                <header className="clip-viewer-header">
                  <button type="button" onClick={closeClipViewer}>
                    <ArrowLeft aria-hidden="true" size={15} /> Back to Library
                  </button>
                  <div>
                    <strong>{viewerRecording.originalFilename}</strong>
                    <span>{viewerRecording.relativePath}</span>
                  </div>
                  <nav aria-label="Clip viewer navigation">
                    <button
                      type="button"
                      disabled={viewerIndex <= 0}
                      onClick={() => navigateClipViewer(-1)}
                    >
                      Previous
                    </button>
                    <button
                      type="button"
                      disabled={
                        viewerIndex < 0 || viewerIndex >= filtered.length - 1
                      }
                      onClick={() => navigateClipViewer(1)}
                    >
                      Next
                    </button>
                  </nav>
                </header>
                <div className="clip-viewer-upper">
                  <div className="clip-player">
                    {!viewerRecording.available ? (
                      <div className="clip-player-state" role="status">
                        <strong>Media unavailable locally</strong>
                        <span>
                          Unavailable means the file is not present on this
                          device; it does not mean archived or deleted.
                        </span>
                      </div>
                    ) : viewerError ? (
                      <div className="clip-player-state" role="alert">
                        <strong>Preview unavailable</strong>
                        <span>{viewerError}</span>
                        <button
                          type="button"
                          disabled={
                            viewerProxyBusy || !mediaTools?.ffmpegAvailable
                          }
                          onClick={() => void createPlaybackProxy()}
                        >
                          {!mediaTools?.ffmpegAvailable
                            ? 'FFmpeg is required for a compatible preview'
                            : viewerProxyBusy
                              ? 'Creating compatible preview…'
                              : 'Create compatible preview'}
                        </button>
                      </div>
                    ) : viewerSource ? (
                      <video
                        ref={viewerVideoRef}
                        src={viewerSource}
                        controls
                        preload="metadata"
                        playsInline
                        aria-label={`Playback ${viewerRecording.originalFilename}`}
                        onLoadedMetadata={() =>
                          setViewerPlaybackMetrics((metrics) => ({
                            ...metrics,
                            metadataMs:
                              metrics.metadataMs ??
                              Math.round(
                                performance.now() -
                                  viewerPlaybackStartedAt.current,
                              ),
                          }))
                        }
                        onLoadedData={() =>
                          setViewerPlaybackMetrics((metrics) => ({
                            ...metrics,
                            firstFrameMs:
                              metrics.firstFrameMs ??
                              Math.round(
                                performance.now() -
                                  viewerPlaybackStartedAt.current,
                              ),
                          }))
                        }
                        onPlaying={() =>
                          setViewerPlaybackMetrics((metrics) => ({
                            ...metrics,
                            playingMs:
                              metrics.playingMs ??
                              Math.round(
                                performance.now() -
                                  viewerPlaybackStartedAt.current,
                              ),
                          }))
                        }
                        onSeeking={() => {
                          viewerSeekStartedAt.current = performance.now();
                        }}
                        onSeeked={() =>
                          setViewerPlaybackMetrics((metrics) => ({
                            ...metrics,
                            lastSeekMs: Math.round(
                              performance.now() - viewerSeekStartedAt.current,
                            ),
                          }))
                        }
                        onError={() =>
                          setViewerError(
                            'WebView2 could not play this file or codec. The original media was not changed.',
                          )
                        }
                      />
                    ) : (
                      <div className="clip-player-state" role="status">
                        Loading local preview…
                      </div>
                    )}
                  </div>
                  <aside className="clip-metadata" aria-label="Clip metadata">
                    <h3>Recording details</h3>
                    <dl>
                      <div>
                        <dt>Resolution</dt>
                        <dd>
                          {viewerRecording.technical.width &&
                          viewerRecording.technical.height
                            ? `${viewerRecording.technical.width} × ${viewerRecording.technical.height}`
                            : 'Unknown'}
                        </dd>
                      </div>
                      <div>
                        <dt>Frame rate</dt>
                        <dd>
                          {viewerRecording.technical.frameRate
                            ? `${viewerRecording.technical.frameRate} fps`
                            : 'Unknown'}
                        </dd>
                      </div>
                      <div>
                        <dt>Codec</dt>
                        <dd>{viewerRecording.technical.codec ?? 'Unknown'}</dd>
                      </div>
                      <div>
                        <dt>Duration</dt>
                        <dd>
                          {duration(viewerRecording.technical.durationSeconds)}
                        </dd>
                      </div>
                      <div>
                        <dt>Size</dt>
                        <dd>{size(viewerRecording.technical.sizeBytes)}</dd>
                      </div>
                      <div>
                        <dt>Integrity</dt>
                        <dd>
                          {viewerRecording.needsReview
                            ? 'Needs review'
                            : viewerRecording.available
                              ? 'Verified locally'
                              : 'Unavailable locally'}
                        </dd>
                      </div>
                    </dl>
                    <div className="clip-associated">
                      <strong>
                        {viewerAssociatedMedia.length} associated physical file
                        {viewerAssociatedMedia.length === 1 ? '' : 's'}
                      </strong>
                      {viewerAssociatedMedia.map((recording) => (
                        <div key={recording.id}>
                          <span>{recording.originalFilename}</span>
                          <button
                            type="button"
                            disabled={!recording.available}
                            onClick={() =>
                              void runSelectedFileAction('reveal', recording)
                            }
                          >
                            Reveal
                          </button>
                        </div>
                      ))}
                    </div>
                  </aside>
                </div>
                <div className="clip-editor">
                  <label className="clip-description">
                    Description
                    <textarea
                      value={descriptionValue}
                      onChange={(event) =>
                        setDescriptionDraft({
                          selectionKey,
                          value: event.target.value,
                        })
                      }
                      placeholder="Describe this recording"
                    />
                    <button
                      type="button"
                      disabled={
                        busy || visibleDescriptionStatus.state === 'saving'
                      }
                      onClick={() => void saveSelectionDescription()}
                    >
                      {visibleDescriptionStatus.state === 'saving'
                        ? 'Saving…'
                        : 'Save description'}
                    </button>
                    <span role="status">
                      {visibleDescriptionStatus.state === 'saved'
                        ? 'Saved'
                        : visibleDescriptionStatus.state === 'failed'
                          ? `Failed: ${visibleDescriptionStatus.detail}`
                          : ''}
                    </span>
                  </label>
                  <div className="clip-editor-field">
                    <SearchableComboBox
                      label="Add tags"
                      mode="multiple"
                      options={knownTags.map((tag) => ({
                        id: tag,
                        label: tag,
                        description: selectedTagState(tag),
                      }))}
                      selectedIds={[]}
                      onSelectionChange={(ids) =>
                        void updateSelection({ addTags: ids })
                      }
                      placeholder="Search or create tags…"
                      onCreate={(label) => ({ id: label, label })}
                    />
                    <div className="clip-chips">
                      {logicalTags(viewerRecording, catalogue.media).map(
                        (tag) => (
                          <button
                            type="button"
                            key={tag}
                            aria-label={`Remove tag ${tag}`}
                            onClick={() =>
                              void updateSelection({ removeTags: [tag] })
                            }
                          >
                            {tag} ×
                          </button>
                        ),
                      )}
                    </div>
                  </div>
                  <div className="clip-editor-field">
                    <SearchableComboBox
                      label="Assign projects"
                      mode="multiple"
                      options={catalogue.projects.map((project) => ({
                        id: project.id,
                        label: project.name,
                      }))}
                      selectedIds={logicalProjectIds(
                        viewerRecording,
                        catalogue.media,
                      )}
                      onSelectionChange={(ids) => {
                        const current = logicalProjectIds(
                          viewerRecording,
                          catalogue.media,
                        );
                        void updateSelection({
                          addProjectIds: ids.filter(
                            (id) => !current.includes(id),
                          ),
                          removeProjectIds: current.filter(
                            (id) => !ids.includes(id),
                          ),
                        });
                      }}
                      placeholder="Search projects…"
                    />
                    <button
                      type="button"
                      className="clip-favourite"
                      onClick={() =>
                        void updateSelection({
                          favourite: !viewerRecording.favourite,
                        })
                      }
                    >
                      <Star
                        aria-hidden="true"
                        size={14}
                        fill={
                          viewerRecording.favourite ? 'currentColor' : 'none'
                        }
                      />{' '}
                      {viewerRecording.favourite
                        ? 'Remove favourite'
                        : 'Favourite'}
                    </button>
                  </div>
                  <section className="analysis-placeholder">
                    <strong>Analysis</strong>
                    <span>
                      Transcript, summary and observations are unavailable in
                      the Light edition. No analysis result is fabricated.
                    </span>
                  </section>
                  <section className="thumbnail-frame-control">
                    <strong>Thumbnail</strong>
                    <button
                      type="button"
                      disabled={!viewerSource || Boolean(viewerError)}
                      onClick={() => void saveCurrentViewerFrame()}
                    >
                      Use current video frame
                    </button>
                    <span role="status">{thumbnailFrameStatus}</span>
                  </section>
                </div>
              </section>
            )}
            <section
              className={`content-grid ${viewerRecording ? 'library-hidden' : ''}`}
            >
              <div className="folder-panel">
                <div className="folder-panel-heading">
                  <h3>Folders</h3>
                  <label>
                    <span>Order</span>
                    <select
                      aria-label="Folder order"
                      value={settings.libraryFolderSort ?? 'newest'}
                      onChange={(event) =>
                        setFolderOrdering(event.target.value as FolderSort)
                      }
                    >
                      <option value="newest">Newest first</option>
                      <option value="oldest">Oldest first</option>
                      <option value="name-asc">Name A–Z</option>
                      <option value="name-desc">Name Z–A</option>
                      <option value="most-recordings">Most recordings</option>
                      <option value="fewest-recordings">
                        Fewest recordings
                      </option>
                      <option value="custom">Custom order</option>
                    </select>
                  </label>
                </div>
                <button
                  className={`folder ${folder === 'All media' ? 'active' : ''}`}
                  onClick={() => setFolder('All media')}
                >
                  <Folder size={16} />
                  <span>
                    All media<small>{displayedMedia.length} recordings</small>
                  </span>
                </button>
                {folders.map(({ name, count }, index) => (
                  <div className="folder-row" key={name}>
                    <button
                      className={`folder ${folder === name ? 'active' : ''}`}
                      data-native-drag={native ? 'true' : undefined}
                      draggable={false}
                      title="Drag this physical folder to another Windows application"
                      onDragStart={(event) => event.preventDefault()}
                      onPointerDown={(event) =>
                        beginNativeDrag(
                          event,
                          folderDrag(name === 'Master Bin root' ? '' : name),
                        )
                      }
                      onPointerMove={continueNativeDrag}
                      onPointerUp={cancelPendingNativeDrag}
                      onPointerCancel={cancelPendingNativeDrag}
                      onLostPointerCapture={cancelPendingNativeDrag}
                      onClick={(event) => {
                        if (
                          suppressClickAfterNativeDrag(
                            `folder:${name === 'Master Bin root' ? '' : name}`,
                          )
                        ) {
                          event.preventDefault();
                          return;
                        }
                        setFolder(name);
                      }}
                    >
                      <Folder size={16} />
                      <span>
                        {name}
                        <small>
                          {count} recording{count === 1 ? '' : 's'}
                        </small>
                      </span>
                    </button>
                    {(settings.libraryFolderSort ?? 'newest') === 'custom' && (
                      <div className="folder-move" aria-label={`Move ${name}`}>
                        <button
                          type="button"
                          aria-label={`Move ${name} up`}
                          title="Move folder up"
                          disabled={index === 0}
                          onClick={() => moveFolder(name, -1)}
                        >
                          <ChevronUp aria-hidden="true" size={13} />
                        </button>
                        <button
                          type="button"
                          aria-label={`Move ${name} down`}
                          title="Move folder down"
                          disabled={index === folders.length - 1}
                          onClick={() => moveFolder(name, 1)}
                        >
                          <ChevronDown aria-hidden="true" size={13} />
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
              <div
                className="library-panel"
                ref={libraryPanelRef}
                onScroll={(event) =>
                  setShowScrollTop(event.currentTarget.scrollTop >= 480)
                }
              >
                <div className="section-head">
                  <div>
                    <h2>Media library</h2>
                    <span>{filtered.length} persisted items</span>
                  </div>
                  <label className="thumbnail-size-control">
                    <span>Thumbnail size</span>
                    <input
                      type="range"
                      min="160"
                      max="360"
                      step="20"
                      value={settings.libraryThumbnailWidth}
                      onChange={(event) =>
                        void persistPreference({
                          libraryThumbnailWidth: Number(event.target.value),
                        })
                      }
                    />
                    <output>{settings.libraryThumbnailWidth}px</output>
                  </label>
                </div>
                {!filtered.length ? (
                  <div className="empty">
                    <h2>
                      {catalogue.media.length
                        ? 'No matching media'
                        : 'Your Master Bin is ready'}
                    </h2>
                    <p>
                      {catalogue.media.length
                        ? 'Clear the search or choose another folder.'
                        : 'Ingest files and folders, or copy media into Master Bin and choose Rescan.'}
                    </p>
                    <button
                      className="primary"
                      onClick={() => setView('Ingest')}
                    >
                      Start an ingest
                    </button>
                  </div>
                ) : (
                  <div
                    className="cards"
                    style={
                      {
                        '--library-thumbnail-width': `${settings.libraryThumbnailWidth}px`,
                      } as CSSProperties
                    }
                    role="listbox"
                    aria-label="Media library"
                    aria-multiselectable="true"
                  >
                    {filtered.map((item) => (
                      <article
                        className={`${selected.includes(item.id) ? 'selected' : ''} ${libraryLocateId === item.id ? 'located' : ''}`.trim()}
                        key={item.id}
                        ref={(element) => {
                          if (element)
                            libraryCardRefs.current.set(item.id, element);
                          else libraryCardRefs.current.delete(item.id);
                        }}
                        role="option"
                        data-media-id={item.id}
                        data-native-drag={
                          native && item.available ? 'true' : undefined
                        }
                        draggable={false}
                        title={
                          item.available
                            ? 'Drag the original file to another Windows application'
                            : undefined
                        }
                        tabIndex={0}
                        aria-selected={selected.includes(item.id)}
                        onDragStart={(event) => event.preventDefault()}
                        onPointerDown={(event) => {
                          if (item.available)
                            beginNativeDrag(event, mediaDrag(item));
                        }}
                        onPointerMove={continueNativeDrag}
                        onPointerUp={cancelPendingNativeDrag}
                        onPointerCancel={cancelPendingNativeDrag}
                        onLostPointerCapture={cancelPendingNativeDrag}
                        onMouseDown={(event) => {
                          const target = event.target as HTMLElement;
                          if (
                            event.shiftKey &&
                            !target.closest(
                              'input, textarea, [contenteditable="true"]',
                            )
                          )
                            event.preventDefault();
                        }}
                        onClick={(event) => {
                          if (
                            suppressClickAfterNativeDrag(`media:${item.id}`)
                          ) {
                            event.preventDefault();
                            return;
                          }
                          setContextMenu(undefined);
                          selectRecording(item.id, {
                            toggle: event.ctrlKey || event.metaKey,
                            range: event.shiftKey,
                          });
                        }}
                        onDoubleClick={() => openClipViewer(item.id)}
                        onKeyDown={(event) => {
                          if (
                            [
                              'ArrowLeft',
                              'ArrowUp',
                              'ArrowRight',
                              'ArrowDown',
                            ].includes(event.key)
                          ) {
                            event.preventDefault();
                            const offset =
                              event.key === 'ArrowLeft' ||
                              event.key === 'ArrowUp'
                                ? -1
                                : 1;
                            const index = filtered.findIndex(
                              (candidate) => candidate.id === item.id,
                            );
                            const target = filtered[index + offset];
                            if (!target) return;
                            libraryCardRefs.current.get(target.id)?.focus();
                            if (event.shiftKey)
                              selectRecording(target.id, {
                                toggle: true,
                                range: true,
                              });
                            return;
                          }
                          if (event.key === 'Enter') {
                            event.preventDefault();
                            openClipViewer(item.id);
                            return;
                          }
                          if (event.key.toLowerCase() === 'k') {
                            event.preventDefault();
                            openClipViewer(item.id);
                            window.requestAnimationFrame(
                              () => void viewerVideoRef.current?.play(),
                            );
                            return;
                          }
                          if (event.key === ' ') {
                            event.preventDefault();
                            selectRecording(item.id, {
                              toggle: true,
                              range: event.shiftKey,
                            });
                          }
                        }}
                        onContextMenu={(event) => {
                          event.preventDefault();
                          if (!selected.includes(item.id)) {
                            selectRecording(item.id, {
                              toggle: false,
                              range: false,
                            });
                          }
                          setContextMenu({
                            mediaId: item.id,
                            x: event.clientX,
                            y: event.clientY,
                          });
                        }}
                      >
                        <div
                          className={`thumb ${!item.available ? 'unavailable' : ''}`}
                        >
                          <MediaThumbnail
                            mediaId={item.id}
                            fileType={item.fileType}
                            thumbnailSource={thumbnailSources[item.id]}
                            sourceAvailable={item.available}
                            previewStored={previewStoredIds.has(item.id)}
                            onPreviewPrepared={receivePreparedPreview}
                            onThumbnailPrepared={receivePreparedThumbnail}
                            previewKey={`${item.id}:${item.sha256}:${item.observedSize ?? ''}:${item.observedModifiedMillis ?? ''}`}
                            previewCacheEpoch={previewCacheEpoch}
                            enableHoverPreview={
                              (settings.hoverPreviewsEnabled ?? true) &&
                              settings.previewPreloading !== 'off' &&
                              !clearingPreviewCache
                            }
                            holdFrame={settings.thumbnailHoldFrame ?? 'first'}
                            preloading={
                              settings.previewPreloading ?? 'visible-nearby'
                            }
                            allowBackgroundPreparation={activeJobs.length === 0}
                          />
                          <input
                            className="media-select"
                            type="checkbox"
                            draggable={false}
                            checked={selected.includes(item.id)}
                            aria-label={`Select ${item.originalFilename}`}
                            onPointerDown={(event) => event.stopPropagation()}
                            onMouseDown={(event) => event.stopPropagation()}
                            onClick={(event) => event.stopPropagation()}
                            onKeyDown={(event) => event.stopPropagation()}
                            onChange={() => {
                              setContextMenu(undefined);
                              selectRecording(item.id, {
                                toggle: true,
                                range: false,
                              });
                            }}
                          />
                          <span>
                            {duration(item.technical.durationSeconds)}
                          </span>
                        </div>
                        <div className="card-body">
                          <strong>{item.originalFilename}</strong>
                          {item.favourite && (
                            <span className="favourite" aria-label="Favourite">
                              <Star size={13} fill="currentColor" /> Favourite
                            </span>
                          )}
                          <small>{item.relativePath}</small>
                          <div className="pills">
                            {logicalTags(item, catalogue.media).map((tag) => (
                              <span className="tag-chip" key={tag}>
                                <button
                                  type="button"
                                  title={`Filter Library by ${tag}`}
                                  onClick={(event) => event.stopPropagation()}
                                  onContextMenu={(event) => {
                                    event.preventDefault();
                                    event.stopPropagation();
                                    setTagFilters((current) =>
                                      current.includes(tag)
                                        ? current
                                        : [...current, tag],
                                    );
                                  }}
                                >
                                  {tag}
                                </button>
                                <button
                                  type="button"
                                  aria-label={`Remove tag ${tag} from ${item.originalFilename}`}
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    void updateRecording(item.id, {
                                      removeTags: [tag],
                                    });
                                  }}
                                >
                                  ×
                                </button>
                              </span>
                            ))}
                          </div>
                          <div className="projects">
                            {logicalProjectIds(item, catalogue.media).map(
                              (id) => {
                                const project = catalogue.projects.find(
                                  (candidate) => candidate.id === id,
                                );
                                return (
                                  <button
                                    type="button"
                                    key={id}
                                    onMouseDown={(event) =>
                                      event.stopPropagation()
                                    }
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      if (project) {
                                        setView('Projects');
                                        openProject(project);
                                      }
                                    }}
                                  >
                                    {project?.name ?? id}
                                  </button>
                                );
                              },
                            )}
                          </div>
                          <footer>
                            <span>
                              <Clock3 size={12} />
                              {item.captureDate?.slice(0, 10) ?? 'Date unknown'}
                            </span>
                            <span>
                              {item.attachedAudioId ? (
                                <>
                                  <Volume2 size={12} /> 2 associated files
                                  ·{' '}
                                </>
                              ) : item.technical.hasAudio === false ? (
                                <VolumeX size={12} />
                              ) : null}
                              {size(item.technical.sizeBytes)} ·{' '}
                              {item.available
                                ? 'Available locally'
                                : 'Synchronizing / unavailable locally'}
                            </span>
                          </footer>
                        </div>
                      </article>
                    ))}
                  </div>
                )}
              </div>
            </section>
            <div
              className={`library-overlays ${viewerRecording ? 'library-hidden' : ''}`}
            >
              {showScrollTop && (
                <button
                  type="button"
                  className="scroll-top"
                  aria-label="Scroll Library to top"
                  title="Scroll to top"
                  onClick={() =>
                    libraryPanelRef.current?.scrollTo({
                      top: 0,
                      behavior: window.matchMedia?.(
                        '(prefers-reduced-motion: reduce)',
                      ).matches
                        ? 'auto'
                        : 'smooth',
                    })
                  }
                >
                  <ArrowUp aria-hidden="true" size={18} />
                </button>
              )}
              {selected.length > 0 && (
                <div
                  className={`batch selection-toolbar ${selectionToolbarPosition}`}
                  role="toolbar"
                  aria-label="Selected recordings"
                >
                  <div className="batch-heading">
                    <strong>{selected.length} selected</strong>
                    <span>Changes persist immediately</span>
                  </div>
                  {selectionToolbarActions.includes('tags') && (
                    <div className="batch-combo compact-batch-control">
                      <Tag size={15} />
                      <SearchableComboBox
                        label="Add tags"
                        mode="multiple"
                        options={knownTags.map((tag) => ({
                          id: tag,
                          label: tag,
                          description: selectedTagState(tag),
                        }))}
                        selectedIds={batchTags}
                        onSelectionChange={(ids) => {
                          setBatchTags(ids);
                          void updateSelection({ addTags: ids });
                        }}
                        placeholder="Add tags…"
                        onCreate={(label) => ({ id: label, label })}
                      />
                    </div>
                  )}
                  {selectionToolbarActions.includes('projects') && (
                    <div className="batch-combo compact-batch-control">
                      <UsersRound size={15} />
                      <SearchableComboBox
                        label="Assign projects"
                        mode="multiple"
                        options={catalogue.projects.map((project) => ({
                          id: project.id,
                          label: project.name,
                        }))}
                        selectedIds={batchProjects}
                        onSelectionChange={(ids) => {
                          setBatchProjects(ids);
                          void updateSelection({ addProjectIds: ids });
                        }}
                        placeholder="Assign project…"
                      />
                    </div>
                  )}
                  {selectionToolbarActions.includes('favourite') && (
                    <button
                      type="button"
                      className="batch-primary"
                      onClick={() => void updateSelection({ favourite: true })}
                    >
                      <Star size={13} /> Favourite
                    </button>
                  )}
                  <button
                    type="button"
                    className="batch-clear"
                    onClick={() => {
                      replaceSelection([], undefined);
                      setBatchTags([]);
                      setRemoveBatchTags([]);
                      setBatchProjects([]);
                      setRemoveBatchProjects([]);
                      setDescriptionDraft({ selectionKey: '', value: '' });
                    }}
                  >
                    Clear selection
                  </button>
                  <button
                    type="button"
                    className="batch-clear"
                    aria-expanded={showSelectionDetails}
                    onClick={() =>
                      setShowSelectionDetails((current) => !current)
                    }
                  >
                    {showSelectionDetails ? 'Fewer actions' : 'More actions'}
                  </button>
                  {showSelectionDetails && (
                    <>
                      <button
                        type="button"
                        disabled={selectedPhysicalIds.length === 0}
                        onClick={() => void exportPremiereManifest()}
                      >
                        Export Premiere manifest
                      </button>
                      <div className="batch-combo">
                        <Tag size={15} />
                        <SearchableComboBox
                          label="Remove tags"
                          mode="multiple"
                          options={knownTags.map((tag) => ({
                            id: tag,
                            label: tag,
                          }))}
                          selectedIds={removeBatchTags}
                          onSelectionChange={(ids) => {
                            setRemoveBatchTags(ids);
                            void updateSelection({ removeTags: ids });
                          }}
                          placeholder="Remove tags…"
                        />
                      </div>
                      <div className="batch-combo">
                        <UsersRound size={15} />
                        <SearchableComboBox
                          label="Remove projects"
                          mode="multiple"
                          options={catalogue.projects.map((project) => ({
                            id: project.id,
                            label: project.name,
                          }))}
                          selectedIds={removeBatchProjects}
                          onSelectionChange={(ids) => {
                            setRemoveBatchProjects(ids);
                            void updateSelection({ removeProjectIds: ids });
                          }}
                          placeholder="Remove projects…"
                        />
                      </div>
                      <label className="batch-description">
                        {selected.length === 1
                          ? `Description for ${selectedRecording?.originalFilename ?? 'recording'}`
                          : 'Replace description on all selected recordings'}
                        <input
                          value={descriptionValue}
                          onChange={(event) =>
                            setDescriptionDraft({
                              selectionKey,
                              value: event.target.value,
                            })
                          }
                          placeholder={
                            selected.length === 1
                              ? 'Describe this recording'
                              : 'Bulk replacement description'
                          }
                        />
                        {selected.length > 1 && (
                          <small>
                            Bulk replacement overwrites the description on every
                            selected physical recording.
                          </small>
                        )}
                        <button
                          className="batch-primary"
                          disabled={
                            busy || visibleDescriptionStatus.state === 'saving'
                          }
                          onClick={() => void saveSelectionDescription()}
                        >
                          {visibleDescriptionStatus.state === 'saving'
                            ? 'Saving…'
                            : selected.length === 1
                              ? 'Save description'
                              : 'Replace descriptions'}
                        </button>
                        <span
                          role="status"
                          className={
                            visibleDescriptionStatus.state === 'failed'
                              ? 'field-error'
                              : undefined
                          }
                        >
                          {visibleDescriptionStatus.state === 'saved'
                            ? 'Saved'
                            : visibleDescriptionStatus.state === 'failed'
                              ? `Failed: ${visibleDescriptionStatus.detail}`
                              : ''}
                        </span>
                      </label>
                      <div
                        className="file-actions"
                        aria-label="Selection actions"
                      >
                        <button
                          onClick={() =>
                            void updateSelection({ favourite: false })
                          }
                        >
                          Remove favourite
                        </button>
                        <button
                          disabled={selectedPhysicalIds.every(
                            (id) =>
                              !catalogue.media.find((item) => item.id === id)
                                ?.available,
                          )}
                          onClick={() => void copySelectedPaths()}
                          title="Copies newline-separated text; it does not import media"
                        >
                          <Copy size={13} /> Copy file paths
                        </button>
                        <small className="action-note">
                          File paths are copied as text only; this does not
                          import media into Premiere.
                        </small>
                      </div>
                      {selectedRecording && (
                        <>
                          <div
                            className="associated-files"
                            role="group"
                            aria-label="Associated physical files"
                          >
                            <strong>
                              {selectedAssociatedMedia.length} associated file
                              {selectedAssociatedMedia.length === 1 ? '' : 's'}
                            </strong>
                            {selectedAssociatedMedia.map((recording) => (
                              <div
                                className="associated-file"
                                key={recording.id}
                              >
                                <div>
                                  <b>{recording.originalFilename}</b>
                                  <span>
                                    {recording.fileType.toUpperCase()} ·{' '}
                                    {size(recording.technical.sizeBytes)} ·{' '}
                                    {recording.needsReview
                                      ? 'Integrity needs review'
                                      : recording.available
                                        ? 'Integrity verified'
                                        : 'Unavailable locally'}
                                  </span>
                                  <code>{recording.relativePath}</code>
                                </div>
                                <button
                                  disabled={!recording.available}
                                  aria-label={`Reveal ${recording.originalFilename} in Explorer`}
                                  onClick={() =>
                                    void runSelectedFileAction(
                                      'reveal',
                                      recording,
                                    )
                                  }
                                >
                                  <LocateFixed size={13} /> Reveal
                                </button>
                              </div>
                            ))}
                          </div>
                          <div
                            className="file-actions"
                            aria-label="Recording actions"
                          >
                            <button
                              disabled={!selectedRecording.available}
                              onClick={() =>
                                void runSelectedFileAction('reveal')
                              }
                            >
                              <LocateFixed size={13} /> Reveal in Explorer
                            </button>
                            <button
                              disabled={!selectedRecording.available}
                              onClick={() =>
                                void runSelectedFileAction('folder')
                              }
                            >
                              <FolderOpen size={13} /> Open date folder
                            </button>
                            <button
                              disabled={!selectedRecording.available}
                              onClick={() => void runSelectedFileAction('copy')}
                            >
                              <Copy size={13} /> Copy path
                            </button>
                          </div>
                        </>
                      )}
                    </>
                  )}
                </div>
              )}
            </div>
            {contextMenu && contextRecording && (
              <div
                className="context-menu"
                role="menu"
                aria-label="Recording context menu"
                style={{ left: contextMenu.x, top: contextMenu.y }}
              >
                <button
                  role="menuitem"
                  disabled={!contextRecording.available}
                  onClick={() => {
                    setContextMenu(undefined);
                    void runSelectedFileAction('reveal', contextRecording);
                  }}
                >
                  Reveal in Explorer
                </button>
                <button
                  role="menuitem"
                  disabled={!contextRecording.available}
                  onClick={() => {
                    setContextMenu(undefined);
                    void runSelectedFileAction('folder', contextRecording);
                  }}
                >
                  Open containing folder
                </button>
                <button
                  role="menuitem"
                  disabled={!contextRecording.available}
                  onClick={() => {
                    setContextMenu(undefined);
                    void copySelectedPaths();
                  }}
                >
                  Copy file paths
                </button>
                <button
                  role="menuitem"
                  onClick={() => {
                    setContextMenu(undefined);
                    void updateSelection({
                      favourite: !contextRecording.favourite,
                    });
                  }}
                >
                  {contextRecording.favourite
                    ? 'Remove favourite'
                    : 'Favourite'}
                </button>
              </div>
            )}
            {showShortcutHelp && (
              <div className="modal-backdrop" role="presentation">
                <section
                  className="shortcut-dialog"
                  role="dialog"
                  aria-modal="true"
                  aria-labelledby="shortcut-title"
                >
                  <div className="section-head">
                    <h2 id="shortcut-title">Library shortcuts</h2>
                    <button
                      type="button"
                      autoFocus
                      onClick={() => setShowShortcutHelp(false)}
                    >
                      Close
                    </button>
                  </div>
                  <dl>
                    <div>
                      <dt>Arrow keys</dt>
                      <dd>Move card focus</dd>
                    </div>
                    <div>
                      <dt>Shift + arrow</dt>
                      <dd>Extend selection</dd>
                    </div>
                    <div>
                      <dt>Ctrl + Space</dt>
                      <dd>Toggle focused recording</dd>
                    </div>
                    <div>
                      <dt>Ctrl + A</dt>
                      <dd>Select filtered recordings</dd>
                    </div>
                    <div>
                      <dt>Enter</dt>
                      <dd>Open focused recording</dd>
                    </div>
                    <div>
                      <dt>K / Space</dt>
                      <dd>Open or toggle viewer playback</dd>
                    </div>
                    <div>
                      <dt>Ctrl + F</dt>
                      <dd>Focus Library search</dd>
                    </div>
                    <div>
                      <dt>Ctrl + Z / Ctrl + Y</dt>
                      <dd>Undo or redo selection only</dd>
                    </div>
                    <div>
                      <dt>Escape twice</dt>
                      <dd>Clear selection</dd>
                    </div>
                    <div>
                      <dt>?</dt>
                      <dd>Show this reference</dd>
                    </div>
                  </dl>
                  <p>
                    Text fields keep their normal typing and undo shortcuts.
                    Selection undo never changes metadata or files.
                  </p>
                </section>
              </div>
            )}
          </>
        ) : view === 'Ingest' ? (
          <section className="ingest">
            <div className="drop">
              <button
                type="button"
                className="primary"
                disabled={!settings.sdGrabberEnabled}
                onClick={() => {
                  setShowSdGrabber((current) => !current);
                  void refreshRemovableVolumes();
                }}
              >
                <HardDrive size={18} /> Grab files from SD card
              </button>
              {showSdGrabber && (
                <div className="source-list" aria-label="SD Card Grabber">
                  <strong>SD Card Grabber</strong>
                  <small>
                    Scans only {settings.sdGrabberExtensions.join(', ')} files
                    and copies verified new recordings. Nothing is deleted from
                    the card.
                  </small>
                  {removableVolumes.map((volume) => (
                    <div className="button-row" key={volume.identity}>
                      <button
                        type="button"
                        onClick={() =>
                          scanSdSource(volume.rootPath, volume.identity)
                        }
                      >
                        Scan {volume.label} ({volume.rootPath})
                      </button>
                      <small>
                        {size(volume.availableBytes)} free of{' '}
                        {size(volume.capacityBytes)}
                        {settings.sdGrabberRecognizedDeviceIds.includes(
                          volume.identity,
                        )
                          ? ' · Recognised card'
                          : ''}
                      </small>
                    </div>
                  ))}
                  {removableVolumes.length === 0 && (
                    <small className="pending">
                      No Windows removable volume detected. Some card readers
                      appear as fixed drives; choose the card folder manually.
                    </small>
                  )}
                  <div className="button-row">
                    <button type="button" onClick={chooseSdSource}>
                      Choose card folder manually
                    </button>
                    <button type="button" onClick={refreshRemovableVolumes}>
                      Refresh devices
                    </button>
                  </div>
                </div>
              )}
              <UploadCloud size={42} />
              <h2>Drop camera media here</h2>
              <p>
                Files and recursive folders are copied to Master Bin,
                independently SHA-256 verified, and atomically promoted.
                Originals are never modified.
              </p>
              <div className="button-row">
                <button onClick={addFiles}>Choose files</button>
                <button onClick={addFolder}>Choose folder</button>
              </div>
              {sources.length ? (
                <div className="source-list">
                  <strong>
                    {sources.length} source selection
                    {sources.length === 1 ? '' : 's'}
                  </strong>
                  {sources.slice(0, 5).map((source) => (
                    <small key={source}>{source}</small>
                  ))}
                  {sources.length > 5 && (
                    <small>…and {sources.length - 5} more</small>
                  )}
                  <button
                    className="link"
                    onClick={() => {
                      setSources([]);
                      setInventory(undefined);
                      setInventoryLoading(false);
                      setInventoryError('');
                      setConflicts([]);
                      setRenames({});
                      setSdGrabberMode(false);
                      setAllowDuplicateCopies(false);
                    }}
                  >
                    Clear
                  </button>
                </div>
              ) : (
                <small className="pending">
                  Camera video, audio, stills, sidecars, and package support
                  files are inventoried before copying.
                </small>
              )}
              {inventoryLoading && (
                <small className="pending">
                  Inventorying selected sources…
                </small>
              )}
              {inventory && (
                <div className="source-list" aria-label="Source inventory">
                  <strong>
                    {sdGrabberMode
                      ? `${inventory.copyFileCount} new recording${inventory.copyFileCount === 1 ? '' : 's'} — ${size(inventory.copyBytes)}`
                      : `${inventory.copyFileCount} of ${inventory.totalFileCount} files will be copied (${size(inventory.copyBytes)})`}
                  </strong>
                  {sdGrabberMode && (
                    <>
                      <small>
                        {inventory.alreadyImportedFileCount} already imported —{' '}
                        {size(inventory.alreadyImportedBytes)}
                      </small>
                      <small
                        className={
                          inventory.attentionFileCount ? 'error' : undefined
                        }
                      >
                        {inventory.attentionFileCount} require attention —{' '}
                        {size(inventory.attentionBytes)}
                      </small>
                    </>
                  )}
                  {!sdGrabberMode &&
                    (inventory.alreadyImportedFileCount ?? 0) > 0 && (
                      <small>
                        {inventory.alreadyImportedFileCount} verified global
                        duplicate
                        {inventory.alreadyImportedFileCount === 1
                          ? ''
                          : 's'}{' '}
                        will be skipped ({size(inventory.alreadyImportedBytes)})
                      </small>
                    )}
                  <small>
                    {
                      inventory.files.filter((file) => file.preserveHierarchy)
                        .length
                    }{' '}
                    preserve their camera-folder path;{' '}
                    {
                      inventory.files.filter((file) => !file.analysisSupported)
                        .length
                    }{' '}
                    will be safely retained without technical analysis.
                  </small>
                  <details>
                    <summary>
                      Review {inventory.copyFileCount} planned destination path
                      {inventory.copyFileCount === 1 ? '' : 's'}
                    </summary>
                    <div
                      className="planned-paths"
                      role="table"
                      aria-label="Planned ingest destination paths"
                    >
                      <div className="planned-paths-head" role="row">
                        <strong role="columnheader">Source</strong>
                        <strong role="columnheader">Destination</strong>
                      </div>
                      {inventory.files.map((file) => (
                        <div
                          className="planned-path-row"
                          role="row"
                          key={file.sourcePath}
                        >
                          <span role="cell">
                            <b>Source</b>
                            <code title={file.sourceRelativePath}>
                              {file.sourceRelativePath}
                            </code>
                          </span>
                          <span role="cell">
                            <b>Destination</b>
                            <code title={file.plannedRelativePath}>
                              {file.disposition === 'already-imported'
                                ? `Already imported: ${file.existingRelativePath}`
                                : file.plannedRelativePath}
                            </code>
                            {file.detail && <small>{file.detail}</small>}
                          </span>
                        </div>
                      ))}
                    </div>
                  </details>
                  {inventory.excluded.map((item) => (
                    <small key={item.path}>
                      Excluded: {item.path} — {item.reason}
                    </small>
                  ))}
                  {inventory.issues.map((issue) => (
                    <small className="error" key={issue}>
                      Inventory issue: {issue}
                    </small>
                  ))}
                </div>
              )}
              {inventoryError && (
                <small className="error">{inventoryError}</small>
              )}
              {progress && (
                <progress
                  value={progress.bytesComplete}
                  max={progress.bytesTotal || 1}
                />
              )}
            </div>
            <div className="mode">
              <h3>Destination date for this ingest</h3>
              <p>
                Your persistent mode is{' '}
                <b>
                  {settings.ingestDateOrganization === 'today'
                    ? "today's date"
                    : 'original file dates'}
                </b>
                .
              </p>
              <fieldset className="ingest-date-choices">
                <legend>Choose once for this batch</legend>
                {[
                  ['today', 'Today'],
                  ['yesterday', 'Yesterday'],
                  ['choose', 'Choose date'],
                  ['original-file-dates', 'Use clip date'],
                ].map(([value, label]) => (
                  <label key={value}>
                    <input
                      type="radio"
                      name="ingest-date-choice"
                      value={value}
                      checked={dateOverride === value}
                      onChange={() => setDateOverride(value)}
                    />
                    {label}
                  </label>
                ))}
                {dateOverride === 'choose' && (
                  <label>
                    Date for this batch
                    <input
                      type="date"
                      value={chosenDate}
                      onChange={(event) => setChosenDate(event.target.value)}
                    />
                  </label>
                )}
              </fieldset>
              <SearchableComboBox
                label="Tags for all files in this ingest"
                mode="multiple"
                options={knownTags.map((tag) => ({ id: tag, label: tag }))}
                selectedIds={ingestBatchTags}
                onSelectionChange={setIngestBatchTags}
                onCreate={(label) => ({ id: label, label })}
                placeholder="Search or create batch tags…"
              />
              <small>
                These tags are added to eligible video, audio and image
                recordings. You can still add file-specific tags while the job
                is running.
              </small>
              {conflicts.length > 0 && (
                <div className="conflict-list">
                  <strong>Resolve filename conflicts</strong>
                  <small>
                    Existing files were left untouched. Enter a new filename for
                    each incoming copy, or change the destination date above.
                  </small>
                  {conflicts.map((conflict) => (
                    <label key={conflict.sourcePath}>
                      {conflict.sourcePath.split(/[\\/]/).pop()}
                      <input
                        placeholder="Explicit new filename"
                        value={renames[conflict.sourcePath] ?? ''}
                        onChange={(event) =>
                          setRenames((current) => ({
                            ...current,
                            [conflict.sourcePath]: event.target.value,
                          }))
                        }
                      />
                    </label>
                  ))}
                </div>
              )}
              {(inventory?.alreadyImportedFileCount ?? 0) > 0 && (
                <label>
                  <input
                    type="checkbox"
                    checked={allowDuplicateCopies}
                    onChange={(event) =>
                      setAllowDuplicateCopies(event.target.checked)
                    }
                  />{' '}
                  Intentionally create additional physical copies of verified
                  duplicates
                </label>
              )}
              {(inventory?.attentionFileCount ?? 0) > 0 && (
                <label>
                  <input
                    type="checkbox"
                    checked={allowUnverifiedReimport}
                    onChange={(event) =>
                      setAllowUnverifiedReimport(event.target.checked)
                    }
                  />{' '}
                  Copy safe incoming sources whose previous catalogue copy is
                  missing or failed verification
                </label>
              )}
              <button
                className="primary"
                disabled={
                  startingIngest ||
                  inventoryLoading ||
                  !inventory ||
                  (inventory.copyFileCount === 0 &&
                    !allowDuplicateCopies &&
                    !allowUnverifiedReimport) ||
                  !sources.length ||
                  (dateOverride === 'choose' && !chosenDate)
                }
                onClick={() =>
                  conflicts.length > 0
                    ? void reviewRenamedDestinations()
                    : void ingest()
                }
              >
                {startingIngest
                  ? conflicts.length > 0
                    ? 'Approving renamed destinations…'
                    : 'Creating ingest job…'
                  : conflicts.length > 0
                    ? 'Review renamed destinations'
                    : sdGrabberMode
                      ? allowDuplicateCopies || allowUnverifiedReimport
                        ? `Grab ${(inventory?.copyFileCount ?? 0) + (allowDuplicateCopies ? (inventory?.alreadyImportedFileCount ?? 0) : 0) + (allowUnverifiedReimport ? (inventory?.attentionFileCount ?? 0) : 0)} reviewed files`
                        : `Grab ${inventory?.copyFileCount ?? 0} new file${inventory?.copyFileCount === 1 ? '' : 's'}`
                      : 'Start verified ingest'}
              </button>
            </div>
          </section>
        ) : view === 'Projects' ? (
          selectedProject ? (
            <section className="project-workspace">
              <header className="project-workspace-header">
                <button
                  type="button"
                  onClick={() => {
                    projectInventoryGeneration.current += 1;
                    setSelectedProjectId(undefined);
                    setProjectInventory(undefined);
                    setProjectInventoryError('');
                    setProjectInventoryLoading(false);
                  }}
                >
                  <ArrowLeft aria-hidden="true" size={15} /> Back to Projects
                </button>
                <div>
                  <span>Working project</span>
                  <h2>{selectedProject.name}</h2>
                  <small>{selectedProject.workingDirectory}</small>
                </div>
              </header>

              <div className="project-summary-grid">
                <div>
                  <span>Status</span>
                  <strong>
                    {projectInventoryError
                      ? 'Folder unavailable'
                      : selectedProjectSyncIssue
                        ? 'Manifest synchronization pending'
                        : 'Ready'}
                  </strong>
                </div>
                <div>
                  <span>Linked recordings</span>
                  <strong>
                    {logicalProjectRecordingCount(
                      selectedProject.id,
                      catalogue.media,
                    )}
                  </strong>
                </div>
                <div>
                  <span>Project files</span>
                  <strong>{projectInventory?.itemCount ?? '—'}</strong>
                </div>
                <div>
                  <span>Folder size</span>
                  <strong>
                    {projectInventory ? size(projectInventory.sizeBytes) : '—'}
                  </strong>
                </div>
                <div>
                  <span>Main Premiere project</span>
                  <strong>
                    {projectInventory?.mainPremiereProject ?? 'Not found'}
                  </strong>
                </div>
                <div>
                  <span>Last successful scan</span>
                  <strong>
                    {projectInventory
                      ? new Date(projectInventory.scannedAt).toLocaleString()
                      : 'Never'}
                  </strong>
                </div>
              </div>

              <div className="project-actions">
                <button
                  type="button"
                  onClick={() =>
                    void desktopBridge
                      .openProjectFolder(selectedProject.id)
                      .catch((reason) => setError(errorText(reason)))
                  }
                >
                  <FolderOpen aria-hidden="true" size={15} /> Open project
                  folder
                </button>
                <button
                  type="button"
                  disabled={projectInventoryLoading}
                  onClick={() =>
                    void refreshProjectInventory(selectedProject.id)
                  }
                >
                  <RefreshCw aria-hidden="true" size={15} />{' '}
                  {projectInventoryLoading ? 'Scanning…' : 'Refresh inventory'}
                </button>
                {selectedProjectSyncIssue && (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() =>
                      void retryProjectSynchronization(selectedProject.id)
                    }
                  >
                    Retry project synchronization
                  </button>
                )}
              </div>
              {selectedProjectSyncIssue && (
                <small role="status">{selectedProjectSyncIssue.detail}</small>
              )}
              {projectInventoryError && (
                <div className="error project-local-error" role="alert">
                  Project folder unavailable: {projectInventoryError}. The
                  Library remains available.
                </div>
              )}

              <section className="project-info-editor">
                <div>
                  <h3>Project information</h3>
                  <p>
                    Portable notes and tags are synchronized as catalogue
                    events.
                  </p>
                </div>
                <label>
                  Description and notes
                  <textarea
                    value={projectNotes}
                    onChange={(event) => setProjectNotes(event.target.value)}
                  />
                </label>
                <SearchableComboBox
                  label="Project tags"
                  mode="multiple"
                  options={knownTags.map((tag) => ({ id: tag, label: tag }))}
                  selectedIds={projectTags}
                  onSelectionChange={setProjectTags}
                  onCreate={(label) => ({ id: label, label })}
                  placeholder="Search or create project tags…"
                />
                <button
                  type="button"
                  className="primary"
                  disabled={busy}
                  onClick={() => void saveProjectInformation()}
                >
                  Save project information
                </button>
              </section>

              <section className="project-assets">
                <div className="section-head">
                  <div>
                    <h3>Project-local files</h3>
                    <span>
                      Read-only inventory; supported saved projects are parsed
                      from stable snapshots and never changed
                    </span>
                  </div>
                </div>
                {projectInventory && projectInventory.items.length > 0 ? (
                  Object.keys(projectCategoryLabels).map((category) => {
                    const items = projectInventory.items.filter(
                      (item) => item.category === category,
                    );
                    if (!items.length) return null;
                    return (
                      <details key={category} open>
                        <summary>
                          {projectCategoryLabels[category] ?? category}{' '}
                          <span>{items.length}</span>
                        </summary>
                        <div className="project-asset-list">
                          {items.map((item) => (
                            <button
                              type="button"
                              key={item.relativePath}
                              onClick={() =>
                                void desktopBridge
                                  .revealProjectAsset(
                                    selectedProject.id,
                                    item.relativePath,
                                  )
                                  .catch((reason) =>
                                    setError(errorText(reason)),
                                  )
                              }
                            >
                              <FileText aria-hidden="true" size={15} />
                              <span>
                                <strong>{item.name}</strong>
                                <small>{item.relativePath}</small>
                              </span>
                              <small>{size(item.sizeBytes)}</small>
                            </button>
                          ))}
                        </div>
                      </details>
                    );
                  })
                ) : (
                  <p className="muted">
                    {projectInventoryLoading
                      ? 'Inspecting the project folder…'
                      : 'No project-local files are currently available.'}
                  </p>
                )}
              </section>

              {projectInventory?.premiereIndex && (
                <section className="premiere-index">
                  <div className="section-head">
                    <div>
                      <h3>Saved-project observation</h3>
                      <span>
                        {projectInventory.premiereIndex.status} · parser{' '}
                        {projectInventory.premiereIndex.parserVersion} ·{' '}
                        {new Date(
                          projectInventory.premiereIndex.indexedAt,
                        ).toLocaleString()}
                      </span>
                    </div>
                  </div>
                  <p>
                    This local, rebuildable index observes a completed saved
                    project. It may differ from unsaved Premiere state, is not
                    live Companion evidence, and never marks unknown media as
                    not imported.
                  </p>
                  {premiereSummary && (
                    <div className="project-stats premiere-summary">
                      <div>
                        <span>Imported</span>
                        <strong>{premiereSummary.imported}</strong>
                      </div>
                      <div>
                        <span>Used in timelines</span>
                        <strong>{premiereSummary.used ?? 'Unknown'}</strong>
                      </div>
                      <div>
                        <span>Unused imports</span>
                        <strong>{premiereSummary.unused ?? 'Unknown'}</strong>
                      </div>
                      <div>
                        <span>Unresolved</span>
                        <strong>{premiereSummary.unresolved}</strong>
                      </div>
                      <div>
                        <span>Historical evidence</span>
                        <strong>{premiereSummary.historical}</strong>
                      </div>
                      <div>
                        <span>Unclassified references</span>
                        <strong>{premiereSummary.unclassified}</strong>
                      </div>
                    </div>
                  )}
                  {projectInventory.premiereIndex.issues.map((issue) => (
                    <div
                      className="project-index-issue"
                      role="status"
                      key={issue}
                    >
                      {issue}
                    </div>
                  ))}
                  {projectInventory.premiereIndex.status !== 'unsupported' && (
                    <div className="premiere-index-grid">
                      <div>
                        <h4>Top-level sequences</h4>
                        {projectInventory.premiereIndex.sequences
                          .filter((sequence) => !sequence.nested)
                          .map((sequence) => (
                            <button
                              type="button"
                              className="project-index-row project-index-sequence"
                              aria-pressed={
                                selectedPremiereSequenceId === sequence.id
                              }
                              key={sequence.id}
                              onClick={() =>
                                setSelectedPremiereSequenceId(
                                  selectedPremiereSequenceId === sequence.id
                                    ? undefined
                                    : sequence.id,
                                )
                              }
                            >
                              <strong>{sequence.name}</strong>
                              <span>
                                {sequence.timelinePlacements} placement
                                {sequence.timelinePlacements === 1 ? '' : 's'}
                              </span>
                            </button>
                          ))}
                        {!projectInventory.premiereIndex.sequences.some(
                          (sequence) => !sequence.nested,
                        ) && <span className="muted">Unknown</span>}
                        <details>
                          <summary>
                            Nested sequences (
                            {
                              projectInventory.premiereIndex.sequences.filter(
                                (sequence) => sequence.nested,
                              ).length
                            }
                            )
                          </summary>
                          {projectInventory.premiereIndex.sequences
                            .filter((sequence) => sequence.nested)
                            .map((sequence) => (
                              <button
                                type="button"
                                className="project-index-row project-index-sequence"
                                aria-pressed={
                                  selectedPremiereSequenceId === sequence.id
                                }
                                key={sequence.id}
                                onClick={() =>
                                  setSelectedPremiereSequenceId(
                                    selectedPremiereSequenceId === sequence.id
                                      ? undefined
                                      : sequence.id,
                                  )
                                }
                              >
                                <strong>{sequence.name}</strong>
                                <span>
                                  {sequence.timelinePlacements} placement
                                  {sequence.timelinePlacements === 1 ? '' : 's'}
                                </span>
                              </button>
                            ))}
                        </details>
                        {selectedPremiereSequenceId &&
                          (() => {
                            const sequence =
                              projectInventory.premiereIndex?.sequences.find(
                                (candidate) =>
                                  candidate.id === selectedPremiereSequenceId,
                              );
                            if (!sequence) return null;
                            return (
                              <div className="sequence-recordings">
                                <h5>{sequence.name} recordings</h5>
                                {sequence.mediaPaths.length ? (
                                  sequence.mediaPaths.map((path) => {
                                    const reference =
                                      projectInventory.premiereIndex?.mediaReferences.find(
                                        (candidate) => candidate.path === path,
                                      );
                                    return (
                                      <div
                                        className="project-index-row"
                                        key={path}
                                      >
                                        <code>{path}</code>
                                        <span>
                                          {reference?.mediaId
                                            ? 'Matched'
                                            : 'Unresolved'}
                                        </span>
                                      </div>
                                    );
                                  })
                                ) : (
                                  <span className="muted">
                                    Usage unknown or no established timeline
                                    media.
                                  </span>
                                )}
                              </div>
                            );
                          })()}
                      </div>
                      <div>
                        <h4>Imported-media evidence</h4>
                        {projectInventory.premiereIndex.mediaReferences.map(
                          (reference) => (
                            <div
                              className="project-index-row"
                              key={reference.path}
                            >
                              <code>{reference.path}</code>
                              <span>
                                {reference.evidence === 'used-in-saved-timeline'
                                  ? `Used · ${reference.timelinePlacements} placement${reference.timelinePlacements === 1 ? '' : 's'}`
                                  : reference.evidence ===
                                      'imported-into-saved-project'
                                    ? 'Imported · unused'
                                    : reference.evidence ===
                                        'historical-reference-supported'
                                      ? 'Historical · supported by prior observation'
                                      : 'Unclassified project reference'}
                                {' · '}
                                {reference.mediaId
                                  ? 'Matched to Master Bin ID'
                                  : 'Unresolved'}
                              </span>
                            </div>
                          ),
                        )}
                        {!projectInventory.premiereIndex.mediaReferences
                          .length && <span className="muted">Unknown</span>}
                      </div>
                    </div>
                  )}
                </section>
              )}

              <section className="project-linked-media">
                <div className="section-head">
                  <div>
                    <h3>Linked Master Bin recordings</h3>
                    <span>
                      Assignments reference immutable originals; they do not
                      copy files into this project
                    </span>
                  </div>
                </div>
                <label className="search">
                  <Search aria-hidden="true" size={17} />
                  <input
                    value={projectMediaQuery}
                    onChange={(event) => {
                      setProjectMediaQuery(event.target.value);
                      setProjectAvailablePage(1);
                    }}
                    placeholder="Search Master Bin footage to link…"
                  />
                </label>
                {projectMediaSelection.length > 0 && (
                  <div className="project-media-actions">
                    <strong>{projectMediaSelection.length} selected</strong>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void updateProjectMediaAssignments(true)}
                    >
                      Add to project
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void updateProjectMediaAssignments(false)}
                    >
                      Remove assignment
                    </button>
                    <button
                      type="button"
                      onClick={() => void copyProjectMediaPaths()}
                    >
                      <Copy aria-hidden="true" size={14} /> Copy original paths
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void updateProjectMedia({ favourite: true })
                      }
                    >
                      Mark favourite
                    </button>
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void updateProjectMedia({ favourite: false })
                      }
                    >
                      Remove favourite
                    </button>
                    <SearchableComboBox
                      label="Tags to add to selected recordings"
                      mode="multiple"
                      options={knownTags.map((tag) => ({
                        id: tag,
                        label: tag,
                      }))}
                      selectedIds={projectAddTags}
                      onSelectionChange={setProjectAddTags}
                      onCreate={(label) => ({ id: label, label })}
                      placeholder="Tags to add…"
                    />
                    <button
                      type="button"
                      disabled={busy || !projectAddTags.length}
                      onClick={() =>
                        void updateProjectMedia({ addTags: projectAddTags })
                      }
                    >
                      Add selected tags
                    </button>
                    <SearchableComboBox
                      label="Tags to remove from selected recordings"
                      mode="multiple"
                      options={[
                        ...new Set(
                          projectMediaSelection.flatMap((id) => {
                            const item = catalogue.media.find(
                              (candidate) => candidate.id === id,
                            );
                            return item
                              ? logicalTags(item, catalogue.media)
                              : [];
                          }),
                        ),
                      ].map((tag) => ({ id: tag, label: tag }))}
                      selectedIds={projectRemoveTags}
                      onSelectionChange={setProjectRemoveTags}
                      placeholder="Tags to remove…"
                    />
                    <button
                      type="button"
                      disabled={busy || !projectRemoveTags.length}
                      onClick={() =>
                        void updateProjectMedia({
                          removeTags: projectRemoveTags,
                        })
                      }
                    >
                      Remove selected tags
                    </button>
                    {projectMediaSelection.length === 1 && (
                      <div className="project-media-description">
                        <label htmlFor="project-media-description">
                          Recording description
                        </label>
                        <input
                          id="project-media-description"
                          value={projectMediaDescription}
                          onChange={(event) =>
                            setProjectMediaDescription(event.target.value)
                          }
                          placeholder="Description or notes"
                        />
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() =>
                            void updateProjectMedia(
                              { notes: projectMediaDescription },
                              false,
                            )
                          }
                        >
                          Save description
                        </button>
                      </div>
                    )}
                  </div>
                )}
                {[
                  {
                    heading: 'Assigned recordings',
                    recordings: linkedProjectMedia,
                  },
                  {
                    heading: 'Available to add',
                    recordings: unlinkedProjectMedia,
                  },
                ].map(({ heading, recordings }) => {
                  const available = heading === 'Available to add';
                  if (available && !projectAvailableOpen) {
                    return (
                      <div className="project-media-section" key={heading}>
                        <button
                          type="button"
                          className="project-media-section-toggle"
                          aria-expanded="false"
                          onClick={() => setProjectAvailableOpen(true)}
                        >
                          Available to add ({unlinkedProjectMedia.length})
                        </button>
                      </div>
                    );
                  }
                  const displayedRecordings = available
                    ? pagedAvailableProjectMedia
                    : recordings;
                  return (
                    <div className="project-media-section" key={heading}>
                      {available ? (
                        <div className="project-available-heading">
                          <button
                            type="button"
                            className="project-media-section-toggle"
                            aria-expanded="true"
                            onClick={() => setProjectAvailableOpen(false)}
                          >
                            Available to add ({unlinkedProjectMedia.length})
                          </button>
                          <label>
                            Search available recordings
                            <input
                              value={projectAvailableQuery}
                              onChange={(event) => {
                                setProjectAvailableQuery(event.target.value);
                                setProjectAvailablePage(1);
                              }}
                            />
                          </label>
                          <SearchableComboBox
                            label="Filter available recordings by tags"
                            mode="multiple"
                            options={knownTags.map((tag) => ({
                              id: tag,
                              label: tag,
                            }))}
                            selectedIds={projectAvailableTags}
                            onSelectionChange={(tags) => {
                              setProjectAvailableTags(tags);
                              setProjectAvailablePage(1);
                            }}
                            placeholder="Filter tags…"
                          />
                          <small>
                            Showing {displayedRecordings.length} of{' '}
                            {availableProjectMedia.length} matching recordings
                          </small>
                        </div>
                      ) : (
                        <h4>
                          Assigned recordings ({linkedProjectMedia.length})
                        </h4>
                      )}
                      <div className="project-media-list">
                        {(displayedRecordings as typeof projectMedia).map(
                          (item) => (
                            <div className="project-media-row" key={item.id}>
                              <input
                                type="checkbox"
                                aria-label={`Select ${item.originalFilename}`}
                                checked={projectMediaSelection.includes(
                                  item.id,
                                )}
                                onChange={(event) => {
                                  setProjectMediaSelection((current) =>
                                    event.target.checked
                                      ? [...new Set([...current, item.id])]
                                      : current.filter((id) => id !== item.id),
                                  );
                                  if (event.target.checked) {
                                    setProjectMediaDescription(item.notes);
                                    setProjectAddTags([]);
                                    setProjectRemoveTags([]);
                                  }
                                }}
                              />
                              <div className="project-media-thumb">
                                <MediaThumbnail
                                  mediaId={item.id}
                                  fileType={item.fileType}
                                  thumbnailSource={thumbnailSources[item.id]}
                                  sourceAvailable={item.available}
                                  previewStored={previewStoredIds.has(item.id)}
                                  onPreviewPrepared={receivePreparedPreview}
                                  previewKey={`${item.id}:${item.sha256}:${item.observedSize ?? ''}:${item.observedModifiedMillis ?? ''}`}
                                  previewCacheEpoch={previewCacheEpoch}
                                  onThumbnailPrepared={receivePreparedThumbnail}
                                />
                              </div>
                              <span>
                                <strong>{item.originalFilename}</strong>
                                <small>{item.relativePath}</small>
                                <span className="pills">
                                  {logicalTags(item, catalogue.media).map(
                                    (tag) => (
                                      <em key={tag}>{tag}</em>
                                    ),
                                  )}
                                </span>
                              </span>
                              {item.favourite && (
                                <Star aria-label="Favourite" size={14} />
                              )}
                              <button
                                type="button"
                                disabled={!item.available}
                                onClick={(event) => {
                                  event.stopPropagation();
                                  void desktopBridge
                                    .revealMediaInExplorer(item.id)
                                    .catch((reason) =>
                                      setError(errorText(reason)),
                                    );
                                }}
                              >
                                Reveal
                              </button>
                              <button
                                type="button"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  navigateProjectMediaToLibrary(item.id);
                                }}
                              >
                                View in Library
                              </button>
                            </div>
                          ),
                        )}
                      </div>
                      {available && availableProjectPageCount > 1 && (
                        <div
                          className="project-available-pages"
                          aria-label="Available recording pages"
                        >
                          <button
                            type="button"
                            disabled={projectAvailablePage === 1}
                            onClick={() =>
                              setProjectAvailablePage((page) => page - 1)
                            }
                          >
                            Previous
                          </button>
                          <span>
                            Page {projectAvailablePage} of{' '}
                            {availableProjectPageCount}
                          </span>
                          <button
                            type="button"
                            disabled={
                              projectAvailablePage === availableProjectPageCount
                            }
                            onClick={() =>
                              setProjectAvailablePage((page) => page + 1)
                            }
                          >
                            Next
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </section>
            </section>
          ) : (
            <section className="settings-form project-index">
              <h2>Working projects</h2>
              <p>
                Projects reference media directly from Master Bin. Existing
                Premiere folders are inventoried without being modified.
              </p>
              <PathField
                label="Project working directory"
                value={projectDirectory}
                onChange={setProjectDirectory}
                onBrowse={async () => {
                  const path = await desktopBridge.chooseFolder();
                  if (path) setProjectDirectory(path);
                }}
              />
              <label>
                Project name
                <input
                  value={projectName}
                  onChange={(event) => setProjectName(event.target.value)}
                />
              </label>
              <button
                className="primary"
                disabled={busy || !projectName || !projectDirectory}
                onClick={createProject}
              >
                Create or associate project
              </button>
              <div className="project-list">
                {catalogue.projects.map((project) => {
                  const syncIssue = catalogue.issues?.find(
                    (issue) =>
                      issue.code === 'project-manifest-pending' &&
                      issue.source === project.id,
                  );
                  return (
                    <div key={project.id}>
                      <button
                        type="button"
                        className="project-card-open"
                        onClick={() => openProject(project)}
                      >
                        <strong>{project.name}</strong>
                        <small>{project.workingDirectory}</small>
                        <span>
                          {logicalProjectRecordingCount(
                            project.id,
                            catalogue.media,
                          )}{' '}
                          assigned recordings
                        </span>
                      </button>
                      {syncIssue && (
                        <div className="project-sync-warning">
                          <small role="status">{syncIssue.detail}</small>
                          <button
                            type="button"
                            disabled={busy}
                            onClick={() =>
                              void retryProjectSynchronization(project.id)
                            }
                          >
                            Retry project synchronization
                          </button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </section>
          )
        ) : (
          <section className="settings-form backup-workspace">
            <h2>Verified backup</h2>
            <p>
              Copy a complete project folder to a separate drive. PIMP stages
              each file on that drive, preserves its hierarchy, refuses
              overwrites, and verifies every SHA-256 hash before marking the
              backup verified.
            </p>
            <SearchableComboBox
              label="Project to back up"
              mode="single"
              options={catalogue.projects.map((project) => ({
                id: project.id,
                label: project.name,
                description: project.workingDirectory,
              }))}
              selectedIds={backupProjectId ? [backupProjectId] : []}
              onSelectionChange={(ids) => setBackupProjectId(ids[0])}
              placeholder="Search projects…"
            />
            <PathField
              label="External-drive destination"
              value={backupDestination}
              onChange={setBackupDestination}
              onBrowse={async () => {
                const path = await desktopBridge.chooseFolder();
                if (path) setBackupDestination(path);
              }}
            />
            <button
              type="button"
              className="primary"
              disabled={backupBusy || !backupProjectId || !backupDestination}
              onClick={() => void startBackup()}
            >
              {backupBusy
                ? 'Backup operation running…'
                : 'Start verified backup'}
            </button>

            <div className="backup-safety" role="note">
              <strong>Cleanup is unavailable</strong>
              <p>
                This milestone never deletes source media. A verified backup is
                evidence only; it does not prove Premiere dependency collection
                or authorize cleanup.
              </p>
            </div>

            <section className="backup-provider">
              <h3>Degoo handoff</h3>
              <p>
                Degoo has no supported public integration API suitable for an
                unattended PIMP backup. Use Degoo's official browser to upload a
                verified generation. Cloud status remains verification pending
                until PIMP can independently download and hash every object.
              </p>
              <button
                type="button"
                onClick={() => void desktopBridge.openSupportLink('degoo')}
              >
                Open official Degoo web app
              </button>
            </section>

            <section className="backup-history">
              <h3>Backup jobs</h3>
              {backupJobs.length === 0 ? (
                <p>No backup jobs have been created on this device.</p>
              ) : (
                backupJobs
                  .slice()
                  .reverse()
                  .map((job) => (
                    <article key={job.id} className="backup-job">
                      <div>
                        <strong>{job.projectName}</strong>
                        <span className={`status ${job.state}`}>
                          {job.state}
                        </span>
                      </div>
                      <small title={job.generationPath}>
                        {job.generationPath}
                      </small>
                      <progress
                        max={Math.max(job.manifest.sourceBytes, 1)}
                        value={job.bytesCompleted}
                      />
                      <small>
                        {job.filesCompleted} / {job.manifest.sourceFileCount}{' '}
                        files · {size(job.bytesCompleted)} /{' '}
                        {size(job.manifest.sourceBytes)}
                      </small>
                      {job.error && <p role="alert">{job.error}</p>}
                      <div className="project-actions">
                        {job.state === 'verified' ? (
                          <>
                            <button
                              type="button"
                              disabled={backupBusy}
                              onClick={() => void verifyBackup(job.id)}
                            >
                              Re-verify hashes
                            </button>
                            <button
                              type="button"
                              disabled={backupBusy}
                              onClick={() => void testBackupRestore(job.id)}
                            >
                              Test restore to empty folder
                            </button>
                          </>
                        ) : (
                          <button
                            type="button"
                            disabled={backupBusy}
                            onClick={() => void retryBackup(job.id)}
                          >
                            Choose drive and retry
                          </button>
                        )}
                      </div>
                    </article>
                  ))
              )}
            </section>
          </section>
        )}
      </main>
    </div>
  );
}

function PathField({
  label,
  value,
  placeholder,
  onChange,
  onBrowse,
}: {
  label: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
  onBrowse: () => void;
}) {
  return (
    <label>
      {label}
      <div className="path-field">
        <input
          value={value}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
        />
        <button onClick={onBrowse}>Browse…</button>
      </div>
    </label>
  );
}

function ToggleField({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="toggle-field">
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  );
}

function jobIsClearableHistory(job: IngestJob) {
  return (
    job.state === 'completed' &&
    job.files.length > 0 &&
    job.failures.length === 0 &&
    (job.inventoryIssues?.length ?? 0) === 0 &&
    job.files.every(
      (file) =>
        file.state === 'completed' &&
        !file.pendingMetadata &&
        !file.metadataError &&
        !file.error,
    )
  );
}

const previewableVideoTypes = new Set([
  'mp4',
  'mov',
  'm4v',
  'mxf',
  'mts',
  'm2ts',
  'avi',
]);

function MediaThumbnail({
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
  const preview =
    previewEntry &&
    previewEntry.key === previewKey &&
    previewEntry.epoch === previewCacheEpoch
      ? previewEntry.preview
      : undefined;
  const previewEligible =
    Boolean(previewKey) &&
    sourceAvailable &&
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
      { rootMargin: '400px' },
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
      {imageSource ? (
        <img
          src={imageSource}
          alt=""
          draggable={false}
          loading={nearViewport ? 'eager' : 'lazy'}
          decoding="async"
          onDragStart={(event) => event.preventDefault()}
          onError={() => setFailedImageSource(imageSource)}
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
          Scrubbable preview stored on this device
        </span>
      )}
      {isHovering &&
        displayedIndex != null &&
        preview?.frameTimes[displayedIndex] != null && (
          <span className="preview-timecode">
            {duration(preview.frameTimes[displayedIndex])}
          </span>
        )}
    </div>
  );
}

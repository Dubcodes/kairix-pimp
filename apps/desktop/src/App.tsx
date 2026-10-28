import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  type Catalogue,
  type AppInfo,
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
  | 'Appearance'
  | 'Updates'
  | 'About & Support'
  | 'Diagnostics';
type FolderSort = NonNullable<DeviceSettings['libraryFolderSort']>;

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
const size = (bytes: number) =>
  bytes < 1024 * 1024
    ? `${Math.round(bytes / 1024)} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
const duration = (seconds?: number) =>
  seconds == null
    ? '—'
    : `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, '0')}`;
const errorText = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function App() {
  const native = nativeAvailable();
  const [settings, setSettings] = useState<DeviceSettings>(emptySettings);
  const [catalogue, setCatalogue] = useState<Catalogue>(emptyCatalogue);
  const [view, setView] = useState<View>('Library');
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>('General');
  const [query, setQuery] = useState('');
  const [folder, setFolder] = useState('All media');
  const [selected, setSelected] = useState<string[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string>();
  const [projectInventory, setProjectInventory] = useState<ProjectInventory>();
  const [projectInventoryLoading, setProjectInventoryLoading] = useState(false);
  const [projectInventoryError, setProjectInventoryError] = useState('');
  const [projectMediaQuery, setProjectMediaQuery] = useState('');
  const [projectMediaSelection, setProjectMediaSelection] = useState<string[]>(
    [],
  );
  const [projectAddTags, setProjectAddTags] = useState<string[]>([]);
  const [projectRemoveTags, setProjectRemoveTags] = useState<string[]>([]);
  const [projectMediaDescription, setProjectMediaDescription] = useState('');
  const [projectNotes, setProjectNotes] = useState('');
  const [projectTags, setProjectTags] = useState<string[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [dateOverride, setDateOverride] = useState('default');
  const [chosenDate, setChosenDate] = useState('');
  const [progress, setProgress] = useState<IngestProgress>();
  const [jobs, setJobs] = useState<IngestJob[]>([]);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [completedJobsOpen, setCompletedJobsOpen] = useState(false);
  const [clearingCompletedJobs, setClearingCompletedJobs] = useState(false);
  const [startingIngest, setStartingIngest] = useState(false);
  const [inventory, setInventory] = useState<SourceInventory>();
  const [inventoryLoading, setInventoryLoading] = useState(false);
  const [inventoryError, setInventoryError] = useState('');
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
  const [batchDescription, setBatchDescription] = useState('');
  const [ingestBatchTags, setIngestBatchTags] = useState<string[]>([]);
  const [showScrollTop, setShowScrollTop] = useState(false);
  const [contextMenu, setContextMenu] = useState<{
    mediaId: string;
    x: number;
    y: number;
  }>();
  const [projectName, setProjectName] = useState('');
  const [projectDirectory, setProjectDirectory] = useState('');
  const catalogueSignature = useRef<string | undefined>(undefined);
  const metadataWrites = useRef(new Map<string, Promise<void>>());
  const pendingMetadataPatches = useRef(
    new Map<string, ProvisionalMetadataPatch>(),
  );
  const selectionAnchor = useRef<string | undefined>(undefined);
  const contentRef = useRef<HTMLElement>(null);
  const projectInventoryGeneration = useRef(0);
  const thumbnailSourceGeneration = useRef(0);
  const libraryCardRefs = useRef(new Map<string, HTMLElement>());
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
    settingsRef.current = settings;
    savingSettings.setLatest(settings);
  }, [savingSettings, settings]);
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
            .enrichMedia()
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
    if (!native || sources.length === 0) return;
    let current = true;
    const previewDateOverride =
      dateOverride === 'default'
        ? undefined
        : dateOverride === 'choose'
          ? chosenDate || undefined
          : dateOverride;
    void desktopBridge
      .inventorySources(sources, previewDateOverride)
      .then((result) => {
        if (current) {
          setInventory(result);
          setInventoryError('');
        }
      })
      .catch((reason) => {
        if (current) setInventoryError(errorText(reason));
      })
      .finally(() => {
        if (current) setInventoryLoading(false);
      });
    return () => {
      current = false;
    };
  }, [native, sources, dateOverride, chosenDate]);
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
    if (!native || !configured) return;
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
  }, [configured, native, runDiscovery]);

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
    const mediaIds = catalogue.media
      .filter((item) => item.thumbnailPath)
      .map((item) => item.id);
    void desktopBridge
      .loadThumbnailSources(mediaIds)
      .then((sources) => {
        setThumbnailSourceRequestCount((count) => count + 1);
        if (generation === thumbnailSourceGeneration.current)
          setThumbnailSources(sources);
      })
      .catch((reason) => {
        setThumbnailSourceRequestCount((count) => count + 1);
        if (generation === thumbnailSourceGeneration.current)
          void desktopBridge.logUiError(
            `Thumbnail source loading failed: ${errorText(reason)}`,
          );
      });
  }, [catalogue.media, native]);
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
          (folder === 'All media' || folder === itemFolder) &&
          logicalMediaMatches(item, query, catalogue)
        );
      }),
    [catalogue, displayedMedia, folder, query],
  );
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
      if (view !== 'Library' || !(event.ctrlKey || event.metaKey)) return;
      if (event.key.toLowerCase() !== 'a') return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, [contenteditable="true"]')) return;
      event.preventDefault();
      setSelected(filtered.map((item) => item.id));
      selectionAnchor.current = filtered.at(-1)?.id;
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [filtered, view]);
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
  useEffect(() => {
    const content = contentRef.current;
    if (!content) return;
    const updateScrollControl = () =>
      setShowScrollTop(content.scrollTop >= 480);
    updateScrollControl();
    content.addEventListener('scroll', updateScrollControl, { passive: true });
    return () => content.removeEventListener('scroll', updateScrollControl);
  }, []);

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
      setInventory(undefined);
      setInventoryError('');
      setInventoryLoading(true);
      setSources((current) => [...new Set([...current, ...chosen])]);
    }
  };
  const addFolder = async () => {
    const path = await desktopBridge.chooseFolder();
    if (path) {
      setInventory(undefined);
      setInventoryError('');
      setInventoryLoading(true);
      setSources((current) => [...new Set([...current, path])]);
    }
  };
  const ingest = async (renamedDestinations: Record<string, string> = {}) => {
    if (!sources.length) return;
    setStartingIngest(true);
    setError('');
    setMessage('');
    const jobId = crypto.randomUUID();
    try {
      const job = await desktopBridge.startIngest({
        jobId,
        sources,
        dateOverride:
          dateOverride === 'default'
            ? undefined
            : dateOverride === 'choose'
              ? chosenDate
              : dateOverride,
        renamedDestinations,
        batchTags: ingestBatchTags,
      });
      setJobs((current) => [...current, job]);
      setSources([]);
      setInventory(undefined);
      setInventoryLoading(false);
      setInventoryError('');
      setConflicts([]);
      setRenames({});
      setIngestBatchTags([]);
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
        setSelected((current) =>
          options.toggle ? [...new Set([...current, ...range])] : range,
        );
        return;
      }
    }
    selectionAnchor.current = mediaId;
    setSelected((current) => {
      if (!options.toggle) return [mediaId];
      return current.includes(mediaId)
        ? current.filter((id) => id !== mediaId)
        : [...current, mediaId];
    });
  };
  const selectedRecording =
    selected.length === 1
      ? catalogue.media.find((item) => item.id === selected[0])
      : undefined;
  const selectedAssociatedMedia = selectedRecording
    ? associatedMedia(selectedRecording, catalogue.media)
    : [];
  const selectedPhysicalIds = expandLogicalMediaIds(selected, catalogue.media);
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
        `${paths.length} media path${paths.length === 1 ? '' : 's'} copied for manual Premiere import${skipped ? `; ${skipped} unavailable item${skipped === 1 ? '' : 's'} skipped` : ''}.`,
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
        `Channel: ${applicationInfo?.releaseChannel ?? 'Beta'}`,
        `Device: ${settings.deviceId}`,
        `Catalogue items: ${catalogue.media.length}`,
        `Last scan: ${discoveryStatus.lastCompletedAt ?? 'never'}`,
        `Scan pending/errors: ${discoveryStatus.pending}/${discoveryStatus.errors}`,
        `Jobs: ${JSON.stringify(jobCounts)}`,
        `FFmpeg: ${mediaTools?.ffmpegAvailable ? 'available' : 'unavailable'}`,
        `FFprobe: ${mediaTools?.ffprobeAvailable ? 'available' : 'unavailable'}`,
        `Thumbnail source batches: ${thumbnailSourceRequestCount}`,
      ].join('\r\n'),
    );
    setMessage('Diagnostic summary copied without personal file paths.');
  };
  const clearPreviewCache = async () => {
    setClearingPreviewCache(true);
    setError('');
    hoverPreviewRequestCache.invalidate();
    setPreviewCacheEpoch((value) => value + 1);
    try {
      setPreviewCacheUsage(await desktopBridge.clearHoverPreviewCache());
      setMessage('Device-local hover preview cache cleared.');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      hoverPreviewRequestCache.invalidate();
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
    setUpdateStatus('Checking for a signed Beta update…');
    setError('');
    try {
      const update = await checkApprovedBetaUpdate();
      const checkedAt = new Date().toLocaleString();
      localStorage.setItem('pimp-last-update-check', checkedAt);
      setAvailableUpdate(update ?? undefined);
      setUpdateStatus(
        update
          ? `Signed Beta ${update.version} is available.`
          : `No newer signed Beta was available at ${checkedAt}.`,
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
        'Update installed. Restart required if PIMP does not reopen.',
      );
    } catch (reason) {
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
    setSelected([target.id]);
    selectionAnchor.current = target.id;
    setLibraryLocateId(target.id);
  };
  const activeJobs = jobs.filter((job) =>
    ['queued', 'copying', 'verifying'].includes(job.state),
  );
  const completedJobs = jobs.filter(jobIsClearableHistory);
  const needsAttentionJobs = jobs.filter(
    (job) => !activeJobs.includes(job) && !completedJobs.includes(job),
  );
  const workJobs = [...activeJobs, ...needsAttentionJobs];
  const updateBlockingJobs = jobs.filter((job) => !jobIsClearableHistory(job));
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
      <main ref={contentRef}>
        <header>
          <div>
            <p>
              {configured ? 'MASTER BIN / LOCAL CATALOGUE' : 'SETUP REQUIRED'}
            </p>
            <h1>{view}</h1>
          </div>
          <div className="header-actions">
            {native && configured && jobs.length > 0 && (
              <button
                className="job-indicator"
                aria-expanded={jobsOpen}
                onClick={() => setJobsOpen((open) => !open)}
              >
                {activeJobs.length > 0
                  ? `${activeJobs.length} active · ${Math.round(
                      (transferredJobBytes / Math.max(totalJobBytes, 1)) * 100,
                    )}%`
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
          <section className="job-panel" aria-label="Ingest jobs">
            <div className="job-panel-heading">
              <div>
                <strong>Background ingest</strong>
                <small>Pause takes effect safely between files.</small>
              </div>
              <button onClick={() => setJobsOpen(false)}>Close</button>
            </div>
            <div className="job-counts" role="status">
              <span>Active {activeJobs.length}</span>
              <span>Completed {completedJobs.length}</span>
              <span>Needs attention {needsAttentionJobs.length}</span>
            </div>
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
                    label="Check for beta updates"
                    description="Check the approved signed Beta channel; never install PR artifacts."
                    checked={settings.checkForUpdates ?? true}
                    onChange={(checkForUpdates) =>
                      void persistPreference({ checkForUpdates })
                    }
                  />
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
                    label="Thumbnail cache (device-local)"
                    value={settings.thumbnailCachePath}
                    placeholder="Automatic local cache"
                    onChange={(thumbnailCachePath) =>
                      setSettings({ ...settings, thumbnailCachePath })
                    }
                    onBrowse={() => chooseSettingFolder('thumbnailCachePath')}
                  />
                  <label>
                    Thumbnail cache limit (GB)
                    <input
                      type="number"
                      min="1"
                      value={Math.round(
                        settings.thumbnailCacheLimitBytes / 1024 / 1024 / 1024,
                      )}
                      onChange={(event) =>
                        setSettings({
                          ...settings,
                          thumbnailCacheLimitBytes:
                            Number(event.target.value) * 1024 * 1024 * 1024,
                        })
                      }
                    />
                  </label>
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
                              ? `${discoveryStatus.filesProcessed} / ${discoveryStatus.totalFiles}`
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
                          Scan for new files
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
                        onChange={() =>
                          setSettings({
                            ...settings,
                            ingestDateOrganization: 'today',
                          })
                        }
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
                        onChange={() =>
                          setSettings({
                            ...settings,
                            ingestDateOrganization: 'original-file-dates',
                          })
                        }
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
                  <ToggleField
                    label="Enable hover previews"
                    description="Generate low-resolution video frames locally when a Library thumbnail is hovered. No audio is played."
                    checked={settings.hoverPreviewsEnabled ?? true}
                    onChange={(hoverPreviewsEnabled) =>
                      void persistPreference({ hoverPreviewsEnabled })
                    }
                  />
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
                  <div className="info-card">
                    <strong>Hover preview cache</strong>
                    <span>{size(previewCacheUsage)} used</span>
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
                      {applicationInfo?.edition ?? 'Kairix PIMP Light'}
                    </strong>
                    <span>Version {applicationInfo?.version ?? 'unknown'}</span>
                    <span>
                      Channel {applicationInfo?.releaseChannel ?? 'Beta'}
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
                    <strong>Latest manually downloadable beta</strong>
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
                        updateBlockingJobs.length > 0
                      }
                      title={
                        updateBlockingJobs.length
                          ? 'Active or recoverable ingest jobs must be resolved before updating'
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
                      Only the approved Beta manifest and artifacts with a valid
                      Tauri signature can install.
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
                      Version {applicationInfo?.version ?? 'unknown'} · Beta
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
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search filenames, folders, tags, notes, projects, dates, codec…"
                />
              </label>
              <button
                disabled={busy || discoveryStatus.running}
                onClick={doRescan}
              >
                <RefreshCw size={15} />
                {discoveryStatus.running
                  ? `Scanning ${discoveryStatus.filesProcessed}/${discoveryStatus.totalFiles}`
                  : 'Scan for new files'}
              </button>
            </section>
            <section className="content-grid">
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
                      onClick={() => setFolder(name)}
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
              <div className="library-panel">
                <div className="section-head">
                  <div>
                    <h2>Media library</h2>
                    <span>{filtered.length} persisted items</span>
                  </div>
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
                        tabIndex={0}
                        aria-selected={selected.includes(item.id)}
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
                          setContextMenu(undefined);
                          selectRecording(item.id, {
                            toggle: event.ctrlKey || event.metaKey,
                            range: event.shiftKey,
                          });
                        }}
                        onKeyDown={(event) => {
                          if (event.key !== 'Enter' && event.key !== ' ')
                            return;
                          event.preventDefault();
                          selectRecording(item.id, {
                            toggle: event.ctrlKey || event.metaKey,
                            range: event.shiftKey,
                          });
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
                            previewKey={`${item.id}:${item.sha256}:${item.observedSize ?? ''}:${item.observedModifiedMillis ?? ''}`}
                            previewCacheEpoch={previewCacheEpoch}
                            enableHoverPreview={
                              (settings.hoverPreviewsEnabled ?? true) &&
                              !clearingPreviewCache
                            }
                          />
                          <span>
                            {duration(item.technical.durationSeconds)}
                          </span>
                          {selected.includes(item.id) && (
                            <b className="check">✓</b>
                          )}
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
                              <em key={tag}>{tag}</em>
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
            <div className="library-overlays">
              {showScrollTop && (
                <button
                  type="button"
                  className="scroll-top"
                  aria-label="Scroll Library to top"
                  title="Scroll to top"
                  onClick={() =>
                    contentRef.current?.scrollTo({
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
                <div className="batch">
                  <div className="batch-heading">
                    <strong>{selected.length} selected</strong>
                    <span>Changes persist immediately</span>
                  </div>
                  <div className="batch-combo">
                    <Tag size={15} />
                    <SearchableComboBox
                      label="Add tags"
                      mode="multiple"
                      options={knownTags.map((tag) => ({
                        id: tag,
                        label: tag,
                      }))}
                      selectedIds={batchTags}
                      onSelectionChange={(ids) => {
                        setBatchTags(ids);
                        void updateSelection({ addTags: ids });
                      }}
                      placeholder="Search or create tags…"
                      onCreate={(label) => ({ id: label, label })}
                    />
                  </div>
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
                      label="Add projects"
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
                      placeholder="Search projects…"
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
                    Description for selection
                    <input
                      value={batchDescription}
                      onChange={(event) =>
                        setBatchDescription(event.target.value)
                      }
                      placeholder="Replace description on selected media"
                    />
                    <button
                      className="batch-primary"
                      disabled={busy}
                      onClick={() =>
                        void updateSelection({ notes: batchDescription })
                      }
                    >
                      Apply description
                    </button>
                  </label>
                  <div className="file-actions" aria-label="Selection actions">
                    <button
                      className="batch-primary"
                      onClick={() => void updateSelection({ favourite: true })}
                    >
                      <Star size={13} /> Favourite
                    </button>
                    <button
                      onClick={() => void updateSelection({ favourite: false })}
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
                    >
                      <Copy size={13} /> Copy paths for Premiere
                    </button>
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
                          <div className="associated-file" key={recording.id}>
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
                                void runSelectedFileAction('reveal', recording)
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
                          onClick={() => void runSelectedFileAction('reveal')}
                        >
                          <LocateFixed size={13} /> Reveal in Explorer
                        </button>
                        <button
                          disabled={!selectedRecording.available}
                          onClick={() => void runSelectedFileAction('folder')}
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
                  <button
                    className="batch-clear"
                    onClick={() => {
                      setSelected([]);
                      setBatchTags([]);
                      setRemoveBatchTags([]);
                      setBatchProjects([]);
                      setRemoveBatchProjects([]);
                      setBatchDescription('');
                      selectionAnchor.current = undefined;
                    }}
                  >
                    Clear selection
                  </button>
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
                  Copy path for Premiere
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
          </>
        ) : view === 'Ingest' ? (
          <section className="ingest">
            <div className="drop">
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
                    {inventory.copyFileCount} of {inventory.totalFileCount}{' '}
                    files will be copied ({size(inventory.copyBytes)})
                  </strong>
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
                              {file.plannedRelativePath}
                            </code>
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
              {[
                ['default', 'Use remembered mode'],
                ['today', 'Today'],
                ['yesterday', 'Yesterday'],
                ['choose', 'Choose date'],
              ].map(([value, label]) => (
                <label
                  className={dateOverride === value ? 'chosen' : ''}
                  key={value}
                >
                  <input
                    type="radio"
                    checked={dateOverride === value}
                    onChange={() => setDateOverride(value)}
                  />
                  <span>
                    <b>{label}</b>
                  </span>
                </label>
              ))}
              {dateOverride === 'choose' && (
                <input
                  type="date"
                  value={chosenDate}
                  onChange={(event) => setChosenDate(event.target.value)}
                />
              )}
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
              <button
                className="primary"
                disabled={
                  startingIngest ||
                  inventoryLoading ||
                  !inventory ||
                  inventory.copyFileCount === 0 ||
                  !sources.length ||
                  (dateOverride === 'choose' && !chosenDate)
                }
                onClick={() =>
                  void ingest(
                    Object.fromEntries(
                      Object.entries(renames).filter(([, value]) =>
                        value.trim(),
                      ),
                    ),
                  )
                }
              >
                {startingIngest
                  ? 'Creating ingest job…'
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
                      Read-only inventory; Premiere files are never parsed or
                      changed
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
                    onChange={(event) =>
                      setProjectMediaQuery(event.target.value)
                    }
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
                  ['Assigned recordings', linkedProjectMedia],
                  ['Available to add', unlinkedProjectMedia],
                ].map(([heading, recordings]) => (
                  <div
                    className="project-media-section"
                    key={heading as string}
                  >
                    <h4>{heading as string}</h4>
                    <div className="project-media-list">
                      {(recordings as typeof projectMedia).map((item) => (
                        <div className="project-media-row" key={item.id}>
                          <input
                            type="checkbox"
                            aria-label={`Select ${item.originalFilename}`}
                            checked={projectMediaSelection.includes(item.id)}
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
                            />
                          </div>
                          <span>
                            <strong>{item.originalFilename}</strong>
                            <small>{item.relativePath}</small>
                            <span className="pills">
                              {logicalTags(item, catalogue.media).map((tag) => (
                                <em key={tag}>{tag}</em>
                              ))}
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
                                .catch((reason) => setError(errorText(reason)));
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
                      ))}
                    </div>
                  </div>
                ))}
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
          <section className="empty">
            <h2>No destructive archive actions</h2>
            <p>
              Automatic deletion and cleanup are intentionally deferred. Your
              source recordings remain untouched.
            </p>
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
}: {
  mediaId: string;
  fileType: string;
  thumbnailSource?: string;
  sourceAvailable?: boolean;
  previewKey?: string;
  previewCacheEpoch?: number;
  enableHoverPreview?: boolean;
}) {
  const thumbnailRef = useRef<HTMLDivElement>(null);
  const [showThumbnail, setShowThumbnail] = useState(
    () => typeof IntersectionObserver === 'undefined',
  );
  const [previewEntry, setPreviewEntry] = useState<{
    key: string;
    epoch: number;
    preview: HoverPreview;
  }>();
  const [previewIndex, setPreviewIndex] = useState<number>();
  const requestGeneration = useRef(0);
  const releasePreview = useCallback(() => {
    requestGeneration.current += 1;
    setPreviewIndex(undefined);
    setPreviewEntry(undefined);
  }, []);
  useEffect(() => {
    requestGeneration.current += 1;
  }, [previewCacheEpoch]);
  const preview =
    previewEntry &&
    previewEntry.key === previewKey &&
    previewEntry.epoch === previewCacheEpoch
      ? previewEntry.preview
      : undefined;
  useEffect(() => {
    const element = thumbnailRef.current;
    if (!element || typeof IntersectionObserver === 'undefined') return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setShowThumbnail(true);
          observer.disconnect();
        }
      },
      { rootMargin: '400px' },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
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
  const requestPreview = () => {
    if (
      preview ||
      !previewKey ||
      !sourceAvailable ||
      !enableHoverPreview ||
      !previewableVideoTypes.has(fileType.toLowerCase()) ||
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    )
      return;
    const generation = ++requestGeneration.current;
    const request = hoverPreviewRequestCache.request(previewKey, () =>
      desktopBridge.loadHoverPreview(mediaId),
    );
    void request
      .then((result) => {
        if (
          generation === requestGeneration.current &&
          result.mediaId === mediaId
        )
          setPreviewEntry({
            key: previewKey,
            epoch: previewCacheEpoch,
            preview: result,
          });
      })
      .catch(() => undefined);
  };
  const displayedFrame =
    previewIndex == null ? undefined : preview?.frames[previewIndex];
  return (
    <div
      ref={thumbnailRef}
      className="media-thumbnail"
      onPointerEnter={requestPreview}
      onPointerMove={(event) => {
        if (!preview?.frames.length) return;
        const bounds = event.currentTarget.getBoundingClientRect();
        const ratio = bounds.width
          ? Math.min(
              1,
              Math.max(0, (event.clientX - bounds.left) / bounds.width),
            )
          : 0;
        setPreviewIndex(
          Math.min(
            preview.frames.length - 1,
            Math.floor(ratio * preview.frames.length),
          ),
        );
      }}
      onPointerLeave={releasePreview}
      aria-label={
        preview ? 'Move horizontally to preview video frames' : undefined
      }
    >
      {displayedFrame || (showThumbnail && thumbnailSource) ? (
        <img
          src={displayedFrame ?? thumbnailSource}
          alt=""
          loading="lazy"
          decoding="async"
        />
      ) : (
        <span className="file-kind">{fileType.toUpperCase()}</span>
      )}
      {previewIndex != null && preview?.frameTimes[previewIndex] != null && (
        <span className="preview-timecode">
          {duration(preview.frameTimes[previewIndex])}
        </span>
      )}
    </div>
  );
}

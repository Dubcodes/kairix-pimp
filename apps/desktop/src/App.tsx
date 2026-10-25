import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Archive,
  Clock3,
  Copy,
  Folder,
  FolderOpen,
  HardDrive,
  Inbox,
  Library,
  LocateFixed,
  RefreshCw,
  Search,
  Settings,
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
  type DeviceSettings,
  type IngestJob,
  type IngestJobFile,
  type IngestProgress,
  type IngestResult,
  type MediaToolsStatus,
  type ProvisionalMetadataPatch,
  type SourceInventory,
} from './bridge';
import { datePatternPreview } from './date-pattern';

type View = 'Ingest' | 'Library' | 'Projects' | 'Archive' | 'Settings';

const icons = {
  Ingest: Inbox,
  Library,
  Projects: UsersRound,
  Archive,
  Settings,
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
  const [query, setQuery] = useState('');
  const [folder, setFolder] = useState('All media');
  const [selected, setSelected] = useState<string[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [dateOverride, setDateOverride] = useState('default');
  const [chosenDate, setChosenDate] = useState('');
  const [progress, setProgress] = useState<IngestProgress>();
  const [jobs, setJobs] = useState<IngestJob[]>([]);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [startingIngest, setStartingIngest] = useState(false);
  const [inventory, setInventory] = useState<SourceInventory>();
  const [inventoryLoading, setInventoryLoading] = useState(false);
  const [inventoryError, setInventoryError] = useState('');
  const [conflicts, setConflicts] = useState<IngestResult['conflicts']>([]);
  const [renames, setRenames] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [mediaTools, setMediaTools] = useState<MediaToolsStatus>();
  const [batchTags, setBatchTags] = useState<string[]>([]);
  const [batchProjects, setBatchProjects] = useState<string[]>([]);
  const [projectName, setProjectName] = useState('');
  const [projectDirectory, setProjectDirectory] = useState('');
  const catalogueSignature = useRef<string | undefined>(undefined);
  const metadataWrites = useRef(new Map<string, Promise<void>>());
  const pendingMetadataPatches = useRef(
    new Map<string, ProvisionalMetadataPatch>(),
  );

  const configured = Boolean(settings.initializedCataloguePath);
  const refresh = async () => setCatalogue(await desktopBridge.loadCatalogue());
  const receiveJobs = (incoming: IngestJob[]) =>
    setJobs(
      incoming.map((job) => ({
        ...job,
        files: job.files.map((file) => {
          const pending = pendingMetadataPatches.current.get(
            `${job.id}:${file.id}`,
          );
          return pending
            ? { ...file, metadata: { ...file.metadata, ...pending } }
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
    ])
      .then(
        async ([
          loaded,
          unlistenProgress,
          unlistenJobs,
          unlistenDrop,
          tools,
        ]) => {
          if (disposed) {
            unlistenProgress();
            unlistenJobs();
            unlistenDrop();
            return;
          }
          stopProgress = unlistenProgress;
          stopJobs = unlistenJobs;
          stopDrop = unlistenDrop;
          setSettings(loaded);
          setMediaTools(tools);
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
      .catch((reason) => setError(errorText(reason)));
    return () => {
      disposed = true;
      stopProgress?.();
      stopJobs?.();
      stopDrop?.();
    };
  }, [native]);

  useEffect(() => {
    if (!native || sources.length === 0) return;
    let current = true;
    void desktopBridge
      .inventorySources(sources)
      .then((result) => {
        if (current) setInventory(result);
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
  }, [native, sources]);
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
    const timer = window.setInterval(() => {
      void desktopBridge
        .rescan()
        .then(async (result) => {
          if (result.registered > 0 || result.changed > 0) {
            setCatalogue(await desktopBridge.loadCatalogue());
            void desktopBridge.enrichMedia().then(async () => {
              setCatalogue(await desktopBridge.loadCatalogue());
            });
          }
        })
        .catch(() => undefined);
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [configured, native]);

  const folders = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of catalogue.media) {
      const name =
        item.relativePath.split('/').slice(0, -1).join('/') ||
        'Master Bin root';
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
    return [...counts.entries()].sort(([a], [b]) => b.localeCompare(a));
  }, [catalogue.media]);
  const filtered = useMemo(
    () =>
      catalogue.media.filter((item) => {
        const itemFolder =
          item.relativePath.split('/').slice(0, -1).join('/') ||
          'Master Bin root';
        const projects = item.projectIds.map(
          (id) =>
            catalogue.projects.find((project) => project.id === id)?.name ?? '',
        );
        const haystack = [
          item.originalFilename,
          item.relativePath,
          item.notes,
          item.captureDate,
          item.ingestDate,
          item.fileType,
          item.technical.codec,
          item.technical.width,
          item.technical.height,
          item.technical.frameRate,
          ...item.tags,
          ...projects,
        ]
          .join(' ')
          .toLowerCase();
        return (
          (folder === 'All media' || folder === itemFolder) &&
          haystack.includes(query.trim().toLowerCase())
        );
      }),
    [catalogue, folder, query],
  );
  const knownTags = useMemo(
    () => [...new Set(catalogue.media.flatMap((item) => item.tags))].sort(),
    [catalogue.media],
  );

  const saveConfiguration = async () => {
    setError('');
    setBusy(true);
    try {
      const saved = await desktopBridge.saveSettings(settings);
      setSettings(saved);
      await refresh();
      setMessage('Configuration saved. The catalogue is ready.');
      setView('Ingest');
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
    }
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
      });
      setJobs((current) => [...current, job]);
      setSources([]);
      setInventory(undefined);
      setInventoryLoading(false);
      setInventoryError('');
      setConflicts([]);
      setRenames({});
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
                  ? { ...candidateFile, metadata }
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
        if (metadataWrites.current.get(key) === next)
          pendingMetadataPatches.current.delete(key);
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
  }) => {
    if (!selected.length) return;
    try {
      setCatalogue(
        await desktopBridge.updateMedia({ mediaIds: selected, ...next }),
      );
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const selectedRecording =
    selected.length === 1
      ? catalogue.media.find((item) => item.id === selected[0])
      : undefined;
  const runSelectedFileAction = async (
    action: 'reveal' | 'folder' | 'copy',
  ) => {
    if (!selectedRecording?.available) return;
    setError('');
    try {
      if (action === 'reveal') {
        await desktopBridge.revealMediaInExplorer(selectedRecording.id);
        setMessage('Recording revealed in Windows Explorer.');
      } else if (action === 'folder') {
        await desktopBridge.openMediaFolder(selectedRecording.id);
        setMessage('Containing date folder opened in Windows Explorer.');
      } else {
        const path = await desktopBridge.resolveMediaPath(selectedRecording.id);
        await navigator.clipboard.writeText(path);
        setMessage('Absolute recording path copied to the clipboard.');
      }
    } catch (reason) {
      setError(errorText(reason));
    }
  };
  const doRescan = async () => {
    setBusy(true);
    setError('');
    try {
      const result = await desktopBridge.rescan();
      await refresh();
      setMessage(
        `${result.registered} external file${result.registered === 1 ? '' : 's'} registered; ${result.changed} changed and ${result.pending} still awaiting stability.`,
      );
      void desktopBridge
        .enrichMedia()
        .then(refresh)
        .catch(() => undefined);
    } catch (reason) {
      setError(errorText(reason));
    } finally {
      setBusy(false);
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
  const activeJobs = jobs.filter(
    (job) => !['completed', 'cancelled', 'failed'].includes(job.state),
  );
  const totalJobBytes = activeJobs.reduce(
    (total, job) => total + job.bytesTotal,
    0,
  );
  const transferredJobBytes = activeJobs.reduce(
    (total, job) => total + job.bytesTransferred,
    0,
  );

  return (
    <div className="app-shell">
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
        <nav>
          {(Object.keys(icons) as View[]).map((item) => {
            const Icon = icons[item];
            return (
              <button
                key={item}
                className={view === item ? 'active' : ''}
                onClick={() => setView(item)}
              >
                <Icon size={18} />
                {item}
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
      <main>
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
                  : `${jobs.length} ingest job${jobs.length === 1 ? '' : 's'}`}
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
            {jobs.map((job) => (
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
              concurrent conflict(s), {catalogue.issues?.length ?? 0} stream
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
          <section className="settings-form">
            <h2>Local storage configuration</h2>
            <p>
              Absolute paths stay on this device. Portable catalogue records use
              paths relative to Master Bin.
            </p>
            <PathField
              label="Master Bin"
              value={settings.masterBinPath}
              onChange={(value) =>
                setSettings({ ...settings, masterBinPath: value })
              }
              onBrowse={() => chooseSettingFolder('masterBinPath')}
            />
            <PathField
              label="Working Projects"
              value={settings.workingProjectsPath}
              onChange={(value) =>
                setSettings({ ...settings, workingProjectsPath: value })
              }
              onBrowse={() => chooseSettingFolder('workingProjectsPath')}
            />
            <label className="check-field">
              <input
                type="checkbox"
                checked={settings.catalogueInMasterBin}
                onChange={(event) =>
                  setSettings({
                    ...settings,
                    catalogueInMasterBin: event.target.checked,
                  })
                }
              />{' '}
              Store shared catalogue inside Master Bin
            </label>
            {!settings.catalogueInMasterBin && (
              <PathField
                label="Shared catalogue"
                value={settings.sharedCataloguePath}
                onChange={(value) =>
                  setSettings({ ...settings, sharedCataloguePath: value })
                }
                onBrowse={() => chooseSettingFolder('sharedCataloguePath')}
              />
            )}
            <PathField
              label="Thumbnail cache (device-local)"
              value={settings.thumbnailCachePath}
              placeholder="Automatic local cache"
              onChange={(value) =>
                setSettings({ ...settings, thumbnailCachePath: value })
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
                    settings.ingestDateOrganization === 'original-file-dates'
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
                placeholder="yyyymmdd"
              />
              <small>
                Preview: {datePatternPreview(settings.dateFolderPattern)}. Use
                MM for month, mm for minute, HH for 24-hour time, hh and a for
                12-hour time. yyyymmdd is supported as a familiar date-only
                alias.
              </small>
            </label>
            {settings.initializedCataloguePath && (
              <small>
                Catalogue location is locked until a safe migration workflow is
                implemented: {settings.initializedCataloguePath}
              </small>
            )}
            <button
              className="primary"
              disabled={busy}
              onClick={saveConfiguration}
            >
              {busy ? 'Saving…' : 'Save configuration'}
            </button>
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
              <button disabled={busy} onClick={doRescan}>
                <RefreshCw size={15} /> Rescan
              </button>
            </section>
            <section className="content-grid">
              <div className="folder-panel">
                <h3>Folders</h3>
                <button
                  className={`folder ${folder === 'All media' ? 'active' : ''}`}
                  onClick={() => setFolder('All media')}
                >
                  <Folder size={16} />
                  <span>
                    All media<small>{catalogue.media.length} items</small>
                  </span>
                </button>
                {folders.map(([name, count]) => (
                  <button
                    className={`folder ${folder === name ? 'active' : ''}`}
                    key={name}
                    onClick={() => setFolder(name)}
                  >
                    <Folder size={16} />
                    <span>
                      {name}
                      <small>{count} items</small>
                    </span>
                  </button>
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
                  <div className="cards">
                    {filtered.map((item) => (
                      <article
                        className={selected.includes(item.id) ? 'selected' : ''}
                        key={item.id}
                        onClick={() =>
                          setSelected((current) =>
                            current.includes(item.id)
                              ? current.filter((id) => id !== item.id)
                              : [...current, item.id],
                          )
                        }
                      >
                        <div
                          className={`thumb ${!item.available ? 'unavailable' : ''}`}
                        >
                          <MediaThumbnail
                            mediaId={item.id}
                            fileType={item.fileType}
                            available={Boolean(item.thumbnailPath)}
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
                          <small>{item.relativePath}</small>
                          <div className="pills">
                            {item.tags.map((tag) => (
                              <em key={tag}>{tag}</em>
                            ))}
                          </div>
                          <div className="projects">
                            {item.projectIds.map((id) => (
                              <span key={id}>
                                {catalogue.projects.find(
                                  (project) => project.id === id,
                                )?.name ?? id}
                              </span>
                            ))}
                          </div>
                          <footer>
                            <span>
                              <Clock3 size={12} />
                              {item.captureDate?.slice(0, 10) ?? 'Date unknown'}
                            </span>
                            <span>
                              {item.attachedAudioId ? (
                                <Volume2 size={12} />
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
                    options={knownTags.map((tag) => ({ id: tag, label: tag }))}
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
                {selectedRecording && (
                  <div className="file-actions" aria-label="Recording actions">
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
                )}
                <button
                  className="batch-clear"
                  onClick={() => {
                    setSelected([]);
                    setBatchTags([]);
                    setBatchProjects([]);
                  }}
                >
                  Clear selection
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
          <section className="settings-form">
            <h2>Working projects</h2>
            <p>
              Projects reference media directly from Master Bin. Existing
              Premiere folders are left intact.
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
              {catalogue.projects.map((project) => (
                <div key={project.id}>
                  <strong>{project.name}</strong>
                  <small>{project.workingDirectory}</small>
                  <span>
                    {
                      catalogue.media.filter((item) =>
                        item.projectIds.includes(project.id),
                      ).length
                    }{' '}
                    assigned media
                  </span>
                </div>
              ))}
            </div>
          </section>
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

function MediaThumbnail({
  mediaId,
  fileType,
  available,
}: {
  mediaId: string;
  fileType: string;
  available: boolean;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!available) return;
    let active = true;
    void desktopBridge.loadThumbnail(mediaId).then((value) => {
      if (active) setUrl(value);
    });
    return () => {
      active = false;
    };
  }, [available, mediaId]);
  return url ? (
    <img src={url} alt="" />
  ) : (
    <span className="file-kind">{fileType.toUpperCase()}</span>
  );
}

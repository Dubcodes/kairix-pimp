import { useEffect, useMemo, useState } from 'react';
import {
  Archive,
  Clock3,
  Folder,
  HardDrive,
  Inbox,
  Library,
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
  type IngestProgress,
  type IngestResult,
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
  const [conflicts, setConflicts] = useState<IngestResult['conflicts']>([]);
  const [renames, setRenames] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [batchTags, setBatchTags] = useState<string[]>([]);
  const [batchProjects, setBatchProjects] = useState<string[]>([]);
  const [projectName, setProjectName] = useState('');
  const [projectDirectory, setProjectDirectory] = useState('');

  const configured = Boolean(settings.initializedCataloguePath);
  const refresh = async () => setCatalogue(await desktopBridge.loadCatalogue());
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let stopProgress: (() => void) | undefined;
    let stopDrop: (() => void) | undefined;
    Promise.all([
      desktopBridge.loadSettings(),
      desktopBridge.onProgress(setProgress),
      desktopBridge.onFileDrop((paths) =>
        setSources((current) => [...new Set([...current, ...paths])]),
      ),
    ])
      .then(async ([loaded, unlistenProgress, unlistenDrop]) => {
        if (disposed) {
          unlistenProgress();
          unlistenDrop();
          return;
        }
        stopProgress = unlistenProgress;
        stopDrop = unlistenDrop;
        setSettings(loaded);
        if (loaded.initializedCataloguePath)
          setCatalogue(await desktopBridge.loadCatalogue());
      })
      .catch((reason) => setError(errorText(reason)));
    return () => {
      disposed = true;
      stopProgress?.();
      stopDrop?.();
    };
  }, [native]);
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
    setSources((current) => [...new Set([...current, ...chosen])]);
  };
  const addFolder = async () => {
    const path = await desktopBridge.chooseFolder();
    if (path) setSources((current) => [...new Set([...current, path])]);
  };
  const ingest = async (renamedDestinations: Record<string, string> = {}) => {
    if (!sources.length) return;
    setBusy(true);
    setError('');
    setMessage('');
    const jobId = crypto.randomUUID();
    try {
      const result = await desktopBridge.startIngest({
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
      await refresh();
      setConflicts(result.conflicts);
      setSources(result.conflicts.map((item) => item.sourcePath));
      setProgress(undefined);
      const parts = [
        `${result.imported.length} imported`,
        `${result.duplicates.length} exact duplicates skipped`,
      ];
      if (result.conflicts.length)
        parts.push(
          `${result.conflicts.length} conflicts need a new date or explicit filename`,
        );
      if (result.unsupported.length)
        parts.push(`${result.unsupported.length} unsupported inputs reported`);
      if (result.failures.length)
        parts.push(`${result.failures.length} failures`);
      setMessage(parts.join(' · '));
      if (result.conflicts.length || result.failures.length)
        setError(
          [
            ...result.conflicts.map(
              (item) => `${item.sourcePath}: ${item.reason}`,
            ),
            ...result.failures,
          ].join('\n'),
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
          <div className="sync">
            {native ? 'Native desktop' : 'Web preview'}{' '}
            <span>{configured ? 'persisted' : 'not configured'}</span>
          </div>
        </header>
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
                              {item.available ? 'Available' : 'Offline'}
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
                      setConflicts([]);
                      setRenames({});
                    }}
                  >
                    Clear
                  </button>
                </div>
              ) : (
                <small className="pending">
                  MP4, MOV, H.264/H.265 and WAV. Unsupported files are reported.
                </small>
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
                  busy ||
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
                {busy ? 'Ingesting and verifying…' : 'Start verified ingest'}
              </button>
              {busy && progress && (
                <button
                  onClick={() => desktopBridge.cancelIngest(progress.jobId)}
                >
                  Cancel safely
                </button>
              )}
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

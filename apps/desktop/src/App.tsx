import { useMemo, useState } from 'react';
import {
  Archive,
  Clock3,
  Folder,
  HardDrive,
  Inbox,
  Library,
  Search,
  Settings,
  Tag,
  UploadCloud,
  UsersRound,
} from 'lucide-react';
import { SearchableComboBox } from './components/SearchableComboBox';
import { demoMedia, demoProjects, demoTags } from './demo';

type View = 'Ingest' | 'Library' | 'Projects' | 'Archive' | 'Settings';
const icons = {
  Ingest: Inbox,
  Library,
  Projects: UsersRound,
  Archive,
  Settings,
};

export function App() {
  const [media, setMedia] = useState(demoMedia);
  const [view, setView] = useState<View>('Library');
  const [query, setQuery] = useState('');
  const [folder, setFolder] = useState('All media');
  const [selected, setSelected] = useState<string[]>(['1', '2']);
  const [batchTagIds, setBatchTagIds] = useState<string[]>([]);
  const [batchProjectIds, setBatchProjectIds] = useState<string[]>([]);
  const [dateMode, setDateMode] = useState<'today' | 'original'>('original');
  const filtered = useMemo(
    () =>
      media.filter(
        (item) =>
          (folder === 'All media' || item.folder === folder) &&
          [
            item.name,
            item.folder,
            ...item.tags,
            ...item.projectIds.map(
              (id) => demoProjects.find((project) => project.id === id)?.name,
            ),
          ]
            .join(' ')
            .toLowerCase()
            .includes(query.toLowerCase()),
      ),
    [folder, media, query],
  );

  const applyBatchTags = (tagIds: string[]) => {
    const addedTagIds = tagIds.filter((id) => !batchTagIds.includes(id));
    setBatchTagIds(tagIds);
    if (!addedTagIds.length) return;
    setMedia((items) =>
      items.map((item) =>
        selected.includes(item.id)
          ? {
              ...item,
              tags: [...new Set([...item.tags, ...addedTagIds])],
            }
          : item,
      ),
    );
  };

  const applyBatchProjects = (projectIds: string[]) => {
    const addedProjectIds = projectIds.filter(
      (id) => !batchProjectIds.includes(id),
    );
    setBatchProjectIds(projectIds);
    if (!addedProjectIds.length) return;
    setMedia((items) =>
      items.map((item) =>
        selected.includes(item.id)
          ? {
              ...item,
              projectIds: [
                ...new Set([...item.projectIds, ...addedProjectIds]),
              ],
            }
          : item,
      ),
    );
  };

  const clearSelection = () => {
    setSelected([]);
    setBatchTagIds([]);
    setBatchProjectIds([]);
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
              <br /> &amp; Media Processor
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
          <div className="bar">
            <i />
          </div>
          <div className="storage-copy">
            <span>Demo usage</span>
            <span>40%</span>
          </div>
        </div>
        <div className="edition">
          <i /> PIMP LIGHT <span>Local AI unavailable</span>
        </div>
      </aside>
      <main>
        <header>
          <div>
            <p>MASTER BIN / MEDIA</p>
            <h1>{view}</h1>
          </div>
          <div className="sync">
            Development fixture <span>local demo data</span>
          </div>
        </header>
        {view === 'Library' ? (
          <>
            <section className="toolbar">
              <label className="search">
                <Search size={17} />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search filenames, folders, tags, projects…"
                />
              </label>
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
                    All media<small>2,418 items</small>
                  </span>
                </button>
                {[
                  '2026-10-21 Race Prep',
                  '2026-10-20 Workshop',
                  '2026-10-19 Track Day',
                  '2026-10-18 Go Kart Repairs',
                ].map((f, i) => (
                  <button
                    className={`folder ${folder === f ? 'active' : ''}`}
                    key={f}
                    onClick={() => setFolder(f)}
                  >
                    <Folder size={16} />
                    <span>
                      {f}
                      <small>{[48, 76, 129, 63][i]} items</small>
                    </span>
                  </button>
                ))}
              </div>
              <div className="library-panel">
                <div className="section-head">
                  <div>
                    <h2>Recent media</h2>
                    <span>{filtered.length} of 2,418 items</span>
                  </div>
                  <span className="view-label">Grid view</span>
                </div>
                <div className="cards">
                  {filtered.map((item) => (
                    <article
                      className={selected.includes(item.id) ? 'selected' : ''}
                      key={item.id}
                      onClick={() =>
                        setSelected((s) =>
                          s.includes(item.id)
                            ? s.filter((id) => id !== item.id)
                            : [...s, item.id],
                        )
                      }
                    >
                      <div
                        className="thumb"
                        style={{ '--hue': item.hue } as React.CSSProperties}
                      >
                        <span>{item.duration}</span>
                        <i className="play">▶</i>
                        {selected.includes(item.id) && (
                          <b className="check">✓</b>
                        )}
                      </div>
                      <div className="card-body">
                        <strong>{item.name}</strong>
                        <small>{item.folder}</small>
                        <div className="pills">
                          {item.tags.map((tag) => (
                            <em key={tag}>{tag}</em>
                          ))}
                        </div>
                        <div className="projects">
                          {item.projectIds.map((projectId) => (
                            <span key={projectId}>
                              {demoProjects.find(
                                (project) => project.id === projectId,
                              )?.name ?? projectId}
                            </span>
                          ))}
                        </div>
                        <footer>
                          <span>
                            <Clock3 size={12} />
                            {item.date}
                          </span>
                          <span>{item.size}</span>
                        </footer>
                      </div>
                    </article>
                  ))}
                </div>
              </div>
            </section>
            {selected.length > 0 && (
              <div className="batch">
                <div className="batch-heading">
                  <strong>{selected.length} selected</strong>
                  <span>Demo fixture changes only</span>
                </div>
                <div className="batch-combo">
                  <Tag aria-hidden="true" size={15} />
                  <SearchableComboBox
                    label="Add tags"
                    mode="multiple"
                    options={demoTags.map((tag) => ({ id: tag, label: tag }))}
                    selectedIds={batchTagIds}
                    onSelectionChange={applyBatchTags}
                    placeholder="Search tags…"
                    onCreate={(label) => ({ id: label, label })}
                  />
                </div>
                <div className="batch-combo">
                  <UsersRound aria-hidden="true" size={15} />
                  <SearchableComboBox
                    label="Assign projects"
                    mode="multiple"
                    options={demoProjects.map((project) => ({
                      id: project.id,
                      label: project.name,
                    }))}
                    selectedIds={batchProjectIds}
                    onSelectionChange={applyBatchProjects}
                    placeholder="Search projects…"
                  />
                </div>
                <button className="batch-clear" onClick={clearSelection}>
                  Clear media selection
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
                Files and folders are copied through a temporary file, then
                independently checksum-verified.
              </p>
              <small className="pending">
                Native folder selection is awaiting the Tauri filesystem bridge.
              </small>
            </div>
            <div className="mode">
              <h3>Date organization</h3>
              <label className={dateMode === 'today' ? 'chosen' : ''}>
                <input
                  type="radio"
                  checked={dateMode === 'today'}
                  onChange={() => setDateMode('today')}
                />
                <span>
                  <b>Use today's date</b>
                  <small>
                    Put this batch in one folder for the local ingest date.
                  </small>
                </span>
              </label>
              <label className={dateMode === 'original' ? 'chosen' : ''}>
                <input
                  type="radio"
                  checked={dateMode === 'original'}
                  onChange={() => setDateMode('original')}
                />
                <span>
                  <b>Use original file dates</b>
                  <small>
                    Organize each recording by its best available capture date.
                  </small>
                </span>
              </label>
            </div>
          </section>
        ) : (
          <section className="placeholder">
            <div>
              <h2>{view} foundation</h2>
              <p>
                This area is intentionally limited to implemented milestone
                capabilities. The architecture and portable records are in place
                for the next increment.
              </p>
            </div>
          </section>
        )}
      </main>
    </div>
  );
}

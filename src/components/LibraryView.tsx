import { useState } from 'react';

import { pickFile, pickFolder } from '../lib/dialogs';
import { fmtDate } from '../lib/format';
import { api, isLite } from '../lib/ipc';
import { useStore } from '../store';
import { PLATFORM_LABEL, type Platform } from '../types';
import { Empty, Field, Panel } from './ui';

export function LibraryView() {
  const entries = useStore((s) => s.entries);
  const info = useStore((s) => s.info);
  const settings = useStore((s) => s.settings);
  const openShow = useStore((s) => s.openShow);
  const refresh = useStore((s) => s.refreshLibrary);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('New show');
  const [platform, setPlatform] = useState<Platform>('ah-sq');
  const [model, setModel] = useState('SQ-5');
  const [importAs, setImportAs] = useState<Platform>('ah-avantis');
  const [filter, setFilter] = useState('');

  const models = info?.platforms.find((p) => p.id === platform)?.models ?? [];

  const done = async (r: { summary: { name: string; channels: number; scenes: number }; kind: string; show: { id: string } }) => {
    toast(`Imported ${r.summary.name} (${r.kind}): ${r.summary.channels} channels, ${r.summary.scenes} scenes`);
    await refresh();
    await openShow(r.show.id);
  };

  const importFile = async () => {
    const path = await pickFile('Show files', ['gz', 'tgz', 'zip', 'dat', 'clf', 'dm3s', 'tfs', 'dm7s', 'dm3p', 'tfp', 'json']);
    if (!path) return;
    const r = await run('Importing', () => api.importPath(path, importAs));
    if (r) await done(r);
  };

  const importFolder = async () => {
    const path = await pickFolder('Import an SQ show folder (holds NVDATA.DAT and SCENEnnn.DAT)');
    if (!path) return;
    const r = await run('Importing', () => api.importPath(path, 'ah-sq'));
    if (r) await done(r);
  };

  const create = async () => {
    const r = await run('Creating', () => api.showNew(name, platform, model));
    if (r) {
      setCreating(false);
      await refresh();
      await openShow(r.id);
    }
  };

  const visible = entries.filter((e) => !filter || `${e.summary.name} ${e.summary.model} ${PLATFORM_LABEL[e.summary.platform]}`.toLowerCase().includes(filter.toLowerCase()));

  return (
    <div className="view">
      <div className="view-head">
        <h2>Library</h2>
        <span className="muted">{settings?.libraryPath}</span>
        <div className="spacer" />
        <input className="search" placeholder="filter" value={filter} onChange={(e) => setFilter(e.target.value)} />
        <select value={importAs} title="A dLive and an Avantis show archive look the same; say which desk a .tar.gz came from" onChange={(e) => setImportAs(e.target.value as Platform)}>
          <option value="ah-avantis">archive is Avantis</option>
          <option value="ah-dlive">archive is dLive</option>
        </select>
        <button type="button" className="btn" onClick={() => void importFile()}>
          Import file…
        </button>
        <button type="button" className="btn" onClick={() => void importFolder()}>
          Import SQ folder…
        </button>
        <button type="button" className="btn primary" onClick={() => setCreating((c) => !c)}>
          New show
        </button>
      </div>
      {creating ? (
        <Panel title="New show">
          <div className="row">
            <Field label="Name">
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </Field>
            <Field label="Platform">
              <select
                value={platform}
                onChange={(e) => {
                  const p = e.target.value as Platform;
                  setPlatform(p);
                  setModel(info?.platforms.find((x) => x.id === p)?.models[0] ?? '');
                }}
              >
                {(info?.platforms ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Model">
              <select value={model} onChange={(e) => setModel(e.target.value)}>
                {models.map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </Field>
            <button type="button" className="btn primary" onClick={() => void create()}>
              Create
            </button>
          </div>
          <div className="muted small">A new show starts with the desk's channels, buses, DCAs and mute groups from its capability table, unnamed.</div>
        </Panel>
      ) : null}
      {visible.length === 0 ? (
        <Empty>
          {entries.length === 0
            ? `No shows yet. Import an SQ show folder, a dLive / Avantis show archive (.tar.gz), a Yamaha .CLF or a DM3 / TF / DM7 scene${isLite ? '' : ', or pull one from a desk'}.`
            : 'Nothing matches the filter.'}
        </Empty>
      ) : (
        <div className="cards">
          {visible.map((e) => (
            <button key={e.summary.id} type="button" className="card" onClick={() => void openShow(e.summary.id)}>
              <div className="card-title">{e.summary.name}</div>
              <div className="card-sub">
                {PLATFORM_LABEL[e.summary.platform]} · {e.summary.model || '—'}
                {e.summary.firmware ? ` · fw ${e.summary.firmware}` : ''}
              </div>
              <div className="card-stats">
                <span>{e.summary.channels} ch</span>
                <span>{e.summary.namedChannels} named</span>
                {e.summary.auxes ? <span>{e.summary.auxes} aux</span> : null}
                {e.summary.groups ? <span>{e.summary.groups} grp</span> : null}
                {e.summary.matrices ? <span>{e.summary.matrices} mtx</span> : null}
                {e.summary.dcas ? <span>{e.summary.dcas} DCA</span> : null}
                <span>{e.summary.scenes} scenes</span>
                {e.summary.cues ? <span>{e.summary.cues} cues</span> : null}
                {e.summary.notesDropped ? <span className="warn">{e.summary.notesDropped} dropped</span> : null}
              </div>
              <div className="card-foot">
                <span>{fmtDate(e.summary.modified)}</span>
                <span>
                  {e.commits} version{e.commits === 1 ? '' : 's'}
                  {e.vendorFiles ? ` · ${e.vendorFiles} vendor file${e.vendorFiles === 1 ? '' : 's'}` : ''}
                </span>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

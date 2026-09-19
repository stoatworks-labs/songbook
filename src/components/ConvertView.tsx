import { useEffect, useState } from 'react';

import { api } from '../lib/ipc';
import { useStore } from '../store';
import { PLATFORM_LABEL, type Capabilities, type Note, type Platform } from '../types';
import { Field, Panel } from './ui';

export function ConvertView() {
  const entries = useStore((s) => s.entries);
  const info = useStore((s) => s.info);
  const show = useStore((s) => s.show);
  const refresh = useStore((s) => s.refreshLibrary);
  const openShow = useStore((s) => s.openShow);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [source, setSource] = useState(show?.id ?? entries[0]?.summary.id ?? '');
  const [target, setTarget] = useState<Platform>('yamaha-dm3');
  const [model, setModel] = useState('DM3');
  const [caps, setCaps] = useState<Capabilities | null>(null);
  const [notes, setNotes] = useState<Note[] | null>(null);
  const [converted, setConverted] = useState<string | null>(null);

  const models = info?.platforms.find((p) => p.id === target)?.models ?? [];
  useEffect(() => {
    void api.capabilities(target, model).then(setCaps).catch(() => setCaps(null));
  }, [target, model]);

  const preview = async () => {
    if (!source) return;
    const r = await run('Converting', () => api.convertShow(source, target, model, false));
    if (r) {
      setNotes(r.notes);
      setConverted(null);
    }
  };
  const commit = async () => {
    if (!source) return;
    const r = await run('Converting', () => api.convertShow(source, target, model, true));
    if (r) {
      setNotes(r.notes);
      setConverted(r.show.id);
      toast(`Saved "${r.show.meta.name}" to the library`);
      await refresh();
    }
  };

  const dropped = notes?.filter((n) => n.level === 'dropped') ?? [];
  const adapted = notes?.filter((n) => n.level === 'adapted') ?? [];
  const infos = notes?.filter((n) => n.level === 'info') ?? [];

  return (
    <div className="view">
      <div className="view-head">
        <h2>Convert</h2>
        <span className="muted">a show for a different desk, with an honest report</span>
      </div>
      <div className="grid-2">
        <Panel title="From → to">
          <Field label="Show">
            <select value={source} onChange={(e) => setSource(e.target.value)}>
              <option value="">—</option>
              {entries.map((e) => (
                <option key={e.summary.id} value={e.summary.id}>
                  {e.summary.name} ({PLATFORM_LABEL[e.summary.platform]} {e.summary.model})
                </option>
              ))}
            </select>
          </Field>
          <div className="row">
            <Field label="Target platform">
              <select
                value={target}
                onChange={(e) => {
                  const p = e.target.value as Platform;
                  setTarget(p);
                  setModel(info?.platforms.find((x) => x.id === p)?.models[0] ?? '');
                }}
              >
                {(info?.platforms ?? []).filter((p) => p.id !== 'generic').map((p) => (
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
          </div>
          <div className="row">
            <button type="button" className="btn" onClick={() => void preview()} disabled={!source}>
              Preview the report
            </button>
            <button type="button" className="btn primary" onClick={() => void commit()} disabled={!source}>
              Convert and save
            </button>
            {converted ? (
              <button type="button" className="btn" onClick={() => void openShow(converted)}>
                Open the converted show
              </button>
            ) : null}
          </div>
        </Panel>
        <Panel title={caps ? `${caps.model} holds` : 'Target'}>
          {caps ? (
            <>
              <div className="stats">
                <div className="stat"><div className="stat-value">{caps.inputChannels}</div><div className="stat-label">input channels</div></div>
                <div className="stat"><div className="stat-value">{caps.stereoInputs}</div><div className="stat-label">stereo inputs</div></div>
                <div className="stat"><div className="stat-value">{caps.localInputs}</div><div className="stat-label">local inputs</div></div>
                <div className="stat"><div className="stat-value">{caps.localOutputs}</div><div className="stat-label">local outputs</div></div>
                <div className="stat"><div className="stat-value">{caps.auxes}</div><div className="stat-label">aux</div></div>
                <div className="stat"><div className="stat-value">{caps.groups}</div><div className="stat-label">groups</div></div>
                <div className="stat"><div className="stat-value">{caps.matrices}</div><div className="stat-label">matrix</div></div>
                <div className="stat"><div className="stat-value">{caps.fxSends}</div><div className="stat-label">FX sends</div></div>
                <div className="stat"><div className="stat-value">{caps.dcas}</div><div className="stat-label">DCAs</div></div>
                <div className="stat"><div className="stat-value">{caps.muteGroups}</div><div className="stat-label">mute groups</div></div>
                <div className="stat"><div className="stat-value">{caps.sceneSlots}</div><div className="stat-label">scenes</div></div>
                <div className="stat"><div className="stat-value">{caps.nameLength}</div><div className="stat-label">name chars</div></div>
              </div>
              <div className="muted small">
                {[caps.mixPool ? `${caps.mixPool} configurable mixes shared by aux and groups` : 'fixed aux / group counts', caps.preampControl ? 'head amps controllable' : 'no head-amp control', caps.cues ? 'cue list' : 'no cue list', `colours: ${caps.colors.join(', ')}`].join(' · ')}
              </div>
              <ul className="notes">
                {caps.notes.map((n) => (
                  <li key={n} className="note note--info">
                    <span>{n}</span>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
        </Panel>
        {notes ? (
          <Panel title={`Report: ${dropped.length} dropped, ${adapted.length} adapted, ${infos.length} notes`} className="span-2">
            <ul className="notes">
              {[...dropped, ...adapted, ...infos].map((n, i) => (
                <li key={i} className={`note note--${n.level}`}>
                  <span className="note-level">{n.level}</span>
                  <span className="note-path">{n.path}</span>
                  <span>{n.message}</span>
                </li>
              ))}
            </ul>
          </Panel>
        ) : null}
      </div>
    </div>
  );
}

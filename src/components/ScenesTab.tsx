import { useState } from 'react';

import { api, toRef } from '../lib/ipc';
import { useStore } from '../store';
import { isAh, type Scene } from '../types';
import { Empty, Field, Panel } from './ui';

export function ScenesTab() {
  const show = useStore((s) => s.show)!;
  const update = useStore((s) => s.update);
  const settings = useStore((s) => s.settings);
  const selected = useStore((s) => s.selectedScene);
  const select = useStore((s) => s.selectScene);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [deviceIdx, setDeviceIdx] = useState(0);
  const devices = (settings?.devices ?? []).filter((d) => d.platform === show.platform);
  const device = devices[deviceIdx];
  const sc = show.scenes.find((s) => s.id === selected) ?? show.scenes[0];

  const edit = (id: string, fn: (s: Scene) => void) =>
    update((sh) => {
      const s = sh.scenes.find((x) => x.id === id);
      if (s) fn(s);
    });

  const add = () => {
    const next = Math.max(0, ...show.scenes.map((s) => s.number ?? 0)) + 1;
    update((sh) => sh.scenes.push({ id: `scene:${next}`, number: next, label: `Scene ${next}`, notes: '', filters: [] }));
  };
  const remove = (id: string) => update((sh) => void (sh.scenes = sh.scenes.filter((s) => s.id !== id)));

  const recall = async (s: Scene) => {
    if (!device || s.number === undefined) return;
    if (!window.confirm(`Recall scene ${s.number} "${s.label}" on ${device.name}? This changes the live mix.`)) return;
    const list = (s.extra?.scpList as string | undefined) ?? (s.bank ? `scene_${s.bank.toLowerCase()}` : undefined);
    const r = await run(`Recalling on ${device.name}`, () => api.deviceScene(toRef(device), { action: 'recall', number: s.number!, list }));
    if (r) toast(`Recalled scene ${s.number}`);
  };
  const store = async (s: Scene) => {
    if (!device || s.number === undefined) return;
    if (!window.confirm(`Store the desk's current mix into scene ${s.number} "${s.label}" on ${device.name}? This overwrites the slot.`)) return;
    const list = (s.extra?.scpList as string | undefined) ?? (s.bank ? `scene_${s.bank.toLowerCase()}` : undefined);
    const r = await run(`Storing on ${device.name}`, () => api.deviceScene(toRef(device), { action: 'store', number: s.number!, list }));
    if (r) toast(`Stored scene ${s.number}`);
  };

  return (
    <div className="split">
      <div className="side wide">
        <Panel
          title={`Scenes (${show.scenes.length})`}
          actions={
            <button type="button" className="btn small" onClick={add}>
              Add scene
            </button>
          }
        >
          {show.scenes.length === 0 ? <Empty>No scenes. A pull over SCP lists the desk's populated slots; an A&H show file lists the stored ones.</Empty> : null}
          <ul className="list">
            {show.scenes.map((s) => (
              <li key={s.id} className={`list-item${s.id === sc?.id ? ' list-item--on' : ''}`} onClick={() => select(s.id)}>
                <strong>
                  {s.bank ? `${s.bank}` : ''}
                  {s.number ?? '—'}
                </strong>{' '}
                {s.label}
                {s.snapshot ? <span className="muted small"> · contents stored</span> : null}
              </li>
            ))}
          </ul>
        </Panel>
      </div>
      <div className="grow">
        {sc ? (
          <Panel
            title={`Scene ${sc.bank ?? ''}${sc.number ?? ''} · ${sc.label}`}
            actions={
              <button type="button" className="btn tiny danger" onClick={() => remove(sc.id)}>
                Remove
              </button>
            }
          >
            <div className="row wrap">
              <Field label="Number">
                <input className="tiny-input" value={sc.number ?? ''} onChange={(e) => edit(sc.id, (x) => void (x.number = e.target.value ? Number(e.target.value) : undefined))} />
              </Field>
              <Field label="Name">
                <input value={sc.label} onChange={(e) => edit(sc.id, (x) => void (x.label = e.target.value))} />
              </Field>
              {sc.bank ? (
                <Field label="Bank">
                  <input value={sc.bank} readOnly />
                </Field>
              ) : null}
            </div>
            <Field label="Notes">
              <textarea rows={4} value={sc.notes} onChange={(e) => edit(sc.id, (x) => void (x.notes = e.target.value))} />
            </Field>
            {sc.filters.length ? <div className="muted small">Recall filters: {sc.filters.join(', ')}</div> : null}
            {sc.snapshot ? (
              <div className="muted small">
                Stored contents: {sc.snapshot.channels.length} channels, {sc.snapshot.buses.length} buses, {sc.snapshot.dcas.length} DCAs.
              </div>
            ) : (
              <div className="muted small">Contents not stored in the library: the desk holds them.</div>
            )}
            <h4>On the desk</h4>
            {devices.length === 0 ? (
              <div className="muted small">Add a {show.platform} desk under Devices to recall or store this scene.</div>
            ) : (
              <div className="row wrap">
                <Field label="Desk">
                  <select value={deviceIdx} onChange={(e) => setDeviceIdx(Number(e.target.value))}>
                    {devices.map((d, i) => (
                      <option key={d.name} value={i}>
                        {d.name} · {d.host}
                      </option>
                    ))}
                  </select>
                </Field>
                <button type="button" className="btn primary" disabled={sc.number === undefined} onClick={() => void recall(sc)}>
                  Recall
                </button>
                {!isAh(show.platform) ? (
                  <button type="button" className="btn danger" disabled={sc.number === undefined} onClick={() => void store(sc)}>
                    Store current mix here
                  </button>
                ) : (
                  <span className="muted small">Allen & Heath desks store scenes from their own screen; MIDI only recalls.</span>
                )}
              </div>
            )}
          </Panel>
        ) : null}
      </div>
    </div>
  );
}

import { useState } from 'react';

import { api, toRef } from '../lib/ipc';
import { useStore } from '../store';
import { PLATFORM_LABEL, isAh, isYamaha, type DeviceEntry, type Platform } from '../types';
import { Empty, Field, Panel } from './ui';

const LIVE: Platform[] = ['ah-sq', 'ah-dlive', 'ah-avantis', 'ah-qu', 'yamaha-cl-ql', 'yamaha-tf', 'yamaha-dm3', 'yamaha-dm7', 'yamaha-rivage'];

export function DevicesView() {
  const settings = useStore((s) => s.settings);
  const info = useStore((s) => s.info);
  const saveSettings = useStore((s) => s.saveSettings);
  const entries = useStore((s) => s.entries);
  const openShow = useStore((s) => s.openShow);
  const refresh = useStore((s) => s.refreshLibrary);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [draft, setDraft] = useState<DeviceEntry>({ name: '', platform: 'ah-sq', host: '', midiChannel: 1, model: 'SQ-5' });
  const [sel, setSel] = useState<number>(0);
  const [probe, setProbe] = useState<Record<string, unknown> | null>(null);
  const [pull, setPull] = useState({ intoShow: '', name: '' });
  const [push, setPush] = useState({ showId: '', names: true, mutes: true, levels: true, sends: false, preamps: false });
  const [log, setLog] = useState<string[]>([]);

  const devices = settings?.devices ?? [];
  const device = devices[sel];
  const models = (p: Platform) => info?.platforms.find((x) => x.id === p)?.models ?? [];

  const add = async () => {
    if (!settings || !draft.name || !draft.host) return;
    await saveSettings({ ...settings, devices: [...settings.devices, { ...draft }] });
    setDraft({ name: '', platform: draft.platform, host: '', midiChannel: 1, model: draft.model });
  };
  const remove = async (i: number) => {
    if (!settings) return;
    await saveSettings({ ...settings, devices: settings.devices.filter((_, k) => k !== i) });
    setSel(0);
  };

  const doProbe = async () => {
    if (!device) return;
    const r = await run(`Probing ${device.name}`, () => api.deviceProbe(toRef(device)));
    if (r) setProbe(r);
  };

  const doPull = async () => {
    if (!device) return;
    const r = await run(`Pulling from ${device.name}`, () => api.devicePull(toRef(device), { intoShow: pull.intoShow || undefined, name: pull.name || undefined }));
    if (r) {
      toast(`Pulled ${r.summary.name}: ${r.summary.channels} channels, ${r.summary.scenes} scenes`);
      await refresh();
      await openShow(r.show.id);
    }
  };

  const doPush = async () => {
    if (!device || !push.showId) return;
    const what = [push.names ? 'names and colours' : '', push.mutes ? 'mutes' : '', push.levels ? 'faders, pans and assignments' : '', push.sends ? 'sends' : '', push.preamps ? 'preamps' : ''].filter(Boolean).join(', ');
    if (!what) return;
    if (!window.confirm(`Push ${what} to ${device.name} (${device.host})? This writes to a live desk and changes the mix.`)) return;
    const r = await run(`Pushing to ${device.name}`, () => api.devicePush(toRef(device), push.showId, { names: push.names, mutes: push.mutes, levels: push.levels, sends: push.sends, preamps: push.preamps }));
    if (r) {
      setLog(r.log);
      toast(`Push to ${device.name} finished`);
    }
  };

  const showsFor = entries.filter((e) => !device || e.summary.platform === device.platform);
  const hint = (p: Platform) => (isAh(p) ? 'the desk\'s IP; MIDI over TCP on port 51325' : isYamaha(p) ? 'the desk\'s IP or name (dm3.local); SCP on port 49280' : '');

  return (
    <div className="view">
      <div className="view-head">
        <h2>Devices</h2>
        <span className="muted">live desks on the network</span>
      </div>
      <div className="grid-2">
        <Panel title="Known desks">
          {devices.length === 0 ? <Empty>No desks yet. Add one below.</Empty> : null}
          <ul className="list">
            {devices.map((d, i) => (
              <li key={`${d.name}${i}`} className={`list-item${i === sel ? ' list-item--on' : ''}`} onClick={() => { setSel(i); setProbe(null); setLog([]); }}>
                <strong>{d.name}</strong> <span className="muted small">{PLATFORM_LABEL[d.platform]} {d.model ?? ''} · {d.host}{isAh(d.platform) ? ` · MIDI ch ${d.midiChannel ?? 1}` : ''}</span>
                <button type="button" className="btn tiny danger" style={{ float: 'right' }} onClick={(e) => { e.stopPropagation(); void remove(i); }}>
                  ×
                </button>
              </li>
            ))}
          </ul>
          <div className="row wrap">
            <Field label="Name">
              <input value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
            </Field>
            <Field label="Platform">
              <select value={draft.platform} onChange={(e) => { const p = e.target.value as Platform; setDraft({ ...draft, platform: p, model: models(p)[0] }); }}>
                {LIVE.map((p) => (
                  <option key={p} value={p}>
                    {PLATFORM_LABEL[p]}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Model">
              <select value={draft.model ?? ''} onChange={(e) => setDraft({ ...draft, model: e.target.value })}>
                {models(draft.platform).map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </Field>
            <Field label="Address" hint={hint(draft.platform)}>
              <input value={draft.host} placeholder={isYamaha(draft.platform) ? 'dm3.local' : '192.168.1.60'} onChange={(e) => setDraft({ ...draft, host: e.target.value })} />
            </Field>
            {isAh(draft.platform) ? (
              <Field label="MIDI channel" hint="Utility → General → MIDI on the desk (the base channel N on dLive / Avantis)">
                <input className="tiny-input" value={draft.midiChannel ?? 1} onChange={(e) => setDraft({ ...draft, midiChannel: Math.max(1, Math.min(16, Number(e.target.value) || 1)) })} />
              </Field>
            ) : null}
            <button type="button" className="btn" onClick={() => void add()}>
              Add desk
            </button>
          </div>
        </Panel>
        <Panel title={device ? `${device.name} · ${device.host}` : 'Select a desk'} actions={device ? <button type="button" className="btn small" onClick={() => void doProbe()}>Probe</button> : null}>
          {probe ? <pre className="pre">{JSON.stringify(probe, null, 1).slice(0, 1500)}</pre> : null}
          {device ? (
            <>
              <h4>Pull the mix from the desk</h4>
              <p className="muted small">
                {isYamaha(device.platform)
                  ? 'Reads every parameter the desk lists over SCP — names, colours, faders, mutes, pans, sends, head amps, DCA and mute-group membership — and its scene list. Read-only.'
                  : device.platform === 'ah-sq' || device.platform === 'ah-qu'
                    ? 'Reads mutes, levels, pans and assignments over MIDI/TCP. The protocol carries no names, so pull into the show that already has them.'
                    : 'Reads names, colours, mutes, faders, main assignments and sends over MIDI/TCP; on a dLive also the MixRack preamps. Read-only.'}
              </p>
              <div className="row wrap">
                <Field label="Into an existing show (optional)">
                  <select value={pull.intoShow} onChange={(e) => setPull({ ...pull, intoShow: e.target.value })}>
                    <option value="">— new show —</option>
                    {showsFor.map((e) => (
                      <option key={e.summary.id} value={e.summary.id}>
                        {e.summary.name}
                      </option>
                    ))}
                  </select>
                </Field>
                <Field label="Name for a new show">
                  <input value={pull.name} placeholder={device.name} onChange={(e) => setPull({ ...pull, name: e.target.value })} />
                </Field>
                <button type="button" className="btn primary" onClick={() => void doPull()}>
                  Pull
                </button>
              </div>
              <h4>Push a show to the desk</h4>
              <div className="row wrap">
                <Field label="Show">
                  <select value={push.showId} onChange={(e) => setPush({ ...push, showId: e.target.value })}>
                    <option value="">—</option>
                    {showsFor.map((e) => (
                      <option key={e.summary.id} value={e.summary.id}>
                        {e.summary.name}
                      </option>
                    ))}
                  </select>
                </Field>
              </div>
              <div className="row wrap">
                <label className="check"><input type="checkbox" checked={push.names} onChange={(e) => setPush({ ...push, names: e.target.checked })} /> names and colours{device.platform === 'ah-sq' || device.platform === 'ah-qu' ? ' (not possible over SQ MIDI)' : ''}</label>
                <label className="check"><input type="checkbox" checked={push.mutes} onChange={(e) => setPush({ ...push, mutes: e.target.checked })} /> mutes</label>
                <label className="check"><input type="checkbox" checked={push.levels} onChange={(e) => setPush({ ...push, levels: e.target.checked })} /> faders, pans, main assignments</label>
                <label className="check"><input type="checkbox" checked={push.sends} onChange={(e) => setPush({ ...push, sends: e.target.checked })} /> sends</label>
                <label className="check"><input type="checkbox" checked={push.preamps} onChange={(e) => setPush({ ...push, preamps: e.target.checked })} /> preamps (gain, phantom){device.platform === 'ah-sq' || device.platform === 'ah-avantis' || device.platform === 'ah-qu' ? ' — not in this protocol' : ''}</label>
              </div>
              <div className="row">
                <button type="button" className="btn danger" disabled={!push.showId} onClick={() => void doPush()}>
                  Push
                </button>
                <span className="muted small">Writes go straight to the live mix. Store a scene on the desk first if you want a way back.</span>
              </div>
              {log.length ? <pre className="pre">{log.join('\n')}</pre> : null}
            </>
          ) : null}
        </Panel>
      </div>
    </div>
  );
}

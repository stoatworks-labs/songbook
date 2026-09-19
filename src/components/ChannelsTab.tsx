import { useState } from 'react';

import { useStore } from '../store';
import { COLORS, COLOR_HEX, FADER_OFF, fmtDb, fmtPan, socketLabel, type Channel, type Send } from '../types';
import { Empty, Field, Panel, TriCheck } from './ui';

const KIND_LABEL = { input: 'Input', 'stereo-input': 'Stereo in', 'fx-return': 'FX return' } as const;

function ColorDot({ color }: { color?: string }) {
  return <span className="dot-color" style={{ background: COLOR_HEX[color ?? 'off'] ?? COLOR_HEX.off }} />;
}

/** A numeric input that treats an empty box as "unknown". */
function DbInput({ value, onChange, off }: { value?: number; onChange: (v: number | undefined) => void; off?: boolean }) {
  const [text, setText] = useState<string | null>(null);
  const shown = text ?? (value === undefined ? '' : value <= FADER_OFF + 0.5 ? '-inf' : String(Math.round(value * 10) / 10));
  return (
    <input
      className="tiny-input"
      value={shown}
      placeholder={off ? '−∞' : '—'}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => {
        if (text === null) return;
        const t = text.trim().toLowerCase();
        onChange(t === '' ? undefined : t === '-inf' || t === 'inf' || t === 'off' ? FADER_OFF : Number.isFinite(Number(t)) ? Number(t) : value);
        setText(null);
      }}
    />
  );
}

export function ChannelsTab() {
  const show = useStore((s) => s.show)!;
  const update = useStore((s) => s.update);
  const selected = useStore((s) => s.selectedChannel);
  const select = useStore((s) => s.selectChannel);
  const [filter, setFilter] = useState('');
  const [kind, setKind] = useState<'all' | Channel['kind']>('all');

  const channels = show.channels.filter((c) => (kind === 'all' || c.kind === kind) && (!filter || `${c.number} ${c.label}`.toLowerCase().includes(filter.toLowerCase())));
  const ch = show.channels.find((c) => c.id === selected) ?? channels[0];
  const sendBuses = show.buses.filter((b) => b.kind !== 'main');
  const inSocket = (c: Channel) => (c.source ? socketLabel(show, c.source) : '');

  const edit = (id: string, fn: (c: Channel) => void) =>
    update((s) => {
      const c = s.channels.find((x) => x.id === id);
      if (c) fn(c);
    });
  const editSend = (id: string, busId: string, fn: (s: Send) => void) =>
    edit(id, (c) => {
      let s = c.sends.find((x) => x.busId === busId);
      if (!s) {
        s = { busId };
        c.sends.push(s);
      }
      fn(s);
    });

  if (show.channels.length === 0) return <Empty>No channels. Import a show file, pull from a desk, or create channels from the desk's capability table (New show).</Empty>;

  return (
    <div className="split">
      <div className="grow">
        <Panel
          title={`Channels (${channels.length})`}
          actions={
            <>
              <select value={kind} onChange={(e) => setKind(e.target.value as typeof kind)}>
                <option value="all">all kinds</option>
                <option value="input">inputs</option>
                <option value="stereo-input">stereo inputs</option>
                <option value="fx-return">FX returns</option>
              </select>
              <input className="search" placeholder="filter" value={filter} onChange={(e) => setFilter(e.target.value)} />
            </>
          }
        >
          <table className="table table--dense">
            <thead>
              <tr>
                <th>#</th>
                <th>Name</th>
                <th>Colour</th>
                <th>Source</th>
                <th>Fader</th>
                <th>On</th>
                <th>Pan</th>
                <th>Main</th>
                <th>DCA</th>
                <th>Mute grp</th>
                <th>HPF</th>
                <th>EQ</th>
                <th>Gate</th>
                <th>Comp</th>
              </tr>
            </thead>
            <tbody>
              {channels.map((c) => (
                <tr key={c.id} className={c.id === ch?.id ? 'row--on' : ''} onClick={() => select(c.id)}>
                  <td className="mono muted">{c.kind === 'input' ? c.number : `${KIND_LABEL[c.kind]} ${c.number}`}</td>
                  <td>
                    <input className="name-input" value={c.label} onChange={(e) => edit(c.id, (x) => void (x.label = e.target.value))} />
                  </td>
                  <td>
                    <span className="color-pick">
                      <ColorDot color={c.color} />
                      <select value={c.color ?? ''} onChange={(e) => edit(c.id, (x) => void (x.color = e.target.value || undefined))}>
                        <option value="">—</option>
                        {COLORS.map((col) => (
                          <option key={col} value={col}>
                            {col}
                          </option>
                        ))}
                      </select>
                    </span>
                  </td>
                  <td className="small">{inSocket(c) || <span className="muted">—</span>}</td>
                  <td>
                    <DbInput value={c.strip.faderDb} off onChange={(v) => edit(c.id, (x) => void (x.strip.faderDb = v))} />
                  </td>
                  <td>
                    <TriCheck value={c.strip.on} onChange={(v) => edit(c.id, (x) => void (x.strip.on = v))} />
                  </td>
                  <td>
                    <input className="tiny-input" value={c.strip.pan === undefined ? '' : Math.round(c.strip.pan * 100)} placeholder="—" title={fmtPan(c.strip.pan)} onChange={(e) => edit(c.id, (x) => void (x.strip.pan = e.target.value === '' ? undefined : Math.max(-1, Math.min(1, Number(e.target.value) / 100))))} />
                  </td>
                  <td>
                    <TriCheck value={c.strip.mainAssign} onChange={(v) => edit(c.id, (x) => void (x.strip.mainAssign = v))} />
                  </td>
                  <td className="small">{c.dcaIds.map((d) => show.dcas.find((x) => x.id === d)?.number ?? d).join(' ')}</td>
                  <td className="small">{c.muteGroupIds.map((m) => show.muteGroups.find((x) => x.id === m)?.number ?? m).join(' ')}</td>
                  <td className="small muted">{c.strip.hpf ? (c.strip.hpf.on ? `${c.strip.hpf.freqHz ? `${Math.round(c.strip.hpf.freqHz)} Hz` : 'on'}` : 'off') : ''}</td>
                  <td className="small muted">{c.strip.eq ? (c.strip.eq.on ? 'on' : 'off') : ''}</td>
                  <td className="small muted">{c.strip.gate ? (c.strip.gate.on ? 'on' : 'off') : ''}</td>
                  <td className="small muted">{c.strip.comp ? (c.strip.comp.on ? 'on' : 'off') : ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Panel>
      </div>
      <div className="side wide">
        {ch ? (
          <Panel title={`${KIND_LABEL[ch.kind]} ${ch.number} · ${ch.label}`}>
            <div className="row wrap">
              <Field label="Name">
                <input value={ch.label} onChange={(e) => edit(ch.id, (x) => void (x.label = e.target.value))} />
              </Field>
              <Field label="Source">
                <select value={ch.source ?? ''} onChange={(e) => edit(ch.id, (x) => void (x.source = e.target.value || undefined))}>
                  <option value="">— unpatched —</option>
                  {show.sockets.filter((s) => s.direction === 'in').map((s) => (
                    <option key={s.id} value={s.id}>
                      {socketLabel(show, s.id)}
                    </option>
                  ))}
                </select>
              </Field>
              <Field label="Trim">
                <DbInput value={ch.strip.trimDb} onChange={(v) => edit(ch.id, (x) => void (x.strip.trimDb = v))} />
              </Field>
              <Field label="Polarity">
                <input type="checkbox" checked={ch.strip.polarity ?? false} onChange={(e) => edit(ch.id, (x) => void (x.strip.polarity = e.target.checked))} />
              </Field>
            </div>
            <div className="row wrap">
              <Field label="DCAs">
                <div className="chips">
                  {show.dcas.map((d) => (
                    <label key={d.id} className="chip">
                      <input type="checkbox" checked={ch.dcaIds.includes(d.id)} onChange={(e) => edit(ch.id, (x) => void (x.dcaIds = e.target.checked ? [...x.dcaIds, d.id] : x.dcaIds.filter((i) => i !== d.id)))} /> {d.number} {d.label}
                    </label>
                  ))}
                  {show.dcas.length === 0 ? <span className="muted small">none</span> : null}
                </div>
              </Field>
              <Field label="Mute groups">
                <div className="chips">
                  {show.muteGroups.map((m) => (
                    <label key={m.id} className="chip">
                      <input type="checkbox" checked={ch.muteGroupIds.includes(m.id)} onChange={(e) => edit(ch.id, (x) => void (x.muteGroupIds = e.target.checked ? [...x.muteGroupIds, m.id] : x.muteGroupIds.filter((i) => i !== m.id)))} /> {m.number} {m.label}
                    </label>
                  ))}
                  {show.muteGroups.length === 0 ? <span className="muted small">none</span> : null}
                </div>
              </Field>
            </div>
            <h4>Sends</h4>
            {sendBuses.length === 0 ? (
              <div className="muted small">No buses to send to.</div>
            ) : (
              <table className="table table--dense">
                <thead>
                  <tr>
                    <th>Bus</th>
                    <th>Level</th>
                    <th>On</th>
                    <th>Pre</th>
                    <th>Pan</th>
                  </tr>
                </thead>
                <tbody>
                  {sendBuses.map((b) => {
                    const s = ch.sends.find((x) => x.busId === b.id);
                    return (
                      <tr key={b.id}>
                        <td>
                          <ColorDot color={b.color} /> {b.label} <span className="muted small">{b.kind}</span>
                        </td>
                        <td>
                          <DbInput value={s?.levelDb} off onChange={(v) => editSend(ch.id, b.id, (x) => void (x.levelDb = v))} />
                          <span className="muted small"> {s?.levelDb !== undefined ? fmtDb(s.levelDb) : ''}</span>
                        </td>
                        <td>
                          <TriCheck value={s?.on} onChange={(v) => editSend(ch.id, b.id, (x) => void (x.on = v))} />
                        </td>
                        <td>
                          <TriCheck value={s?.pre} onChange={(v) => editSend(ch.id, b.id, (x) => void (x.pre = v))} />
                        </td>
                        <td className="small muted">{fmtPan(s?.pan)}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
            {ch.extra && Object.keys(ch.extra).length ? (
              <div className="muted small" style={{ marginTop: 8 }}>
                Vendor: {Object.entries(ch.extra).map(([k, v]) => `${k}=${String(v)}`).join(' · ')}
              </div>
            ) : null}
          </Panel>
        ) : null}
      </div>
    </div>
  );
}

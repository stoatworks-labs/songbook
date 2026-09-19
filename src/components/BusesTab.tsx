import { useStore } from '../store';
import { BUS_KIND_LABEL, COLORS, COLOR_HEX, FADER_OFF, fmtDb, fmtPan, socketLabel, type Bus } from '../types';
import { Empty, Panel, TriCheck } from './ui';

function Dot({ color }: { color?: string }) {
  return <span className="dot-color" style={{ background: COLOR_HEX[color ?? 'off'] ?? COLOR_HEX.off }} />;
}

function LevelInput({ value, onChange }: { value?: number; onChange: (v: number | undefined) => void }) {
  return (
    <input
      className="tiny-input"
      defaultValue={value === undefined ? '' : value <= FADER_OFF + 0.5 ? '-inf' : String(Math.round(value * 10) / 10)}
      key={value ?? 'u'}
      placeholder="—"
      onBlur={(e) => {
        const t = e.target.value.trim().toLowerCase();
        onChange(t === '' ? undefined : t === '-inf' || t === 'off' ? FADER_OFF : Number.isFinite(Number(t)) ? Number(t) : value);
      }}
    />
  );
}

export function BusesTab() {
  const show = useStore((s) => s.show)!;
  const update = useStore((s) => s.update);
  const editBus = (id: string, fn: (b: Bus) => void) =>
    update((s) => {
      const b = s.buses.find((x) => x.id === id);
      if (b) fn(b);
    });
  const order: Bus['kind'][] = ['main', 'aux', 'group', 'fx-send', 'matrix', 'other'];
  const buses = [...show.buses].sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind) || a.number - b.number);
  const matrices = show.buses.filter((b) => b.kind === 'matrix');

  return (
    <div className="grid-2">
      <Panel title={`Buses (${show.buses.length})`} className="span-2">
        {show.buses.length === 0 ? <Empty>No buses.</Empty> : null}
        {show.buses.length ? (
          <table className="table table--dense">
            <thead>
              <tr>
                <th>Kind</th>
                <th>#</th>
                <th>Name</th>
                <th>Colour</th>
                <th>Stereo</th>
                <th>Fader</th>
                <th>On</th>
                <th>Balance</th>
                <th>Feeds from</th>
                {matrices.length ? <th>→ matrix</th> : null}
                <th>Output</th>
              </tr>
            </thead>
            <tbody>
              {buses.map((b) => {
                const feeds = show.channels.filter((c) => c.sends.some((s) => s.busId === b.id && (s.on ?? true) && (s.levelDb === undefined || s.levelDb > FADER_OFF + 0.5))).length;
                const outs = show.outputPatch.filter((o) => o.source.refId === b.id).map((o) => socketLabel(show, o.socketId));
                return (
                  <tr key={b.id}>
                    <td className="muted">{BUS_KIND_LABEL[b.kind]}</td>
                    <td className="mono muted">{b.number}</td>
                    <td>
                      <input className="name-input" value={b.label} onChange={(e) => editBus(b.id, (x) => void (x.label = e.target.value))} />
                    </td>
                    <td>
                      <span className="color-pick">
                        <Dot color={b.color} />
                        <select value={b.color ?? ''} onChange={(e) => editBus(b.id, (x) => void (x.color = e.target.value || undefined))}>
                          <option value="">—</option>
                          {COLORS.map((c) => (
                            <option key={c} value={c}>
                              {c}
                            </option>
                          ))}
                        </select>
                      </span>
                    </td>
                    <td>
                      <input type="checkbox" checked={b.stereo} onChange={(e) => editBus(b.id, (x) => void (x.stereo = e.target.checked))} />
                    </td>
                    <td>
                      <LevelInput value={b.strip.faderDb} onChange={(v) => editBus(b.id, (x) => void (x.strip.faderDb = v))} />
                      <span className="muted small"> {fmtDb(b.strip.faderDb)}</span>
                    </td>
                    <td>
                      <TriCheck value={b.strip.on} onChange={(v) => editBus(b.id, (x) => void (x.strip.on = v))} />
                    </td>
                    <td className="small muted">{fmtPan(b.strip.pan)}</td>
                    <td className="small">{b.kind === 'main' ? `${show.channels.filter((c) => c.strip.mainAssign).length} assigned` : feeds ? `${feeds} channel${feeds === 1 ? '' : 's'}` : <span className="muted">—</span>}</td>
                    {matrices.length ? (
                      <td className="small">
                        {b.kind !== 'matrix' ? b.sends.filter((s) => matrices.some((m) => m.id === s.busId)).map((s) => `${matrices.find((m) => m.id === s.busId)?.label}: ${fmtDb(s.levelDb)}`).join(', ') : ''}
                      </td>
                    ) : null}
                    <td className="small">{outs.length ? outs.join(', ') : <span className="muted">—</span>}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : null}
      </Panel>
      <Panel title={`DCAs (${show.dcas.length})`}>
        {show.dcas.length === 0 ? <Empty>No DCAs on this desk.</Empty> : null}
        {show.dcas.length ? (
          <table className="table table--dense">
            <thead>
              <tr>
                <th>#</th>
                <th>Name</th>
                <th>Colour</th>
                <th>Fader</th>
                <th>On</th>
                <th>Members</th>
              </tr>
            </thead>
            <tbody>
              {show.dcas.map((d) => (
                <tr key={d.id}>
                  <td className="mono muted">{d.number}</td>
                  <td>
                    <input className="name-input" value={d.label} onChange={(e) => update((s) => void (s.dcas.find((x) => x.id === d.id)!.label = e.target.value))} />
                  </td>
                  <td>
                    <span className="color-pick">
                      <Dot color={d.color} />
                      <select value={d.color ?? ''} onChange={(e) => update((s) => void (s.dcas.find((x) => x.id === d.id)!.color = e.target.value || undefined))}>
                        <option value="">—</option>
                        {COLORS.map((c) => (
                          <option key={c} value={c}>
                            {c}
                          </option>
                        ))}
                      </select>
                    </span>
                  </td>
                  <td>
                    <LevelInput value={d.faderDb} onChange={(v) => update((s) => void (s.dcas.find((x) => x.id === d.id)!.faderDb = v))} />
                  </td>
                  <td>
                    <TriCheck value={d.on} onChange={(v) => update((s) => void (s.dcas.find((x) => x.id === d.id)!.on = v))} />
                  </td>
                  <td className="small">{show.channels.filter((c) => c.dcaIds.includes(d.id)).map((c) => c.label).join(', ') || <span className="muted">—</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : null}
      </Panel>
      <Panel title={`Mute groups (${show.muteGroups.length})`}>
        {show.muteGroups.length === 0 ? <Empty>No mute groups on this desk.</Empty> : null}
        {show.muteGroups.length ? (
          <table className="table table--dense">
            <thead>
              <tr>
                <th>#</th>
                <th>Name</th>
                <th>Active</th>
                <th>Members</th>
              </tr>
            </thead>
            <tbody>
              {show.muteGroups.map((m) => (
                <tr key={m.id}>
                  <td className="mono muted">{m.number}</td>
                  <td>
                    <input className="name-input" value={m.label} onChange={(e) => update((s) => void (s.muteGroups.find((x) => x.id === m.id)!.label = e.target.value))} />
                  </td>
                  <td>
                    <TriCheck value={m.on} onChange={(v) => update((s) => void (s.muteGroups.find((x) => x.id === m.id)!.on = v))} />
                  </td>
                  <td className="small">{show.channels.filter((c) => c.muteGroupIds.includes(m.id)).map((c) => c.label).join(', ') || <span className="muted">—</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : null}
      </Panel>
    </div>
  );
}

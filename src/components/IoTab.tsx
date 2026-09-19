import { useStore } from '../store';
import { fmtDb, socketLabel, type Socket } from '../types';
import { Empty, Panel } from './ui';

const KIND: Record<string, string> = { mic: 'mic/line', line: 'line', aes: 'AES', dante: 'Dante', 's-link': 'SLink', usb: 'USB', madi: 'MADI', card: 'card', internal: 'internal', other: '' };

export function IoTab() {
  const show = useStore((s) => s.show)!;
  const update = useStore((s) => s.update);
  const ins = show.sockets.filter((s) => s.direction === 'in');
  const outs = show.sockets.filter((s) => s.direction === 'out');

  const feeds = (s: Socket) => show.channels.filter((c) => c.source === s.id || c.sourceRight === s.id);
  const carries = (s: Socket) => show.outputPatch.find((o) => o.socketId === s.id);
  const preamp = (s: Socket) => show.preamps.find((p) => p.socketId === s.id);

  const setPreamp = (socketId: string, fn: (p: { gainDb?: number; pad?: boolean; phantom?: boolean }) => void) =>
    update((sh) => {
      let p = sh.preamps.find((x) => x.socketId === socketId);
      if (!p) {
        p = { socketId };
        sh.preamps.push(p);
      }
      fn(p);
    });

  return (
    <div className="grid-2">
      <Panel title={`Units (${show.system.units.length})`} className="span-2">
        {show.system.units.length === 0 ? <Empty>No units recorded — the file did not say what boxes the desk has.</Empty> : null}
        <div className="chips">
          {show.system.units.map((u) => (
            <span key={u.id} className="chip">
              <strong>{u.label}</strong> {u.model ? `· ${u.model}` : ''} · {u.role}
              {u.address ? ` · ${u.address}` : ''} · {show.sockets.filter((s) => s.unitId === u.id && s.direction === 'in').length} in / {show.sockets.filter((s) => s.unitId === u.id && s.direction === 'out').length} out
            </span>
          ))}
        </div>
      </Panel>
      <Panel title={`Input sockets (${ins.length})`}>
        {ins.length === 0 ? <Empty>No input sockets. A pull over SCP or an import with a patch table fills this in.</Empty> : null}
        {ins.length ? (
          <table className="table">
            <thead>
              <tr>
                <th>Socket</th>
                <th>Kind</th>
                <th>Feeds</th>
                <th>Gain</th>
                <th>Pad</th>
                <th>48V</th>
              </tr>
            </thead>
            <tbody>
              {ins.map((s) => {
                const p = preamp(s);
                const chans = feeds(s);
                return (
                  <tr key={s.id}>
                    <td>{socketLabel(show, s.id)}</td>
                    <td className="muted">{KIND[s.kind] ?? s.kind}</td>
                    <td>{chans.length ? chans.map((c) => c.label).join(', ') : <span className="muted">—</span>}</td>
                    <td>
                      {s.kind === 'mic' || p ? (
                        <input className="tiny-input" value={p?.gainDb ?? ''} placeholder={p ? '—' : ''} title={fmtDb(p?.gainDb)} onChange={(e) => setPreamp(s.id, (x) => void (x.gainDb = e.target.value === '' ? undefined : Number(e.target.value)))} />
                      ) : (
                        <span className="muted">—</span>
                      )}
                    </td>
                    <td>{p ? <input type="checkbox" checked={!!p.pad} onChange={(e) => setPreamp(s.id, (x) => void (x.pad = e.target.checked))} /> : null}</td>
                    <td>{p || s.kind === 'mic' ? <input type="checkbox" checked={!!p?.phantom} onChange={(e) => setPreamp(s.id, (x) => void (x.phantom = e.target.checked))} /> : null}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : null}
      </Panel>
      <Panel title={`Output sockets (${outs.length})`}>
        {outs.length === 0 ? <Empty>No output sockets recorded.</Empty> : null}
        {outs.length ? (
          <table className="table">
            <thead>
              <tr>
                <th>Socket</th>
                <th>Kind</th>
                <th>Carries</th>
              </tr>
            </thead>
            <tbody>
              {outs.map((s) => {
                const o = carries(s);
                const ref = o?.source.refId ? (show.buses.find((b) => b.id === o.source.refId)?.label ?? show.channels.find((c) => c.id === o.source.refId)?.label) : undefined;
                return (
                  <tr key={s.id}>
                    <td>{socketLabel(show, s.id)}</td>
                    <td className="muted">{KIND[s.kind] ?? s.kind}</td>
                    <td>{o ? `${ref ?? o.source.label}${ref && o.source.label && ref !== o.source.label ? ` (${o.source.label})` : ''}` : <span className="muted">not read</span>}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : null}
      </Panel>
    </div>
  );
}

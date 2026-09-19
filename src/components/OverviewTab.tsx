import { useEffect, useState } from 'react';

import { fmtDate } from '../lib/format';
import { api, inTauri } from '../lib/ipc';
import { useStore } from '../store';
import { PLATFORM_LABEL } from '../types';
import { Field, Panel, Stat } from './ui';

export function OverviewTab() {
  const show = useStore((s) => s.show)!;
  const info = useStore((s) => s.info);
  const update = useStore((s) => s.update);
  const [problems, setProblems] = useState<string[]>([]);

  useEffect(() => {
    if (!inTauri) return;
    let live = true;
    void api.showValidate(show).then((p) => live && setProblems(p)).catch(() => {});
    return () => {
      live = false;
    };
  }, [show]);

  const models = info?.platforms.find((p) => p.id === show.platform)?.models ?? [];
  const inputs = show.channels.filter((c) => c.kind === 'input');
  const named = inputs.filter((c) => c.label.trim() && !/^(ip|ch)\s?\d+$/i.test(c.label.trim()));
  const patched = inputs.filter((c) => c.source);
  const dropped = show.notes.filter((n) => n.level === 'dropped');
  const adapted = show.notes.filter((n) => n.level === 'adapted');
  const infos = show.notes.filter((n) => n.level === 'info');
  const count = (kind: string) => show.buses.filter((b) => b.kind === kind).length;

  return (
    <div className="grid-2">
      <Panel title="Desk">
        <div className="row wrap">
          <Field label="Platform">
            <input value={PLATFORM_LABEL[show.platform]} readOnly />
          </Field>
          <Field label="Model">
            <input list="models" value={show.system.model} onChange={(e) => update((s) => void (s.system.model = e.target.value))} />
            <datalist id="models">
              {models.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </Field>
          <Field label="Firmware">
            <input value={show.system.firmware} onChange={(e) => update((s) => void (s.system.firmware = e.target.value))} />
          </Field>
          <Field label="Desk name">
            <input value={show.system.name} onChange={(e) => update((s) => void (s.system.name = e.target.value))} />
          </Field>
          <Field label="Sample rate">
            <input value={show.system.sampleRate ?? ''} placeholder="48000" onChange={(e) => update((s) => void (s.system.sampleRate = e.target.value ? Number(e.target.value) : undefined))} />
          </Field>
          <Field label="Clock">
            <input value={show.system.clockSource ?? ''} placeholder="internal" onChange={(e) => update((s) => void (s.system.clockSource = e.target.value || undefined))} />
          </Field>
        </div>
        <Field label="Tags (comma separated)">
          <input value={show.meta.tags.join(', ')} onChange={(e) => update((s) => void (s.meta.tags = e.target.value.split(',').map((t) => t.trim()).filter(Boolean)))} />
        </Field>
        <Field label="Show notes">
          <textarea rows={6} value={show.meta.notes} onChange={(e) => update((s) => void (s.meta.notes = e.target.value))} />
        </Field>
        <div className="muted small">
          Created {fmtDate(show.meta.created)} · modified {fmtDate(show.meta.modified)}
          {show.meta.author ? ` · ${show.meta.author}` : ''}
          {show.meta.source ? ` · from ${show.meta.source.kind} ${show.meta.source.origin} at ${fmtDate(show.meta.source.at)}` : ''}
        </div>
      </Panel>
      <Panel title="At a glance">
        <div className="stats">
          <Stat label="input channels" value={inputs.length} />
          <Stat label="named" value={named.length} />
          <Stat label="patched" value={patched.length} />
          <Stat label="stereo / FX rtn" value={show.channels.length - inputs.length} />
          <Stat label="aux" value={count('aux')} />
          <Stat label="groups" value={count('group')} />
          <Stat label="matrix" value={count('matrix')} />
          <Stat label="FX sends" value={count('fx-send')} />
          <Stat label="DCAs" value={show.dcas.length} />
          <Stat label="mute groups" value={show.muteGroups.length} />
          <Stat label="scenes" value={show.scenes.length} />
          <Stat label="cues" value={show.cues.length} />
          <Stat label="sockets in" value={show.sockets.filter((s) => s.direction === 'in').length} />
          <Stat label="sockets out" value={show.sockets.filter((s) => s.direction === 'out').length} />
          <Stat label="preamps read" value={show.preamps.length} />
        </div>
        <div className="muted small">
          {show.system.units.length ? `Units: ${show.system.units.map((u) => `${u.label}${u.model ? ` (${u.model})` : ''}`).join(', ')}` : 'No units recorded.'}
        </div>
        {problems.length ? (
          <div className="problems">
            <strong>{problems.length} reference problem{problems.length === 1 ? '' : 's'}</strong>
            <ul>
              {problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </Panel>
      {show.notes.length ? (
        <Panel title={`Import & conversion notes (${dropped.length} dropped, ${adapted.length} adapted, ${infos.length} info)`} className="span-2">
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
  );
}

import { useEffect, useState } from 'react';

import { api } from '../lib/ipc';
import { useStore } from '../store';
import type { Change, Commit } from '../types';
import { fmtDate } from '../lib/format';
import { Panel } from './ui';

export function HistoryTab() {
  const show = useStore((s) => s.show)!;
  const dirty = useStore((s) => s.dirty);
  const openShow = useStore((s) => s.openShow);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [commits, setCommits] = useState<Commit[]>([]);
  const [sel, setSel] = useState<string | null>(null);
  const [diff, setDiff] = useState<Change[]>([]);

  useEffect(() => {
    void api.showHistory(show.id).then((c) => setCommits([...c].reverse())).catch(() => setCommits([]));
  }, [show.id, show.meta.modified]);

  useEffect(() => {
    const c = commits.find((x) => x.id === sel);
    if (!sel || !c?.parent) {
      void Promise.resolve([]).then(setDiff);
      return;
    }
    void api.showDiff(show.id, c.parent, c.id).then(setDiff).catch(() => setDiff([]));
  }, [sel, commits, show.id]);

  const restore = async (c: Commit) => {
    if (dirty && !window.confirm('You have unsaved changes; restoring will discard them. Continue?')) return;
    const r = await run('Restoring', () => api.showRestore(show.id, c.id));
    if (r) {
      toast(`Restored ${c.id}`);
      await openShow(show.id);
    }
  };

  const fmt = (v: unknown) => (v === undefined ? '∅' : typeof v === 'string' ? `"${v}"` : JSON.stringify(v).slice(0, 80));

  return (
    <div className="split">
      <div className="side wide">
        <Panel title={`Versions (${commits.length})`}>
          {dirty ? <div className="muted small">Unsaved edits are not a version yet — Save version to record them.</div> : null}
          <ul className="list">
            {commits.map((c, i) => (
              <li key={c.id} className={`list-item${c.id === sel ? ' list-item--on' : ''}`} onClick={() => setSel(c.id)}>
                <div>
                  <strong>{c.message}</strong> <span className="mono muted">{c.id}</span>
                </div>
                <div className="muted small">
                  {fmtDate(c.at)}
                  {c.author ? ` · ${c.author}` : ''} · {c.changes} change{c.changes === 1 ? '' : 's'} · {c.summary.channels} channels, {c.summary.scenes} scenes
                  {i === 0 ? ' · current' : ''}
                </div>
                {i !== 0 ? (
                  <button type="button" className="btn tiny" onClick={(e) => { e.stopPropagation(); void restore(c); }}>
                    Restore this version
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        </Panel>
      </div>
      <div className="grow">
        <Panel title={sel ? `Changes in ${sel}` : 'Select a version'}>
          {sel && diff.length === 0 ? <div className="muted">{commits.find((c) => c.id === sel)?.parent ? 'No differences recorded.' : 'First version — nothing before it to compare with.'}</div> : null}
          <ul className="diff">
            {diff.map((d, i) => (
              <li key={i} className={`diff--${d.kind}`}>
                <span className="diff-kind">{d.kind === 'added' ? '+' : d.kind === 'removed' ? '−' : '~'}</span>
                <span className="mono">{d.path}</span>
                {d.kind === 'changed' ? (
                  <span className="muted">
                    {' '}
                    {fmt(d.before)} → {fmt(d.after)}
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </Panel>
      </div>
    </div>
  );
}

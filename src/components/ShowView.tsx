import { useState } from 'react';

import { api } from '../lib/ipc';
import { useStore, type ShowTab } from '../store';
import { PLATFORM_LABEL } from '../types';
import { BusesTab } from './BusesTab';
import { ChannelsTab } from './ChannelsTab';
import { CuesTab } from './CuesTab';
import { ExportTab } from './ExportTab';
import { HistoryTab } from './HistoryTab';
import { IoTab } from './IoTab';
import { OverviewTab } from './OverviewTab';
import { ScenesTab } from './ScenesTab';
import { VendorTab } from './VendorTab';

const TABS: { id: ShowTab; label: string }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'io', label: 'I/O & patch' },
  { id: 'channels', label: 'Channels' },
  { id: 'buses', label: 'Buses & outputs' },
  { id: 'scenes', label: 'Scenes' },
  { id: 'cues', label: 'Cues' },
  { id: 'export', label: 'Documents & export' },
  { id: 'vendor', label: 'Vendor files' },
  { id: 'history', label: 'History' },
];

export function ShowView() {
  const show = useStore((s) => s.show)!;
  const dirty = useStore((s) => s.dirty);
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  const update = useStore((s) => s.update);
  const save = useStore((s) => s.save);
  const closeShow = useStore((s) => s.closeShow);
  const refresh = useStore((s) => s.refreshLibrary);
  const openShow = useStore((s) => s.openShow);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [message, setMessage] = useState('');

  const doSave = async () => {
    await save(message || 'Edit');
    setMessage('');
  };

  const duplicate = async () => {
    const name = window.prompt('Name for the copy', `${show.meta.name} copy`);
    if (!name) return;
    const r = await run('Duplicating', () => api.showDuplicate(show.id, name));
    if (r) {
      await refresh();
      await openShow(r.id);
    }
  };

  const remove = async () => {
    if (!window.confirm(`Delete "${show.meta.name}" and its whole history from the library? This cannot be undone.`)) return;
    const ok = await run('Deleting', () => api.showDelete(show.id));
    if (ok !== undefined) {
      toast('Deleted');
      closeShow();
      await refresh();
    }
  };

  return (
    <div className="view">
      <div className="view-head">
        <input className="title-input" value={show.meta.name} onChange={(e) => update((s) => void (s.meta.name = e.target.value))} />
        <span className="muted">
          {PLATFORM_LABEL[show.platform]} · {show.system.model || 'model not set'}
          {show.system.firmware ? ` · fw ${show.system.firmware}` : ''}
        </span>
        <div className="spacer" />
        <input className="search" placeholder="what changed (commit message)" value={message} onChange={(e) => setMessage(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && void doSave()} />
        <button type="button" className={`btn${dirty ? ' primary' : ''}`} onClick={() => void doSave()}>
          {dirty ? 'Save version' : 'Saved'}
        </button>
        <button type="button" className="btn" onClick={() => void duplicate()}>
          Duplicate
        </button>
        <button type="button" className="btn danger" onClick={() => void remove()}>
          Delete
        </button>
        <button type="button" className="btn" onClick={closeShow}>
          Close
        </button>
      </div>
      <nav className="tabs">
        {TABS.map((t) => (
          <button key={t.id} type="button" className={`tab${tab === t.id ? ' tab--on' : ''}`} onClick={() => setTab(t.id)}>
            {t.label}
          </button>
        ))}
      </nav>
      <div className="tab-body">
        {tab === 'overview' ? <OverviewTab /> : null}
        {tab === 'io' ? <IoTab /> : null}
        {tab === 'channels' ? <ChannelsTab /> : null}
        {tab === 'buses' ? <BusesTab /> : null}
        {tab === 'scenes' ? <ScenesTab /> : null}
        {tab === 'cues' ? <CuesTab /> : null}
        {tab === 'export' ? <ExportTab /> : null}
        {tab === 'vendor' ? <VendorTab /> : null}
        {tab === 'history' ? <HistoryTab /> : null}
      </div>
    </div>
  );
}

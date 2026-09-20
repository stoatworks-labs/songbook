import { useEffect } from 'react';

import { ConvertView } from './components/ConvertView';
import { DevicesView } from './components/DevicesView';
import { LibraryView } from './components/LibraryView';
import { SettingsView } from './components/SettingsView';
import { ShowView } from './components/ShowView';
import { SyncView } from './components/SyncView';
import { inTauri, isLite } from './lib/ipc';
import { LITE_FULL_APP_LABEL, LITE_FULL_APP_URL } from './lib/lite-links';
import { useStore, type View } from './store';

const NAV: { id: View; label: string; lite: boolean }[] = [
  { id: 'library', label: 'Library', lite: true },
  { id: 'devices', label: 'Devices', lite: false },
  { id: 'convert', label: 'Convert', lite: true },
  { id: 'sync', label: 'Sync', lite: false },
  { id: 'settings', label: 'Settings', lite: true },
];

export function App() {
  const load = useStore((s) => s.load);
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const show = useStore((s) => s.show);
  const dirty = useStore((s) => s.dirty);
  const busy = useStore((s) => s.busy);
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismiss);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="app">
      <header className="top">
        <h1>
          Song<span>book</span>
          {isLite ? <em className="lite-mark">Lite</em> : null}
        </h1>
        <span className="tag">{isLite ? 'show files for digital mixing desks — in your browser' : 'show file library for digital mixing desks'}</span>
        <span className="ver">{__APP_VERSION__}</span>
        <div className="spacer" />
        {busy ? <span className="pill busy">{busy}…</span> : null}
        {isLite ? (
          <span className="pill lite">
            files only, kept in this browser ·{' '}
            <a href={LITE_FULL_APP_URL} target="_blank" rel="noreferrer">
              {LITE_FULL_APP_LABEL}
            </a>{' '}
            adds live desks and cloud sync
          </span>
        ) : !inTauri ? (
          <span className="pill warn">browser demo — two example shows in memory; files and desks need the desktop app</span>
        ) : null}
        <button type="button" className="btn small" data-stoatworks-about>
          About
        </button>
      </header>
      <div className="main">
        <nav className="sidebar">
          {NAV.filter((n) => !isLite || n.lite).map((n) => (
            <button key={n.id} type="button" className={`nav${view === n.id ? ' nav--on' : ''}`} onClick={() => setView(n.id)}>
              {n.label}
            </button>
          ))}
          {show ? (
            <>
              <div className="nav-sep">open show</div>
              <button type="button" className={`nav nav-show${view === 'show' ? ' nav--on' : ''}`} onClick={() => setView('show')} title={show.meta.name}>
                {show.meta.name}
                {dirty ? <span className="dot" title="unsaved changes" /> : null}
              </button>
            </>
          ) : null}
        </nav>
        <section className="content">
          {view === 'library' ? <LibraryView /> : null}
          {view === 'show' ? show ? <ShowView /> : <LibraryView /> : null}
          {view === 'devices' ? <DevicesView /> : null}
          {view === 'convert' ? <ConvertView /> : null}
          {view === 'sync' ? <SyncView /> : null}
          {view === 'settings' ? <SettingsView /> : null}
        </section>
      </div>
      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast--${t.kind}`} onClick={() => dismiss(t.id)}>
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}

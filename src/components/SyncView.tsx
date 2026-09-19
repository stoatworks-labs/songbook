import { openUrl } from '@tauri-apps/plugin-opener';
import { useState } from 'react';

import { pickFolder } from '../lib/dialogs';
import { api } from '../lib/ipc';
import { useStore } from '../store';
import type { SyncReport } from '../types';
import { Field, Panel } from './ui';

const PROVIDERS = [
  { id: 'none', label: 'Off' },
  { id: 'folder', label: 'A folder (Dropbox / OneDrive / Google Drive desktop client, a NAS…)' },
  { id: 'dropbox', label: 'Dropbox (API)' },
  { id: 'google', label: 'Google Drive (API)' },
  { id: 'onedrive', label: 'OneDrive (API)' },
];

export function SyncView() {
  const settings = useStore((s) => s.settings);
  const saveSettings = useStore((s) => s.saveSettings);
  const refresh = useStore((s) => s.refreshLibrary);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [report, setReport] = useState<SyncReport | null>(null);
  const [waiting, setWaiting] = useState(false);
  if (!settings) return null;
  const s = settings.sync;
  const set = (patch: Partial<typeof s>) => void saveSettings({ ...settings, sync: { ...s, ...patch } });
  const cloud = s.provider === 'dropbox' || s.provider === 'google' || s.provider === 'onedrive';

  const chooseFolder = async () => {
    const picked = await pickFolder('Folder to mirror the library into');
    if (picked) set({ root: picked });
  };

  const connect = async () => {
    const url = await run('Starting sign-in', () => api.oauthBegin(s.provider, s.clientId, s.clientSecret));
    if (!url) return;
    setWaiting(true);
    await openUrl(url);
    const t = await run('Waiting for the browser', () => api.oauthFinish());
    setWaiting(false);
    if (t) {
      toast(`Connected to ${s.provider}`);
      const fresh = await api.settingsGet();
      await saveSettings(fresh);
    }
  };

  const doSync = async (direction: 'both' | 'push' | 'pull') => {
    const r = await run('Syncing', () => api.syncRun(direction));
    if (r) {
      setReport(r);
      set({ lastRun: new Date().toISOString() });
      await refresh();
      toast(`Sync: ${r.uploaded.length} up, ${r.downloaded.length} down, ${r.unchanged} unchanged${r.errors.length ? `, ${r.errors.length} errors` : ''}`);
    }
  };

  return (
    <div className="view">
      <div className="view-head">
        <h2>Sync</h2>
        <span className="muted">mirror the library with a folder or a cloud drive</span>
      </div>
      <div className="grid-2">
        <Panel title="Where">
          <Field label="Provider">
            <select value={s.provider || 'none'} onChange={(e) => set({ provider: e.target.value })}>
              {PROVIDERS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </Field>
          {s.provider === 'folder' ? (
            <div className="row">
              <Field label="Folder">
                <input value={s.root} onChange={(e) => set({ root: e.target.value })} />
              </Field>
              <button type="button" className="btn" onClick={() => void chooseFolder()}>
                Choose…
              </button>
            </div>
          ) : null}
          {cloud ? (
            <>
              <Field label="Remote folder" hint="created if it does not exist">
                <input value={s.root} placeholder={s.provider === 'dropbox' ? '/Songbook' : 'Songbook'} onChange={(e) => set({ root: e.target.value })} />
              </Field>
              <Field label="App / client id" hint="from your own app registration with the provider (Dropbox App Console, Google Cloud OAuth client of type Desktop, Microsoft Entra app with a public client and the loopback redirect)">
                <input value={s.clientId} onChange={(e) => set({ clientId: e.target.value })} />
              </Field>
              {s.provider === 'google' ? (
                <Field label="Client secret" hint="Google issues one for desktop clients; it is not treated as confidential">
                  <input value={s.clientSecret ?? ''} onChange={(e) => set({ clientSecret: e.target.value || undefined })} />
                </Field>
              ) : null}
              <div className="row">
                <button type="button" className="btn" disabled={!s.clientId || waiting} onClick={() => void connect()}>
                  {s.tokens ? 'Reconnect' : 'Connect'} ({s.provider})
                </button>
                {s.tokens ? <span className="muted small">connected{s.tokens.account ? ` as ${s.tokens.account}` : ''}{s.tokens.expiresAt ? ` · token to ${new Date(s.tokens.expiresAt * 1000).toLocaleTimeString()}` : ''}</span> : null}
                {s.tokens?.refreshToken ? (
                  <button type="button" className="btn small" onClick={() => void run('Refreshing token', () => api.oauthRefresh())}>
                    Refresh token
                  </button>
                ) : null}
              </div>
            </>
          ) : null}
          <p className="muted small">
            A sync copies every show file that is newer on one side to the other and never deletes anything; every version the app saved stays in the show's history, so an overwrite of show.json loses nothing. The simplest reliable setup is a library folder inside a drive's desktop client — pick it under Settings — and leaving this off.
          </p>
        </Panel>
        <Panel title="Run">
          <div className="row">
            <button type="button" className="btn primary" disabled={!s.provider || s.provider === 'none'} onClick={() => void doSync('both')}>
              Sync both ways
            </button>
            <button type="button" className="btn" disabled={!s.provider || s.provider === 'none'} onClick={() => void doSync('push')}>
              Push only
            </button>
            <button type="button" className="btn" disabled={!s.provider || s.provider === 'none'} onClick={() => void doSync('pull')}>
              Pull only
            </button>
          </div>
          {s.lastRun ? <div className="muted small">last run {new Date(s.lastRun).toLocaleString()}</div> : null}
          {report ? (
            <div className="report">
              <div>
                {report.uploaded.length} uploaded · {report.downloaded.length} downloaded · {report.unchanged} unchanged · {report.errors.length} errors
              </div>
              <ul className="small">
                {report.uploaded.map((p) => (
                  <li key={`u${p}`}>↑ {p}</li>
                ))}
                {report.downloaded.map((p) => (
                  <li key={`d${p}`}>↓ {p}</li>
                ))}
                {report.errors.map((p) => (
                  <li key={`e${p}`} className="bad">
                    ! {p}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </Panel>
      </div>
    </div>
  );
}

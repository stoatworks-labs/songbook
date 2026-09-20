
import { pickFolder } from '../lib/dialogs';
import { isLite } from '../lib/ipc';
import { LITE_FULL_APP_LABEL, LITE_FULL_APP_URL } from '../lib/lite-links';
import { useStore } from '../store';
import { Field, Panel } from './ui';

export function SettingsView() {
  const settings = useStore((s) => s.settings);
  const info = useStore((s) => s.info);
  const saveSettings = useStore((s) => s.saveSettings);
  if (!settings) return null;

  const pick = async () => {
    const picked = await pickFolder('Library folder');
    if (picked) await saveSettings({ ...settings, libraryPath: picked });
  };

  return (
    <div className="view">
      <div className="view-head">
        <h2>Settings</h2>
      </div>
      <div className="grid-2">
        <Panel title="Library">
          {isLite ? (
            <p className="muted small">
              Shows, their versions and the vendor files they came in with are kept in this browser's IndexedDB — nothing is uploaded, and clearing site data removes them. Export a show as JSON to keep a copy elsewhere. The{' '}
              <a href={LITE_FULL_APP_URL} target="_blank" rel="noreferrer">
                {LITE_FULL_APP_LABEL}
              </a>{' '}
              keeps the library as plain files, syncs it through a drive folder and talks to live desks.
            </p>
          ) : (
            <div className="row">
              <Field label="Folder" hint="plain files: shows/<id>/show.json, history/, vendor/. Put it inside a synced drive folder to share it.">
                <input value={settings.libraryPath} onChange={(e) => void saveSettings({ ...settings, libraryPath: e.target.value })} />
              </Field>
              <button type="button" className="btn" onClick={() => void pick()}>
                Choose…
              </button>
            </div>
          )}
          <Field label="Your name (recorded on each version)">
            <input value={settings.author} onChange={(e) => void saveSettings({ ...settings, author: e.target.value })} />
          </Field>
        </Panel>
        <Panel title="About">
          <p>
            Songbook {info?.version} — show file library and documentation generator for digital mixing desks. {isLite ? 'Settings live in this browser.' : <>Settings live in <span className="mono">{info?.configDir}</span>.</>}
          </p>
          <p className="muted small">Not affiliated with Allen & Heath or Yamaha. Drivers were built against the vendors' published protocol documents, their offline editors and one DM3 on a bench; see the README for what has and has not been checked on hardware.</p>
        </Panel>
      </div>
    </div>
  );
}

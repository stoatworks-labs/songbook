import { useState } from 'react';

import { pickFolder, pickSave } from '../lib/dialogs';
import { api, type VendorWriteResult } from '../lib/ipc';
import { useStore } from '../store';
import { bytes, fmtDate } from '../lib/format';
import { Panel } from './ui';

/** The vendor kinds Songbook can write a show back into. */
const WRITABLE: Record<string, { what: string; how: string }> = {
  'sq-show': {
    what: 'input patch → NVDATA.DAT and every scene that was following it, scene names → SCENEnnn.DAT',
    how: 'Choose a folder to write the images into (copy it to a USB stick under AHSQ/Shows/<name>), or a .zip path to keep them together.',
  },
  clf: {
    what: 'input patch and channel names → the console file',
    how: 'Choose where to save the .CLF; load it through CL/QL Editor or the console.',
  },
};

export function VendorTab() {
  const show = useStore((s) => s.show)!;
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [last, setLast] = useState<VendorWriteResult | null>(null);

  const exportBlob = async (sha: string, suggested: string) => {
    const path = await pickSave('Export vendor file', suggested, ['*']);
    if (!path) return;
    const r = await run('Exporting', () => api.vendorExport(show.id, sha, path));
    if (r !== undefined) toast(`Wrote ${path}`);
  };

  const writeBack = async (sha: string, kind: string, suggested: string, asZip: boolean) => {
    let path: string | null;
    if (kind === 'sq-show' && !asZip) {
      path = await pickFolder('Folder to write the SQ show images into');
    } else if (kind === 'sq-show') {
      path = await pickSave('Write SQ show as zip', suggested.replace(/\.zip$/i, '') + '.zip', ['zip']);
    } else {
      path = await pickSave('Write console file', suggested.replace(/\.clf$/i, '') + '.CLF', ['CLF', 'clf']);
    }
    if (!path) return;
    const r = await run('Writing', () => api.vendorWrite(show, sha, path));
    if (r) {
      setLast(r);
      toast(r.report.skipped.length ? `Wrote ${r.path} — ${r.report.skipped.length} item(s) not carried` : `Wrote ${r.path}`);
    }
  };

  return (
    <>
      <Panel title="Vendor files kept with this show">
        {show.vendor.length === 0 ? (
          <div className="muted">
            None. A vendor file is the desk's own show file — an SQ show folder (kept as a zip), a dLive / Avantis show archive, a Yamaha .CLF or a .dm3s / .tfs scene — kept byte-for-byte beside the model so the show can go back to its own desk exactly. Importing a file keeps it here.
          </div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Kind</th>
                <th>Captured</th>
                <th>Size</th>
                <th>Note</th>
                <th>sha256</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {show.vendor.map((v) => {
                const suggested = v.note.split(' — ')[0] || `${v.kind}`;
                const w = WRITABLE[v.kind];
                return (
                  <tr key={v.sha256}>
                    <td>{v.kind}</td>
                    <td>{fmtDate(v.capturedAt)}</td>
                    <td>{bytes(v.size)}</td>
                    <td>{v.note}</td>
                    <td className="mono muted">{v.sha256.slice(0, 12)}…</td>
                    <td className="row-actions">
                      <button type="button" className="btn small" onClick={() => void exportBlob(v.sha256, suggested)}>
                        Export as kept…
                      </button>
                      {w && v.kind === 'sq-show' && (
                        <>
                          <button type="button" className="btn small primary" title={w.what} onClick={() => void writeBack(v.sha256, v.kind, suggested, false)}>
                            Write show into folder…
                          </button>
                          <button type="button" className="btn small" title={w.what} onClick={() => void writeBack(v.sha256, v.kind, suggested, true)}>
                            …as zip
                          </button>
                        </>
                      )}
                      {w && v.kind === 'clf' && (
                        <button type="button" className="btn small primary" title={w.what} onClick={() => void writeBack(v.sha256, v.kind, suggested, false)}>
                          Write show into .CLF…
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        <div className="muted small" style={{ marginTop: 12 }}>
          <p>
            <b>Export as kept</b> gives you the file exactly as it was imported. <b>Write show into…</b> makes a copy of it with this show's edits written in and the file's checksum recomputed — the SQ show's input patch and scene names (an SQ scene stores the patch it was saved with, so scenes that were following the desk's patch get the new one and a scene with a patch of its own keeps it), the CL/QL console file's input patch and channel names; everything the reader does not decode (preamps, processing, the mix) stays as it was in the kept file. The kept file itself is never changed.
          </p>
          <p>
            dLive / Avantis show archives and DM3 / TF / DM7 scene files are export-only for now: their scene blobs are only partly decoded, so a write could not promise to leave the rest intact.
          </p>
          <p>Load a written file the way the desk expects: an SQ show folder on a USB stick under AHSQ/Shows, a .CLF through CL/QL Editor or the console's USB load.</p>
        </div>
      </Panel>
      {last && (
        <Panel title={`Written: ${last.path}`}>
          <div className="stats">
            <div className="stat"><div className="stat-value">{last.report.patchesWritten}</div><div className="stat-label">patch entries</div></div>
            {last.report.namesWritten !== undefined && <div className="stat"><div className="stat-value">{last.report.namesWritten}</div><div className="stat-label">channel names</div></div>}
            {last.report.sceneNamesWritten !== undefined && <div className="stat"><div className="stat-value">{last.report.sceneNamesWritten}</div><div className="stat-label">scene names</div></div>}
            {last.report.scenePatchesWritten !== undefined && <div className="stat"><div className="stat-value">{last.report.scenePatchesWritten}</div><div className="stat-label">scenes given the patch</div></div>}
            <div className="stat"><div className="stat-value">{last.report.skipped.length}</div><div className="stat-label">not carried</div></div>
          </div>
          {last.report.files && last.report.files.length > 0 && <div className="muted small mono">{last.report.files.join(' · ')}</div>}
          {last.report.skipped.length > 0 && (
            <ul className="notes">
              {last.report.skipped.map((s) => (
                <li key={s} className="note note--adapted">
                  <span className="note-level">skipped</span>
                  <span className="note-path" />
                  <span>{s}</span>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      )}
    </>
  );
}

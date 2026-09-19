import { useState } from 'react';

import { inputListCsv } from '../lib/csv';
import { pickFile, pickFolder, pickSave } from '../lib/dialogs';
import { api } from '../lib/ipc';
import { DEFAULT_LABELS, labelStrip, safeName, toPngBase64, type LabelOptions } from '../lib/labels';
import { buildPdf, type PdfOptions } from '../lib/pdf';
import { useStore } from '../store';
import { isAh, type CompanionExportOptions, type CompanionImportReport } from '../types';
import { Field, Panel } from './ui';

function b64(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

export function ExportTab() {
  const show = useStore((s) => s.show)!;
  const settings = useStore((s) => s.settings);
  const run = useStore((s) => s.run);
  const toast = useStore((s) => s.toast);
  const [pdfOpts, setPdfOpts] = useState<PdfOptions>({ includeSends: true, includeScenes: true, includeHistory: true, preparedBy: settings?.author ?? '', event: '' });
  const [labels, setLabels] = useState<LabelOptions>(DEFAULT_LABELS);
  const [comp, setComp] = useState<CompanionExportOptions>({ connectionLabel: safeName(show.system.model || 'desk').toLowerCase(), host: settings?.devices.find((d) => d.platform === show.platform)?.host ?? '192.168.1.60', includeChannels: true, includeDcas: true, includeMuteGroups: true, includeScenes: true, pageName: '' });
  const [report, setReport] = useState<CompanionImportReport | null>(null);

  const pdf = async () => {
    const path = await pickSave('Save PDF', `${safeName(show.meta.name)}.pdf`, ['pdf']);
    if (!path) return;
    const r = await run('Building PDF', async () => {
      const commits = await api.showHistory(show.id).catch(() => []);
      const bytes = await buildPdf(show, commits, pdfOpts);
      await api.writeFile(path, b64(bytes));
      return true;
    });
    if (r) toast(`Wrote ${path}`);
  };

  const csv = async () => {
    const path = await pickSave('Save input list', `${safeName(show.meta.name)}-inputs.csv`, ['csv']);
    if (!path) return;
    const r = await run('Writing CSV', () => api.writeText(path, inputListCsv(show)));
    if (r !== undefined) toast(`Wrote ${path}`);
  };

  const strips = async () => {
    const folder = await pickFolder('Folder for the label strip PNGs');
    if (!folder) return;
    const inputs = show.channels.filter((c) => c.kind === 'input');
    const r = await run('Rendering labels', async () => {
      let n = 0;
      for (let from = 0; from < inputs.length; from += labels.perStrip) {
        const c = labelStrip(show, from, labels);
        await api.writeFile(`${folder}/${safeName(`${show.meta.name}-labels-${from + 1}-${Math.min(from + labels.perStrip, inputs.length)}`)}.png`, await toPngBase64(c));
        n += 1;
      }
      return n;
    });
    if (r) toast(`Wrote ${r} label strip${r === 1 ? '' : 's'} to ${folder}`);
  };

  const companionOut = async () => {
    const path = await pickSave('Companion page', `${safeName(show.meta.name)}.companionconfig`, ['companionconfig', 'json']);
    if (!path) return;
    const r = await run('Exporting Companion page', () => api.companionExport(show.id, comp, path));
    if (r) toast(`Wrote ${r.path} (${r.pages} page${r.pages === 1 ? '' : 's'}, ${r.type})`);
  };
  const companionIn = async () => {
    const path = await pickFile('Companion config', ['companionconfig', 'json']);
    if (!path) return;
    const r = await run('Reading Companion page', () => api.companionImport(show.id, path));
    if (r) setReport(r);
  };

  const exportJson = async () => {
    const path = await pickSave('Songbook show', `${safeName(show.meta.name)}.songbook.json`, ['json']);
    if (!path) return;
    const r = await run('Exporting', () => api.exportShowJson(show.id, path));
    if (r !== undefined) toast(`Wrote ${path}`);
  };

  return (
    <div className="grid-2">
      <Panel title="PDF documentation">
        <p className="muted small">Cover, desk and I/O, the input list with patch and preamps, the send matrix, buses and outputs, DCAs and mute groups, scenes and cues, the import notes, and the version history.</p>
        <div className="row wrap">
          <Field label="Event / job">
            <input value={pdfOpts.event} onChange={(e) => setPdfOpts({ ...pdfOpts, event: e.target.value })} />
          </Field>
          <Field label="Prepared for">
            <input value={pdfOpts.preparedBy} onChange={(e) => setPdfOpts({ ...pdfOpts, preparedBy: e.target.value })} />
          </Field>
        </div>
        <div className="row wrap">
          <label className="check"><input type="checkbox" checked={pdfOpts.includeSends} onChange={(e) => setPdfOpts({ ...pdfOpts, includeSends: e.target.checked })} /> send matrix</label>
          <label className="check"><input type="checkbox" checked={pdfOpts.includeScenes} onChange={(e) => setPdfOpts({ ...pdfOpts, includeScenes: e.target.checked })} /> scenes and cues</label>
          <label className="check"><input type="checkbox" checked={pdfOpts.includeHistory} onChange={(e) => setPdfOpts({ ...pdfOpts, includeHistory: e.target.checked })} /> version history</label>
        </div>
        <div className="row">
          <button type="button" className="btn primary" onClick={() => void pdf()}>
            Save PDF…
          </button>
          <button type="button" className="btn" onClick={() => void csv()}>
            Input list CSV…
          </button>
        </div>
      </Panel>
      <Panel title="Label strips">
        <p className="muted small">One PNG per bank of faders at 300 dpi, each channel in its desk colour. The pitch is yours to set — measure centre to centre across ten faders and divide by ten; the default is not a measured figure for any desk.</p>
        <div className="row wrap">
          <Field label="Fader pitch (mm)">
            <input className="tiny-input" value={labels.pitchMm} onChange={(e) => setLabels({ ...labels, pitchMm: Number(e.target.value) || DEFAULT_LABELS.pitchMm })} />
          </Field>
          <Field label="Height (mm)">
            <input className="tiny-input" value={labels.heightMm} onChange={(e) => setLabels({ ...labels, heightMm: Number(e.target.value) || DEFAULT_LABELS.heightMm })} />
          </Field>
          <Field label="Faders per strip">
            <input className="tiny-input" value={labels.perStrip} onChange={(e) => setLabels({ ...labels, perStrip: Math.max(1, Number(e.target.value) || DEFAULT_LABELS.perStrip) })} />
          </Field>
          <label className="check"><input type="checkbox" checked={labels.numbers} onChange={(e) => setLabels({ ...labels, numbers: e.target.checked })} /> channel numbers</label>
        </div>
        <button type="button" className="btn primary" onClick={() => void strips()}>
          Save label strips…
        </button>
      </Panel>
      <Panel title="Bitfocus Companion">
        <p className="muted small">
          A page of mute buttons per channel, DCA and mute group, and a recall button per scene, for the {isAh(show.platform) ? 'allenheath-*' : 'yamaha-rcp'} module. Reading a page back checks which buttons still match this show.
        </p>
        <div className="row wrap">
          <Field label="Connection label">
            <input value={comp.connectionLabel} onChange={(e) => setComp({ ...comp, connectionLabel: e.target.value })} />
          </Field>
          <Field label="Desk address">
            <input value={comp.host} onChange={(e) => setComp({ ...comp, host: e.target.value })} />
          </Field>
          <Field label="Page name">
            <input value={comp.pageName} placeholder={show.meta.name} onChange={(e) => setComp({ ...comp, pageName: e.target.value })} />
          </Field>
        </div>
        <div className="row wrap">
          <label className="check"><input type="checkbox" checked={comp.includeChannels} onChange={(e) => setComp({ ...comp, includeChannels: e.target.checked })} /> channel mutes</label>
          <label className="check"><input type="checkbox" checked={comp.includeDcas} onChange={(e) => setComp({ ...comp, includeDcas: e.target.checked })} /> DCA mutes</label>
          <label className="check"><input type="checkbox" checked={comp.includeMuteGroups} onChange={(e) => setComp({ ...comp, includeMuteGroups: e.target.checked })} /> mute groups</label>
          <label className="check"><input type="checkbox" checked={comp.includeScenes} onChange={(e) => setComp({ ...comp, includeScenes: e.target.checked })} /> scene recalls</label>
        </div>
        <div className="row">
          <button type="button" className="btn primary" onClick={() => void companionOut()}>
            Export page…
          </button>
          <button type="button" className="btn" onClick={() => void companionIn()}>
            Check a page against this show…
          </button>
        </div>
        {report ? (
          <div className="report">
            <div className="muted small">
              {report.kind} export v{report.fileVersion} · pages: {report.pages.join(', ') || '—'} · connections: {report.connections.join(', ') || '—'} · {report.buttons.length} buttons, {report.unmatched} unmatched
            </div>
            <table className="table">
              <thead>
                <tr>
                  <th>Page</th>
                  <th>Button</th>
                  <th>Action</th>
                  <th>Matches</th>
                </tr>
              </thead>
              <tbody>
                {report.buttons.map((b, i) => (
                  <tr key={i} className={b.problem ? 'row--problem' : ''}>
                    <td>{b.page}</td>
                    <td>{b.text.replace('\n', ' ')}</td>
                    <td className="mono small">{b.definition}</td>
                    <td>{b.problem ?? b.matched ?? ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : null}
      </Panel>
      <Panel title="Songbook file">
        <p className="muted small">The model as JSON, importable into another library. Vendor files travel separately (Vendor files tab).</p>
        <button type="button" className="btn" onClick={() => void exportJson()}>
          Export .songbook.json…
        </button>
      </Panel>
    </div>
  );
}

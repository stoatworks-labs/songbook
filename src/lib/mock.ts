/**
 * A browser-only stand-in for the Tauri commands, so the UI can be looked at
 * (and the PDF / label generators exercised) in a plain tab. It serves two
 * demo shows and keeps edits in memory; everything that needs the disk or a
 * desk says so.
 */
import type { AppInfo, Commit, Entry, Settings, Show } from '../types';
import type { api as realApi } from './ipc';

type Api = typeof realApi;

const shows = new Map<string, Show>();
const history = new Map<string, Commit[]>();
let loaded: Promise<void> | null = null;

const info: AppInfo = {
  version: 'demo',
  configDir: '(browser demo — nothing is written)',
  platforms: [
    { id: 'ah-sq', label: 'Allen & Heath SQ', vendor: 'Allen & Heath', models: ['SQ-5', 'SQ-6', 'SQ-7'] },
    { id: 'ah-dlive', label: 'Allen & Heath dLive', vendor: 'Allen & Heath', models: ['dLive S3000', 'dLive S5000', 'dLive S7000', 'dLive C1500', 'dLive C2500', 'dLive C3500'] },
    { id: 'ah-avantis', label: 'Allen & Heath Avantis', vendor: 'Allen & Heath', models: ['Avantis', 'Avantis Solo'] },
    { id: 'ah-qu', label: 'Allen & Heath Qu', vendor: 'Allen & Heath', models: ['Qu-5', 'Qu-6', 'Qu-7'] },
    { id: 'ah-cq', label: 'Allen & Heath CQ', vendor: 'Allen & Heath', models: ['CQ-12T', 'CQ-18T', 'CQ-20B'] },
    { id: 'yamaha-cl-ql', label: 'Yamaha CL / QL', vendor: 'Yamaha', models: ['QL1', 'QL5', 'CL1', 'CL3', 'CL5'] },
    { id: 'yamaha-tf', label: 'Yamaha TF', vendor: 'Yamaha', models: ['TF1', 'TF3', 'TF5', 'TF-Rack'] },
    { id: 'yamaha-dm3', label: 'Yamaha DM3', vendor: 'Yamaha', models: ['DM3', 'DM3S'] },
    { id: 'yamaha-dm7', label: 'Yamaha DM7', vendor: 'Yamaha', models: ['DM7', 'DM7 Compact'] },
    { id: 'yamaha-rivage', label: 'Yamaha RIVAGE PM', vendor: 'Yamaha', models: ['PM3', 'PM5', 'PM7', 'PM10'] },
    { id: 'generic', label: 'Generic', vendor: '', models: ['Generic'] },
  ],
};

let settings: Settings = {
  libraryPath: '(demo library in memory)',
  author: 'demo',
  devices: [
    { name: 'SQ-5 (demo)', platform: 'ah-sq', host: '192.168.1.60', midiChannel: 1, model: 'SQ-5' },
    { name: 'DM3 (demo)', platform: 'yamaha-dm3', host: 'dm3.local', model: 'DM3' },
  ],
  sync: { provider: 'none', root: '', clientId: '', lastRun: '' },
  companionHost: '',
};

function summary(s: Show) {
  return {
    id: s.id,
    name: s.meta.name,
    platform: s.platform,
    model: s.system.model,
    firmware: s.system.firmware,
    modified: s.meta.modified,
    inputs: s.sockets.filter((x) => x.direction === 'in').length,
    outputs: s.sockets.filter((x) => x.direction === 'out').length,
    channels: s.channels.length,
    namedChannels: s.channels.filter((c) => c.label.trim()).length,
    auxes: s.buses.filter((b) => b.kind === 'aux').length,
    groups: s.buses.filter((b) => b.kind === 'group').length,
    matrices: s.buses.filter((b) => b.kind === 'matrix').length,
    fx: s.buses.filter((b) => b.kind === 'fx-send').length,
    dcas: s.dcas.length,
    muteGroups: s.muteGroups.length,
    scenes: s.scenes.length,
    cues: s.cues.length,
    notesDropped: s.notes.filter((n) => n.level === 'dropped').length,
  };
}

async function ensure() {
  if (!loaded) {
    loaded = (async () => {
      for (const f of ['sq5-band', 'dm3-corporate']) {
        const s = (await (await fetch(`./demo/${f}.json`)).json()) as Show;
        shows.set(s.id, s);
        history.set(s.id, [{ id: 'demo0', at: s.meta.modified, message: 'Demo show', hash: 'demo', changes: 0, summary: summary(s) }]);
      }
    })();
  }
  await loaded;
}

const notDesktop = (what: string) => Promise.reject(new Error(`${what} needs the desktop app (this is the browser demo)`));

function download(name: string, bytes: Uint8Array | string, type: string) {
  const blob = new Blob([bytes as BlobPart], { type });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

export const mockApi: Api = {
  appInfo: async () => info,
  settingsGet: async () => settings,
  settingsSet: async (s) => (settings = s),
  libraryList: async () => {
    await ensure();
    return [...shows.values()].map<Entry>((s) => ({ summary: summary(s), dir: '(memory)', commits: history.get(s.id)?.length ?? 0, vendorFiles: s.vendor.length }));
  },
  showLoad: async (id) => {
    await ensure();
    const s = shows.get(id);
    if (!s) throw new Error(`no show ${id}`);
    return structuredClone(s);
  },
  showSave: async (show, message) => {
    show.meta.modified = new Date().toISOString();
    shows.set(show.id, structuredClone(show));
    const h = history.get(show.id) ?? [];
    const commit: Commit = { id: `demo${h.length}`, parent: h[h.length - 1]?.id, at: show.meta.modified, message, hash: String(h.length), changes: 1, summary: summary(show) };
    h.push(commit);
    history.set(show.id, h);
    return { show, commit };
  },
  showNew: async (name, platform, model) => {
    const now = new Date().toISOString();
    const s: Show = { schema: 'songbook/1', id: `demo-${Date.now()}`, meta: { name, notes: '', tags: [], created: now, modified: now }, platform, system: { model, firmware: '', name: '', units: [] }, sockets: [], preamps: [], channels: [], buses: [], dcas: [], muteGroups: [], outputPatch: [], scenes: [], cues: [], vendor: [], notes: [] };
    shows.set(s.id, s);
    history.set(s.id, [{ id: 'demo0', at: now, message: 'Created', hash: '0', changes: 0, summary: summary(s) }]);
    return s;
  },
  showHistory: async (id) => history.get(id) ?? [],
  showSnapshot: async (id) => shows.get(id)!,
  showDiff: async () => [],
  showRestore: () => notDesktop('Restore'),
  showDelete: async (id) => void shows.delete(id),
  showDuplicate: async (id, name) => {
    const s = structuredClone(shows.get(id)!);
    s.id = `demo-${Date.now()}`;
    s.meta.name = name;
    shows.set(s.id, s);
    history.set(s.id, []);
    return s;
  },
  showValidate: async () => [],
  importPath: () => notDesktop('Import'),
  exportShowJson: async (id) => download(`${shows.get(id)?.meta.name ?? 'show'}.songbook.json`, JSON.stringify(shows.get(id), null, 2), 'application/json'),
  vendorExport: () => notDesktop('Vendor export'),
  vendorWrite: () => notDesktop('Writing a vendor file'),
  writeFile: async (path, base64) => download(path.split('/').pop() ?? 'file', Uint8Array.from(atob(base64), (c) => c.charCodeAt(0)), 'application/octet-stream'),
  writeText: async (path, text) => download(path.split('/').pop() ?? 'file', text, 'text/plain'),
  readText: () => notDesktop('Reading a file'),
  deviceProbe: () => notDesktop('Probing a desk'),
  devicePull: () => notDesktop('Pulling from a desk'),
  devicePush: () => notDesktop('Pushing to a desk'),
  deviceScene: () => notDesktop('Recalling on a desk'),
  convertShow: () => notDesktop('Conversion'),
  capabilities: () => notDesktop('Capabilities'),
  companionExport: () => notDesktop('Companion export'),
  companionImport: () => notDesktop('Companion import'),
  syncRun: () => notDesktop('Sync'),
  oauthBegin: () => notDesktop('Sign-in'),
  oauthFinish: () => notDesktop('Sign-in'),
  oauthRefresh: () => notDesktop('Sign-in'),
};

// For looking at the model from the browser console in the demo.
(window as unknown as { __songbookDemo: unknown }).__songbookDemo = { shows };

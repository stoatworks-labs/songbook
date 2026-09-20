/**
 * Songbook Lite: the same `api` surface as the desktop app, backed by the
 * Rust core in WebAssembly and a library kept in this browser's IndexedDB.
 *
 * Files never leave the tab. What needs a disk becomes a download; what needs
 * a desk or a cloud account says so and points at the desktop app.
 */
import type { AppInfo, Change, Commit, CompanionExportOptions, Entry, Settings, Show, Summary, VendorBlob } from '../types';
import { LITE_FULL_APP_URL } from './lite-links';
import type { api as tauriApi, ConvertResult, ImportResult, VendorWriteReport, VendorWriteResult } from './ipc';
import { takeFiles } from './browser-files';
import { call } from './wasm';

type Api = typeof tauriApi;

// ---------------------------------------------------------------- storage

const DB = 'songbook-lite';
let dbPromise: Promise<IDBDatabase> | null = null;

function db(): Promise<IDBDatabase> {
  if (!dbPromise) {
    dbPromise = new Promise((resolve, reject) => {
      const req = indexedDB.open(DB, 1);
      req.onupgradeneeded = () => {
        const d = req.result;
        d.createObjectStore('shows', { keyPath: 'id' });
        d.createObjectStore('versions', { keyPath: 'key' });
        d.createObjectStore('vendor', { keyPath: 'key' });
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error ?? new Error('IndexedDB unavailable'));
    });
  }
  return dbPromise;
}

function tx<T>(store: string, mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return db().then(
    (d) =>
      new Promise<T>((resolve, reject) => {
        const t = d.transaction(store, mode);
        const r = fn(t.objectStore(store));
        r.onsuccess = () => resolve(r.result);
        r.onerror = () => reject(r.error);
      }),
  );
}

interface StoredShow {
  id: string;
  show: Show;
  summary: Summary;
  commits: number;
}
interface StoredVersion {
  key: string;
  id: string;
  commit: Commit;
  show: Show;
}
interface StoredVendor {
  key: string;
  bytes: Uint8Array;
}

const getShow = (id: string) => tx<StoredShow | undefined>('shows', 'readonly', (s) => s.get(id));
const putShow = (r: StoredShow) => tx('shows', 'readwrite', (s) => s.put(r));
const allShows = () => tx<StoredShow[]>('shows', 'readonly', (s) => s.getAll());
const versionsOf = async (id: string) => (await tx<StoredVersion[]>('versions', 'readonly', (s) => s.getAll())).filter((v) => v.id === id).sort((a, b) => a.commit.at.localeCompare(b.commit.at) || a.key.localeCompare(b.key));
const putVersion = (v: StoredVersion) => tx('versions', 'readwrite', (s) => s.put(v));
const vendorBytes = async (id: string, sha: string) => (await tx<StoredVendor | undefined>('vendor', 'readonly', (s) => s.get(`${id}/${sha}`)))?.bytes;
const putVendor = (id: string, sha: string, bytes: Uint8Array) => tx('vendor', 'readwrite', (s) => s.put({ key: `${id}/${sha}`, bytes }));

async function sha256(bytes: Uint8Array | string): Promise<string> {
  const data = typeof bytes === 'string' ? new TextEncoder().encode(bytes) : bytes;
  const h = await crypto.subtle.digest('SHA-256', data as BufferSource);
  return Array.from(new Uint8Array(h), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** sha256 of the show with `meta.modified` blanked, like the desktop library. */
async function contentHash(show: Show): Promise<string> {
  const c = structuredClone(show);
  c.meta.modified = '';
  return sha256(JSON.stringify(c));
}

async function summaryOf(show: Show): Promise<Summary> {
  return (await call<Summary>('summary', { show })).ok;
}

const notHere = (what: string): never => {
  throw new Error(`${what} is not in Songbook Lite — it needs the desktop app (${LITE_FULL_APP_URL}).`);
};

function download(name: string, data: Uint8Array | string, type: string) {
  const blob = new Blob([data as BlobPart], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

const baseName = (path: string) => path.split('/').pop() || 'file';

// ---------------------------------------------------------------- settings

const SETTINGS_KEY = 'songbook-lite.settings';
function loadSettings(): Settings {
  let saved: Partial<Settings> = {};
  try {
    saved = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? '{}') as Partial<Settings>;
  } catch {
    saved = {};
  }
  return {
    libraryPath: 'this browser (IndexedDB) — nothing leaves the tab',
    author: saved.author ?? '',
    devices: [],
    sync: { provider: 'none', root: '', clientId: '', lastRun: '' },
    companionHost: saved.companionHost ?? '',
  };
}

// ---------------------------------------------------------------- library ops

async function save(show: Show, message: string): Promise<{ show: Show; commit: Commit | null }> {
  const existing = await getShow(show.id);
  const versions = await versionsOf(show.id);
  const last = versions[versions.length - 1];
  const hash = await contentHash(show);
  const settings = loadSettings();
  let commit: Commit | null = null;
  if (!last || last.commit.hash !== hash) {
    show.meta.modified = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    const changes = last ? (await call<Change[]>('diff', { before: last.show, after: show })).ok.length : 0;
    commit = {
      id: hash.slice(0, 12),
      parent: last?.commit.id,
      at: show.meta.modified,
      message,
      author: settings.author || undefined,
      hash,
      summary: await summaryOf(show),
      changes,
    };
    await putVersion({ key: `${show.id}/${commit.at}/${commit.id}`, id: show.id, commit, show: structuredClone(show) });
  }
  const summary = await summaryOf(show);
  await putShow({ id: show.id, show: structuredClone(show), summary, commits: versions.length + (commit ? 1 : 0) || existing?.commits || 0 });
  return { show, commit };
}

async function importFiles(files: File[], platform?: string): Promise<ImportResult> {
  if (files.length === 0) throw new Error('no file chosen');
  const blobs = await Promise.all(files.map(async (f) => new Uint8Array(await f.arrayBuffer())));
  // An SQ show is named after the folder it came from; a lone image gets a plain name.
  const folderName = files[0].webkitRelativePath ? files[0].webkitRelativePath.split('/')[0] : undefined;
  const dats = files.every((f) => /\.dat$/i.test(f.name));
  const name = folderName ?? (dats ? 'SQ show' : files[0].name.replace(/\.[^.]+$/, ''));
  const r = await call<{ show: Show; kind: string; vendorName: string | null }>('import', { files: files.map((f) => ({ name: f.name })), platform: platform ?? null, name }, blobs);
  const show = r.ok.show;
  if (r.ok.vendorName && r.blobs[0]) {
    const bytes = r.blobs[0];
    const sha = await sha256(bytes);
    await putVendor(show.id, sha, bytes);
    const blob: VendorBlob = { platform: show.platform, kind: r.ok.kind, sha256: sha, file: `vendor/${sha}`, size: bytes.length, capturedAt: new Date().toISOString(), note: `${r.ok.vendorName} — imported file` };
    show.vendor = show.vendor.filter((v) => v.sha256 !== sha).concat(blob);
  }
  const saved = await save(show, `Imported ${r.ok.vendorName ?? files[0].name}`);
  return { show: saved.show, summary: await summaryOf(saved.show), kind: r.ok.kind };
}

export const liteApi: Api = {
  appInfo: async () => {
    const r = await call<{ version: string; platforms: AppInfo['platforms'] }>('info');
    return { version: `${r.ok.version} (lite)`, configDir: 'this browser', platforms: r.ok.platforms };
  },
  settingsGet: async () => loadSettings(),
  settingsSet: async (settings) => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ author: settings.author, companionHost: settings.companionHost }));
    return loadSettings();
  },

  libraryList: async () => {
    const rows = await allShows();
    const entries: Entry[] = rows.map((r) => ({ summary: r.summary, dir: 'browser', commits: r.commits, vendorFiles: r.show.vendor.length }));
    return entries.sort((a, b) => b.summary.modified.localeCompare(a.summary.modified));
  },
  showLoad: async (id) => {
    const r = await getShow(id);
    if (!r) throw new Error(`no show ${id} in this browser's library`);
    return structuredClone(r.show);
  },
  showSave: (show, message) => save(show, message),
  showNew: async (name, platform, model) => {
    const r = await call<Show>('new', { name, platform, model });
    return (await save(r.ok, 'Created')).show;
  },
  showHistory: async (id) => (await versionsOf(id)).map((v) => v.commit).reverse(),
  showSnapshot: async (id, commit) => {
    const v = (await versionsOf(id)).find((x) => x.commit.id === commit);
    if (!v) throw new Error(`no version ${commit}`);
    return structuredClone(v.show);
  },
  showDiff: async (id, from, to) => {
    const vs = await versionsOf(id);
    const a = vs.find((x) => x.commit.id === from);
    const b = vs.find((x) => x.commit.id === to);
    if (!a || !b) throw new Error('no such version');
    return (await call<Change[]>('diff', { before: a.show, after: b.show })).ok;
  },
  showRestore: async (id, commit) => {
    const v = (await versionsOf(id)).find((x) => x.commit.id === commit);
    if (!v) throw new Error(`no version ${commit}`);
    const restored = structuredClone(v.show);
    const r = await save(restored, `Restored ${commit}`);
    if (!r.commit) throw new Error('already at that version');
    return r.commit;
  },
  showDelete: async (id) => {
    await tx('shows', 'readwrite', (s) => s.delete(id));
    for (const v of await versionsOf(id)) await tx('versions', 'readwrite', (s) => s.delete(v.key));
  },
  showDuplicate: async (id, name) => {
    const src = await getShow(id);
    if (!src) throw new Error(`no show ${id}`);
    const copy = structuredClone(src.show);
    copy.id = crypto.randomUUID();
    copy.meta.name = name;
    copy.meta.created = new Date().toISOString();
    for (const v of copy.vendor) {
      const bytes = await vendorBytes(id, v.sha256);
      if (bytes) await putVendor(copy.id, v.sha256, bytes);
    }
    return (await save(copy, `Duplicated from ${src.show.meta.name}`)).show;
  },
  showValidate: async (show) => (await call<string[]>('validate', { show })).ok,

  importPath: async (path, platform) => importFiles(takeFiles(path), platform === 'ah-sq' || platform == null ? platform ?? undefined : platform),
  exportShowJson: async (id, path) => {
    const r = await getShow(id);
    if (!r) throw new Error(`no show ${id}`);
    download(baseName(path), JSON.stringify(r.show, null, 2), 'application/json');
  },
  vendorExport: async (id, sha, path) => {
    const bytes = await vendorBytes(id, sha);
    if (!bytes) throw new Error('that vendor file is not in this browser');
    download(baseName(path), bytes, 'application/octet-stream');
  },
  vendorWrite: async (show, sha, path): Promise<VendorWriteResult> => {
    const blob = show.vendor.find((v) => v.sha256 === sha);
    const bytes = blob && (await vendorBytes(show.id, sha));
    if (!blob || !bytes) throw new Error('that vendor file is not in this browser');
    const r = await call<{ report: VendorWriteReport; fileNames: string[] }>('vendor_write', { show, kind: blob.kind, asZip: blob.kind === 'sq-show' }, [bytes]);
    const name = blob.kind === 'sq-show' ? baseName(path).replace(/\.zip$/i, '') + '.zip' : baseName(path);
    download(name, r.blobs[0], blob.kind === 'sq-show' ? 'application/zip' : 'application/octet-stream');
    return { path: name, kind: blob.kind, report: r.ok.report };
  },
  writeFile: async (path, base64) => download(baseName(path), Uint8Array.from(atob(base64), (c) => c.charCodeAt(0)), 'application/octet-stream'),
  writeText: async (path, text) => download(baseName(path), text, 'text/plain'),
  readText: async (path) => {
    const [f] = takeFiles(path);
    if (!f) throw new Error('no file chosen');
    return f.text();
  },

  deviceProbe: () => notHere('Talking to a desk'),
  devicePull: () => notHere('Pulling from a desk'),
  devicePush: () => notHere('Pushing to a desk'),
  deviceScene: () => notHere('Recalling on a desk'),

  convertShow: async (id, target, model, saveIt): Promise<ConvertResult> => {
    const src = await getShow(id);
    if (!src) throw new Error(`no show ${id}`);
    const r = await call<{ show: Show; notes: ConvertResult['notes']; carried: number; adapted: number; dropped: number }>('convert', { show: src.show, target, model });
    let show = r.ok.show;
    if (saveIt) show = (await save(show, `Converted from ${src.show.meta.name}`)).show;
    return { ...r.ok, show, saved: saveIt };
  },
  capabilities: async (platform, model) => (await call<Awaited<ReturnType<Api['capabilities']>>>('capabilities', { platform, model })).ok,

  companionExport: async (id, opts: CompanionExportOptions, path) => {
    const src = await getShow(id);
    if (!src) throw new Error(`no show ${id}`);
    const r = await call<Record<string, unknown>>('companion_export', { show: src.show, opts });
    download(baseName(path), JSON.stringify(r.ok, null, 2), 'application/json');
    const pages = r.ok.pages && typeof r.ok.pages === 'object' ? Object.keys(r.ok.pages as object).length : 1;
    return { path: baseName(path), type: String(r.ok.type ?? ''), pages };
  },
  companionImport: async (id, path) => {
    const src = await getShow(id);
    if (!src) throw new Error(`no show ${id}`);
    const [f] = takeFiles(path);
    if (!f) throw new Error('no file chosen');
    return (await call<Awaited<ReturnType<Api['companionImport']>>>('companion_import', { show: src.show }, [new Uint8Array(await f.arrayBuffer())])).ok;
  },

  syncRun: () => notHere('Cloud sync'),
  oauthBegin: () => notHere('Signing in to a drive'),
  oauthFinish: () => notHere('Signing in to a drive'),
  oauthRefresh: () => notHere('Signing in to a drive'),
};

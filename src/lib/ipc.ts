/**
 * Typed wrappers over the Tauri commands. Nothing else calls `invoke`
 * directly, so a rename on the Rust side breaks exactly one place.
 */
import { invoke } from '@tauri-apps/api/core';

import type {
  AppInfo,
  Capabilities,
  Change,
  Commit,
  CompanionExportOptions,
  CompanionImportReport,
  DeviceEntry,
  Entry,
  Note,
  Platform,
  Settings,
  Show,
  Summary,
  SyncReport,
  Tokens,
} from '../types';

export interface ImportResult {
  show: Show;
  summary: Summary;
  kind: string;
}

export interface DeviceRef {
  platform: Platform;
  host: string;
  midiChannel?: number;
  model?: string;
}

export interface PushOptions {
  names: boolean;
  mutes: boolean;
  levels: boolean;
  sends: boolean;
  preamps: boolean;
}

export interface VendorWriteReport {
  files?: string[];
  patchesWritten: number;
  namesWritten?: number;
  sceneNamesWritten?: number;
  scenePatchesWritten?: number;
  skipped: string[];
}

export interface VendorWriteResult {
  path: string;
  kind: string;
  report: VendorWriteReport;
}

export interface ConvertResult {
  show: Show;
  notes: Note[];
  carried: number;
  adapted: number;
  dropped: number;
  saved: boolean;
}

export const toRef = (d: DeviceEntry): DeviceRef => ({ platform: d.platform, host: d.host, midiChannel: d.midiChannel, model: d.model });

const tauriApi = {
  appInfo: () => invoke<AppInfo>('app_info'),
  settingsGet: () => invoke<Settings>('settings_get'),
  settingsSet: (settings: Settings) => invoke<Settings>('settings_set', { settings }),

  libraryList: () => invoke<Entry[]>('library_list'),
  showLoad: (id: string) => invoke<Show>('show_load', { id }),
  showSave: (show: Show, message: string) => invoke<{ show: Show; commit: Commit | null }>('show_save', { show, message }),
  showNew: (name: string, platform: Platform, model: string) => invoke<Show>('show_new', { name, platform, model }),
  showHistory: (id: string) => invoke<Commit[]>('show_history', { id }),
  showSnapshot: (id: string, commit: string) => invoke<Show>('show_snapshot', { id, commit }),
  showDiff: (id: string, from: string, to: string) => invoke<Change[]>('show_diff', { id, from, to }),
  showRestore: (id: string, commit: string) => invoke<Commit>('show_restore', { id, commit }),
  showDelete: (id: string) => invoke<void>('show_delete', { id }),
  showDuplicate: (id: string, name: string) => invoke<Show>('show_duplicate', { id, name }),
  showValidate: (show: Show) => invoke<string[]>('show_validate', { show }),

  importPath: (path: string, platform?: Platform) => invoke<ImportResult>('import_path', { path, platform: platform ?? null }),
  exportShowJson: (id: string, path: string) => invoke<void>('export_show_json', { id, path }),
  vendorExport: (id: string, sha256: string, path: string) => invoke<void>('vendor_export', { id, sha256, path }),
  /** Write the show on screen into a copy of a kept vendor file (SQ show, CL/QL .CLF). */
  vendorWrite: (show: Show, sha256: string, path: string) => invoke<VendorWriteResult>('vendor_write', { show, sha256, path }),
  writeFile: (path: string, base64: string) => invoke<void>('write_file', { path, base64 }),
  writeText: (path: string, text: string) => invoke<void>('write_text', { path, text }),
  readText: (path: string) => invoke<string>('read_text', { path }),

  deviceProbe: (dev: DeviceRef) => invoke<Record<string, unknown>>('device_probe', { dev }),
  devicePull: (dev: DeviceRef, opts: { intoShow?: string; name?: string }) => invoke<ImportResult>('device_pull', { dev, opts }),
  devicePush: (dev: DeviceRef, id: string, opts: PushOptions) => invoke<{ ok: boolean; log: string[] }>('device_push', { dev, id, opts }),
  deviceScene: (dev: DeviceRef, req: { action: 'recall' | 'store'; number: number; list?: string }) => invoke<Record<string, unknown>>('device_scene', { dev, req }),

  convertShow: (id: string, target: Platform, model: string, save: boolean) => invoke<ConvertResult>('convert_show', { id, target, model, save }),
  capabilities: (platform: Platform, model: string) => invoke<Capabilities>('capabilities', { platform, model }),

  companionExport: (id: string, opts: CompanionExportOptions, path: string) => invoke<{ path: string; type: string; pages: number }>('companion_export', { id, opts, path }),
  companionImport: (id: string, path: string) => invoke<CompanionImportReport>('companion_import', { id, path }),

  syncRun: (direction: 'both' | 'push' | 'pull') => invoke<SyncReport>('sync_run', { direction }),
  oauthBegin: (provider: string, clientId: string, clientSecret?: string) => invoke<string>('oauth_begin', { provider, clientId, clientSecret: clientSecret || null }),
  oauthFinish: () => invoke<Tokens>('oauth_finish'),
  oauthRefresh: () => invoke<Tokens>('oauth_refresh'),
};

/** True inside the Tauri webview; false in a plain browser tab. */
export const inTauri = '__TAURI_INTERNALS__' in window;
/** True in the hosted Songbook Lite build: Rust in WebAssembly, library in IndexedDB. */
export const isLite: boolean = typeof __SONGBOOK_LITE__ !== 'undefined' && __SONGBOOK_LITE__ && !inTauri;

/** The real commands inside the app; the wasm core in Songbook Lite; the in-memory demo in a plain tab. */
export const api: typeof tauriApi = inTauri ? tauriApi : isLite ? (await import('./lite')).liteApi : (await import('./mock')).mockApi;

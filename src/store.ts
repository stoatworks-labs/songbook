import { create } from 'zustand';

import { api } from './lib/ipc';
import type { AppInfo, Entry, Settings, Show } from './types';

export type View = 'library' | 'show' | 'devices' | 'convert' | 'sync' | 'settings';
export type ShowTab = 'overview' | 'io' | 'channels' | 'buses' | 'scenes' | 'cues' | 'export' | 'vendor' | 'history';

export interface Toast {
  id: number;
  kind: 'info' | 'error';
  text: string;
}

interface State {
  info: AppInfo | null;
  settings: Settings | null;
  view: View;
  entries: Entry[];
  show: Show | null;
  dirty: boolean;
  tab: ShowTab;
  selectedChannel: string | null;
  selectedScene: string | null;
  busy: string | null;
  toasts: Toast[];

  load: () => Promise<void>;
  refreshLibrary: () => Promise<void>;
  setView: (v: View) => void;
  setTab: (t: ShowTab) => void;
  openShow: (id: string) => Promise<void>;
  setShow: (show: Show, dirty?: boolean) => void;
  update: (fn: (show: Show) => void) => void;
  save: (message: string) => Promise<void>;
  closeShow: () => void;
  selectChannel: (id: string | null) => void;
  selectScene: (id: string | null) => void;
  saveSettings: (s: Settings) => Promise<void>;
  toast: (text: string, kind?: 'info' | 'error') => void;
  dismiss: (id: number) => void;
  run: <T>(label: string, fn: () => Promise<T>) => Promise<T | undefined>;
}

let toastSeq = 1;

export const useStore = create<State>((set, get) => ({
  info: null,
  settings: null,
  view: 'library',
  entries: [],
  show: null,
  dirty: false,
  tab: 'overview',
  selectedChannel: null,
  selectedScene: null,
  busy: null,
  toasts: [],

  load: async () => {
    try {
      const [info, settings, entries] = await Promise.all([api.appInfo(), api.settingsGet(), api.libraryList()]);
      set({ info, settings, entries });
      // A deep link: #show=<id>&tab=<tab> opens straight onto a show (used by
      // the screenshot script, handy in the browser demo).
      const hash = new URLSearchParams(window.location.hash.replace(/^#/, ''));
      const id = hash.get('show');
      if (id && entries.some((e) => e.summary.id === id)) {
        await get().openShow(id);
        const tab = hash.get('tab') as ShowTab | null;
        if (tab) set({ tab });
      }
    } catch (e) {
      get().toast(String(e), 'error');
    }
  },

  refreshLibrary: async () => {
    try {
      set({ entries: await api.libraryList() });
    } catch (e) {
      get().toast(String(e), 'error');
    }
  },

  setView: (view) => set({ view }),
  setTab: (tab) => set({ tab }),

  openShow: async (id) => {
    try {
      const show = await api.showLoad(id);
      set({ show, dirty: false, view: 'show', tab: 'overview', selectedChannel: show.channels[0]?.id ?? null, selectedScene: show.scenes[0]?.id ?? null });
    } catch (e) {
      get().toast(String(e), 'error');
    }
  },

  setShow: (show, dirty = false) => set({ show, dirty, selectedChannel: get().selectedChannel ?? show.channels[0]?.id ?? null }),

  update: (fn) => {
    const cur = get().show;
    if (!cur) return;
    const next = structuredClone(cur);
    fn(next);
    set({ show: next, dirty: true });
  },

  save: async (message) => {
    const show = get().show;
    if (!show) return;
    try {
      const r = await api.showSave(show, message);
      set({ show: r.show, dirty: false });
      get().toast(r.commit ? `Saved: ${r.commit.message} (${r.commit.changes} change${r.commit.changes === 1 ? '' : 's'})` : 'Nothing changed since the last save');
      await get().refreshLibrary();
    } catch (e) {
      get().toast(String(e), 'error');
    }
  },

  closeShow: () => set({ show: null, dirty: false, view: 'library' }),
  selectChannel: (id) => set({ selectedChannel: id }),
  selectScene: (id) => set({ selectedScene: id }),

  saveSettings: async (s) => {
    try {
      const saved = await api.settingsSet(s);
      set({ settings: saved });
      await get().refreshLibrary();
    } catch (e) {
      get().toast(String(e), 'error');
    }
  },

  toast: (text, kind = 'info') => {
    const id = toastSeq++;
    set({ toasts: [...get().toasts, { id, kind, text }] });
    setTimeout(() => get().dismiss(id), kind === 'error' ? 12000 : 5000);
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),

  run: async (label, fn) => {
    set({ busy: label });
    try {
      return await fn();
    } catch (e) {
      get().toast(String(e), 'error');
      return undefined;
    } finally {
      set({ busy: null });
    }
  },
}));

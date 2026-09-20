/**
 * File dialogs: Tauri's inside the app, a prompt in the browser demo (where
 * the "path" is only a file name the download gets).
 */
import { open as tauriOpen, save as tauriSave } from '@tauri-apps/plugin-dialog';

import { pickBrowserFiles } from './browser-files';
import { inTauri, isLite } from './ipc';

const accept = (extensions: string[]) => (extensions.includes('*') ? undefined : extensions.map((e) => `.${e}`).join(','));

export async function pickFile(title: string, extensions: string[]): Promise<string | null> {
  if (isLite) return pickBrowserFiles({ accept: accept(extensions) });
  if (!inTauri) return window.prompt(`${title}\n(browser demo: files cannot be read here)`) || null;
  const picked = await tauriOpen({ multiple: false, title, filters: [{ name: title, extensions }, { name: 'All files', extensions: ['*'] }] });
  if (!picked) return null;
  return Array.isArray(picked) ? picked[0] : picked;
}

export async function pickFolder(title: string): Promise<string | null> {
  // Lite: a folder is either a set of files to read (SQ show) or a place to
  // write, which in a browser is always the downloads folder.
  if (isLite) return /import|holds/i.test(title) ? pickBrowserFiles({ directory: true }) : 'downloads';
  if (!inTauri) return window.prompt(`${title}\n(browser demo: files are downloaded instead)`, 'downloads') || null;
  const picked = await tauriOpen({ directory: true, title });
  if (!picked) return null;
  return Array.isArray(picked) ? picked[0] : picked;
}

export async function pickSave(title: string, defaultName: string, extensions: string[]): Promise<string | null> {
  // Lite: the browser asks where downloads go; the name is enough.
  if (isLite) return defaultName;
  if (!inTauri) return window.prompt(title, defaultName) || null;
  return tauriSave({ title, defaultPath: defaultName, filters: [{ name: title, extensions }] });
}

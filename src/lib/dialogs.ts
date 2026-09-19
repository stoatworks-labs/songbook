/**
 * File dialogs: Tauri's inside the app, a prompt in the browser demo (where
 * the "path" is only a file name the download gets).
 */
import { open as tauriOpen, save as tauriSave } from '@tauri-apps/plugin-dialog';

import { inTauri } from './ipc';

export async function pickFile(title: string, extensions: string[]): Promise<string | null> {
  if (!inTauri) return window.prompt(`${title}\n(browser demo: files cannot be read here)`) || null;
  const picked = await tauriOpen({ multiple: false, title, filters: [{ name: title, extensions }, { name: 'All files', extensions: ['*'] }] });
  if (!picked) return null;
  return Array.isArray(picked) ? picked[0] : picked;
}

export async function pickFolder(title: string): Promise<string | null> {
  if (!inTauri) return window.prompt(`${title}\n(browser demo: files are downloaded instead)`, 'downloads') || null;
  const picked = await tauriOpen({ directory: true, title });
  if (!picked) return null;
  return Array.isArray(picked) ? picked[0] : picked;
}

export async function pickSave(title: string, defaultName: string, extensions: string[]): Promise<string | null> {
  if (!inTauri) return window.prompt(title, defaultName) || null;
  return tauriSave({ title, defaultPath: defaultName, filters: [{ name: title, extensions }] });
}

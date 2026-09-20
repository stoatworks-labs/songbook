/// <reference types="vite/client" />

/** Injected by vite.config.ts from package.json. Shown in the header and the About dialog. */
declare const __APP_VERSION__: string;
/** True in the Songbook Lite build (lite/vite.config.ts): the Rust core runs in WebAssembly. */
declare const __SONGBOOK_LITE__: boolean;

interface Window {
  STOATWORKS_ABOUT?: Record<string, string>;
}

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

import { readFileSync } from 'node:fs';

const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));

// Songbook Lite: the desktop app's front end built as a website, with the
// Rust core compiled to WebAssembly (scripts/build-lite.sh puts songbook.wasm
// in lite/public). Deployed by wrangler from dist-lite/.
export default defineConfig({
  root: __dirname,
  publicDir: 'public',
  define: {
    __APP_VERSION__: JSON.stringify(`v${pkg.version}`),
    __SONGBOOK_LITE__: 'true',
  },
  plugins: [react()],
  base: '/',
  server: { port: 5179, strictPort: true },
  build: {
    outDir: '../dist-lite',
    emptyOutDir: true,
    sourcemap: false,
    target: 'es2022',
  },
});

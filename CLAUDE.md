# CLAUDE.md — Songbook

Command reference. For the model, the invariants and the traps, read
[AGENTS.md](AGENTS.md) first.

## Commands

```bash
npm install
npm run app          # tauri dev: vite on :5178 + cargo, opens the window
npm run app:build    # release bundle for this platform (src-tauri/target/release/bundle)
npm test             # vitest — the PDF and CSV builders on the demo shows
npm run lint         # oxlint
npm run typecheck    # tsc -b
cd src-tauri && cargo test --workspace     # every crate, on synthetic files and fake desks
cd src-tauri && cargo run --example seed -- ~/Documents/Songbook <show files…>
cd src-tauri && cargo run --example demo -- ../public/demo
cd src-tauri && cargo run --example write_back -- <vendor file or SQ folder> <out> 'ch:1=Kick' 'ch:2@usb:1' 'scene:2=Opening'
npm run build:lite   # Songbook Lite: cargo → lite/public/songbook.wasm, vite → dist-lite/
npm run dev:lite     # Lite on :5179 (build the wasm first)
```

`npm run dev` alone serves the browser demo (two example shows in memory). Songbook Lite is
deployed by `.github/workflows/deploy.yml` (wrangler, static assets) on every push to main.

## Release

`.github/workflows/desktop.yml` builds on a `v*` tag: macOS universal (unsigned — the
fleet's autosign agent notarises after publishing), Linux deb/rpm, Windows NSIS. Bump the
version in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` together.

## Verifying against a desk

No simulator exists for either vendor's protocol; the fakes live in the driver crates' tests.
The desks on this network: an SQ-5 (MIDI over TCP on 51325, set its MIDI channel in
Utility → General → MIDI) and, when it is on the bench, the DM3 at `dm3.local:49280`
(see docs/NOTES.md). Pull first — it is read-only — and store a scene on the desk before
any push.

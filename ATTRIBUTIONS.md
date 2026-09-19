# Attributions

Songbook is built on:

- [Tauri](https://tauri.app) (MIT / Apache-2.0) — the desktop shell.
- [React](https://react.dev) (MIT), [zustand](https://github.com/pmndrs/zustand) (MIT),
  [Vite](https://vite.dev) (MIT), [vitest](https://vitest.dev) (MIT), [oxlint](https://oxc.rs) (MIT).
- [pdf-lib](https://pdf-lib.js.org) (MIT) — the PDF.
- Rust crates: serde, serde_json, flate2, tar, zip, ureq, sha2, hex, uuid, chrono, base64, rand,
  url, thiserror (MIT / Apache-2.0).
- [Showbook](https://github.com/stoatworks-labs/showbook) (MIT) — the library, history, sync and
  OAuth crates and the app shell, which Songbook shares.
- [PatchFerret](https://github.com/stoatworks-labs/patchferret) (MIT) — the MBDF container and
  MMSXLIT decoders, the CL/QL table offsets, the SQ record signature and the A&H archive findings
  all came from its research; the readers here are its adapters re-targeted at this model.

Protocol and format knowledge came from:

- Allen & Heath, *SQ MIDI Protocol* Issue 5 (firmware V1.5.0+), *dLive MIDI Over TCP/IP Protocol*
  V2.0, *Avantis MIDI TCP/IP Protocol* (firmware V2.0+), *Qu-5/6/7 MIDI Protocol* Issue 2 — the
  message shapes, the NRPN tables, the channel-selection tables, the fader and gain laws, the
  colour codes.
- Allen & Heath's Avantis Director V2.01 and SQ MixPad 1.6.0, and Yamaha's QL Editor V5.8.1,
  DM3 Editor V3 and TF Editor V4.50 — the offline editors whose files the readers were built on.
- Yamaha's DM3 (firmware V3.00) on this fleet's bench on 2026-09-15 — the SCP grammar, the
  `prminfo` dictionary, the `NOTIFY` push and the scene verbs, as recorded in Dante-BabelBox.
- Bitfocus Companion 5.0.5 and the `allenheath-sq` 3.1.0, `allenheath-dlive` 1.0.1,
  `allenheath-avantis` 1.0.0 and `yamaha-rcp` 3.5.12 modules (MIT) — the page export shape and
  the action ids; the `yamaha-rcp` module also captured the per-family `prminfo` dictionaries
  that `src-tauri/crates/songbook-yamaha/dict/` embeds verbatim.
- The capability figures follow the vendors' spec sheets and the protocol documents' address
  ranges; where a figure is a family convention it is marked in the code.

Allen & Heath, SQ, dLive, Avantis, Qu, CQ, Yamaha, CL, QL, TF, DM3, DM7, RIVAGE PM and Bitfocus
Companion are trademarks of their owners. Songbook is not affiliated with any of them.

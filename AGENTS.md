# AGENTS.md — bringing an LLM up to speed on Songbook

Orientation for an AI assistant (or a new human) picking this project up cold. `CLAUDE.md`
holds the short command reference; this file explains the model and the traps.

## 1. What this is

A Tauri v2 desktop app (React front end, Rust core) that keeps a library of mixing desk show
files with version history, and does the things a show file should let you do: inspect, edit,
document (PDF or web page, CSV, label strips), convert between desks, build Companion pages, pull from and
push to the live desk. Two vendors are real today — Allen & Heath (SQ, dLive, Avantis) and
Yamaha (CL/QL, TF, DM3, DM7, RIVAGE PM) — and Qu / CQ are capability descriptors waiting for a
driver. It is the audio sibling of Showbook (video switchers) and shares its shape on purpose:
the library crate is the same code renamed.

It is a desktop app and not only a browser tool for one reason: the desks speak raw TCP (MIDI on
51325, SCP on 49280) that a hosted page cannot reach. Everything else ships twice: **Songbook
Lite** (`lite/`, deployed by `wrangler.toml` to songbook-lite.stoatworks-labs.com) is the same
React front end told at build time (`__SONGBOOK_LITE__`) to talk to the Rust core compiled to
WebAssembly (`crates/songbook-wasm`) and to keep its library in IndexedDB (`src/lib/lite.ts`).

## 2. The one idea

**Everything above the drivers sees only `songbook_model::Show`.** A driver reads a vendor file
or a live desk into that model and writes it back out; the inspector, the documentation, the conversion
and the history never look at vendor data. What a driver cannot express in the shared fields
goes into an entity's `extra` bag under the vendor's own name (`yamahaColor`, `ahTarget`,
`scpTable`), so nothing is lost on a round trip but nothing vendor-specific leaks up. Conversion
never reads `extra` and empties every bag, because a target driver addresses strips by kind and
number.

Four rules the model keeps:

- **IDs are stable strings** with a kind prefix (`ch:`, `st:`, `fxr:`, `bus:aux:`, `dca:`,
  `mg:`, `scene:`, `scene:a:`, `cue:`, `unit:`, `skt:<unit>:in:`), chosen by the driver so that
  re-importing the same file yields the same IDs and history diffs stay readable.
- **Levels are dB, pans are −1…+1, frequencies are Hz.** Fader off is `FADER_OFF` (−144 dB)
  because JSON has no −∞; every driver converts its own unit (an SQ 14-bit NRPN value, a Yamaha
  centi-dB integer, a dLive 7-bit level).
- **Preamps belong to the socket, not the channel.** Gain, pad and phantom live on the physical
  connector (`Preamp.socket_id`); a channel names its source socket. On a shared stage box every
  desk on it shares the gain, and modelling it per channel would lie about that.
- **`None` means "not read", never "off".** Every strip field is optional because every driver
  knows a different subset (the SQ protocol has no names; the CLF has no faders). The UI shows an
  indeterminate checkbox and an empty cell for unknown; a push writes only what is known.

`Show::validate` checks every reference; every driver's fixture test asserts it is empty.

## 3. Layout

```
src/types.ts                   the JSON shapes shared with Rust — read this first
src/lib/ipc.ts                 every invoke in one place; lite.ts is Songbook Lite's backend (wasm + IndexedDB), mock.ts the browser demo
src/lib/wasm.ts                the bytes-in/bytes-out bridge to songbook-wasm; browser-files.ts turns file pickers into path tokens
lite/                          Songbook Lite's index.html, vite.config.ts and static files; scripts/build-lite.sh builds it
src/store.ts                   zustand: settings, library entries, the open show, dirty flag
src/components/ShowView.tsx    the tabs; each tab is one file
src/lib/pdf.ts                 entry: builds the document, renders it as PDF or HTML
src/lib/document.ts            every section of the documentation, as blocks and diagrams
src/lib/doc/ir.ts              the block/diagram model both renderers draw
src/lib/doc/theme.ts           the five themes, the papers, colour helpers
src/lib/doc/diagrams.ts        shared diagram helpers (SVG paths, the column flow)
src/lib/doc/render-pdf.ts      pdf-lib renderer; render-html.ts writes one self-contained page
src/lib/glossary.ts            the glossary chapter; csv.ts and labels.ts beside them
src-tauri/src/lib.rs           Tauri commands: library, import, devices, convert, companion, sync
src-tauri/crates/songbook-model     Show, diff, summary, ids
src-tauri/crates/songbook-ah        midi.rs (wire codec), sq.rs (images + NRPN tables), dlive.rs (archive + channel tables), import.rs, live.rs
src-tauri/crates/songbook-yamaha    mbdf.rs, mms.rs, scene.rs (MBDF → Show), clf.rs, scp.rs (protocol + client + pull/push), dict/*.txt
src-tauri/crates/songbook-library   Library (history, vendor blobs), sync.rs (four providers), oauth.rs (PKCE)
src-tauri/crates/songbook-convert   capabilities.rs (per model), lib.rs (the conversion + report)
src-tauri/crates/songbook-companion export/import of .companionconfig pages
src-tauri/crates/songbook-wasm      the browser entry point: one C-ABI call framed as JSON + blobs, no wasm-bindgen
src-tauri/examples/demo.rs          writes public/demo/*.json; seed.rs imports files into a library
```

## 4. Invariants

- **Rust decides, TypeScript displays.** The model, every parse, the conversion and its report,
  the diff, the Companion page — all Rust. The UI edits the model and draws it. The exceptions,
  deliberately: the documentation, the CSV and the label strips are rendered in the webview and written to
  disk through one command.
- **Every save is a commit.** `Library::save` hashes the canonical JSON (with `meta.modified`
  blanked), keeps a gzip'd snapshot per distinct hash, appends to `history/index.json`. Saving
  the same thing twice makes no commit.
- **Vendor files are opaque and content-addressed.** An SQ show folder (zipped), a dLive archive,
  a `.CLF`, a `.dm3s` is stored under `vendor/<sha256><ext>` and listed in `show.vendor`. The
  kept file is never modified. `vendor_write` writes the show into a *copy* of an SQ show
  (`ah::write`) or a `.CLF` (`yamaha::clf::write`) — only the fields the reader decodes, with the
  checksum recomputed; the other kinds are export-only because their blobs are only partly decoded.
- **Conversion reports; it does not claim.** Every dropped entity is a `Note` with `level:
  dropped`, every nearest-neighbour substitution `adapted`. The converted show carries the report.
- **Nothing writes to a desk implicitly.** Pull is read-only. Push is a separate button with a
  confirm and per-kind checkboxes; scene recall and store are their own buttons with a confirm.
- **The pull reads what the desk lists, not what a table says.** The Yamaha driver asks the desk
  for its own `prmnum`/`prminfo` dictionary and only falls back to the embedded copy when it does
  not answer; the strip counts come from the dictionary's `xcount`. The dLive driver asks every
  possible strip for its name and keeps the ones that answer.
- **Sync never deletes.** The mirror copies newer files across and nothing else.
- **The wasm imports nothing.** `songbook-wasm` is built with plain `cargo build --target
  wasm32-unknown-unknown`; there is no wasm-bindgen glue, so anything that would import a host
  function (a clock, `getrandom`) breaks the page. The model takes its clock and its ID seed from
  the host (`set_clock`, `seed_ids`); `uuid` is a native-only dependency. CI checks the import
  section is empty.
- **Songbook Lite and the desktop app share every screen.** A screen that must differ asks
  `isLite` (Devices and Sync are hidden; file dialogs become pickers and downloads). Do not fork a
  component for Lite — add the branch.

## 5. Traps

- **The SQ NRPN address space is 14-bit, `MSB × 128 + LSB`.** The protocol document's tables
  look like `0x40 0x44` etc.; read as `MSB*256+LSB` the arithmetic breaks at every 128-boundary
  (Ip6 → Aux1 is `41 00`, not `40 80`). `sq::level_param` and friends do the arithmetic; the
  tests pin every worked example in the document.
- **Pan and assign are level + `0x10:00` / `0x20:00`**, and a master's balance exists for LR,
  aux and matrix only.
- **The SQ protocol carries no names, colours or preamps.** Pull into the show that already has
  the names (the Devices screen's "into an existing show") and the driver keeps them.
- **dLive / Avantis address a strip by MIDI channel offset + note.** Inputs on N, groups N+1,
  auxes N+2, matrices N+3, everything else on N+4; the ranges differ per desk and the mute-group
  base note is `4E` on dLive and `46` on Avantis. The `Target` enum owns this.
- **Mono and stereo mixes share the model's numbering.** A dLive stereo aux 1 becomes
  `bus:aux:<mono count + 1>`; `extra.ahTarget` remembers the desk's own target so a push goes
  back to the right one. A converted show has no `ahTarget` and is addressed by kind, number and
  its `stereo` flag.
- **SQ images: the class byte, 48 records, 0-based scene files, 16-byte names.** A patch record
  is `[index][00][class][fe]` at 336-byte stride from 0x38c — class 0 none, 1 Local, 2 SLink,
  3 USB, 4 I/O Port (MixPad's tab order; 1 and 3 seen). The first 48 records are Ip1–48 and the
  table runs on for 122 strips. `SCENE001.DAT` is scene **2**. The scene name field is 16 bytes
  (MixPad truncated a longer one on re-save). A scene image carries the same patch table as
  NVDATA — the patch the scene was stored with — so `ah::write` writes the new patch into every
  scene that agreed with the old NVDATA patch and leaves the others alone.
- **The two solved checksums.** An SQ image (`NVDATA.DAT`, `SCENEnnn.DAT`, 128 KiB) ends in a
  little-endian zlib CRC-32 over `[0x14, 0x1fffc)` — the 20-byte header is outside it. A `.CLF` is a
  chain of `ff 6c 00 00` records (u32 LE header length, 8-byte name, u32 LE data length); the
  `MEMAPI` record's last four bytes are the one's complement of the big-endian u32 word sum of the
  data before them, stored big-endian. Both were confirmed by reproducing the editors' own saves
  byte-for-byte (`sq::write_tests`, `clf::write_tests`, gated on the sample files). The `MMS`
  record's checksum field is 0 in every sample and is left alone.
- **The A&H name blocks: match the class from the known list.** The byte before the class name
  is the low half of the block length and is often a printable letter (`VMono Aux`, `XMono
  Group`); walking back over "letters" picks it up. `dlive::NAME_CLASSES` is the list.
- **MMSXLIT mixes endianness with the container.** Record headers are big-endian; the schema
  and the values are little-endian. Type codes: 0 string, 1 signed, 2 unsigned — `Fader/Level`
  and `ToMix/Level` are signed centi-dB with `-32768` for −∞.
- **Yamaha field names differ per model.** DM3 `Patch/Source` (4 bytes), TF `Patch/Select`
  (1 byte, a selector, resolves no connector), DM7 `InPatch`. `scene.rs` looks each up by name
  and reports absence; never hard-code DM3's layout.
- **The `Stereo` table holds L and R as two elements of one bus.** `scene.rs` takes element 0;
  an output patch word `0x0520` index 1/2 is ST L / ST R of `bus:main:1`.
- **DM3 head-amp gain is global, not scene data** (proved on the desk 2026-09-15): a scene
  recall never moves it, so neither does storing one. The pull says so in a note.
- **The embedded DM3 dictionary lists unsupported parameters with no status** (three spaces,
  then `prminfo`); the CL/QL one uses `--`. `parse_reply` treats both as `Unsupported`, otherwise
  `dictionary()` waits 3 s per chunk for replies that never come.
- **A `get_many` chunk waits for exactly as many replies as it sent.** Anything the fake or the
  desk answers with `ERROR` still counts; a line it drops costs the timeout. Keep chunks small.
- **`tauri dev` needs port 5178 free** (`strictPort`); the browser preview server and the app
  cannot run at once. The dev binary loads `devUrl`, so a `cargo build` binary needs the Vite
  server up.
- **System Events cannot click the webview** on this machine (as noted for Showbook); the
  screenshots were taken through Safari WebDriver on the browser demo (`#show=<id>&tab=<tab>`
  opens a show directly) and `screencapture -l <CGWindowID>` for the desktop window.

## 6. Verifying

- `cd src-tauri && cargo test --workspace` — the codecs against the protocol documents' worked
  examples, the readers against synthetic files and the editor-made fixtures under `fixtures/`
  (the CLF writer reproduces QL Editor's saves byte-for-byte; the SQ reader and CRC run on
  MixPad's images), the live drivers against in-process fake desks, the conversion, the Companion
  pages, the library, the wasm request framing.
- Env-gated tests against real files on this machine (skip when absent):
  `SONGBOOK_AH_SHOW=<Director .tar.gz>`, `SONGBOOK_YAMAHA_SCENES=<DM3 or TF Editor SceneList dir>`,
  `SONGBOOK_CLF_DIR=<dir of .CLF>` (defaults to `fixtures/ql5`).
- `./scripts/build-lite.sh` builds the wasm and the site into `dist-lite/`; `npm run dev:lite`
  serves Lite on :5179. Drive it from the page: `registerFiles([new File(...)])` from
  `src/lib/browser-files.ts` returns the token `api.importPath` takes, and wrapping
  `URL.createObjectURL` captures what would have downloaded.
- `cargo run --example seed -- <library> <files…>` imports from the terminal;
  `cargo run --example demo -- ../public/demo` regenerates the demo shows.
- `npm test` builds the PDF and the web page for the demo shows, in every theme; `npm run typecheck`, `npm run lint`.

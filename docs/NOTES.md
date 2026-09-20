# Notes

Working notes for this repo: decisions, and the traps that actually bit, written while
building it on 2026-09-19. Cross-cutting notes live in
[fleet-notes](https://github.com/stoatworks-labs/fleet-notes).

## Where the show files really are

- **SQ**: MixPad keeps a live show under `~/Library/Application Support/Allen & Heath/SQ-MixPad
  <version>/CurrentShow/` — `NVDATA.DAT` (global setup, the input patch) and `SCENEnnn.DAT`,
  each exactly 128 KiB, byte 0 `B5` / `A1`. MixPad writes nothing until you **log out**. A
  console's USB show is the same files under `AHSQ/Shows/<name>/`. Songbook zips the folder
  as the vendor blob.
- **Avantis / dLive**: Director stores shows under `~/Library/Application Support/AllenAndHeath/
  AllenHeath/Avantis/Data/Director/Shows/User/<name>.tar.gz` (dLive: `TLDV2.11/TLDData/Director/
  Shows/`). Gzipped tar; `Show/Scenes/StageBoxSceneNNN.tar.gz` are nested tars; `65535` is the
  live state and the only one with the `Channel Mapper` (the patch). Every save re-gzips with a
  new timestamp, so compare decompressed contents, never files.
- **Yamaha DM3 / TF**: factory scenes live inside the editors —
  `/Applications/DM3 Editor.app/Contents/Resources/FactoryPreset/SceneList/Bank{A,B}/*.dm3s`,
  same for TF Editor (`.tfs`); the editors' own descriptors are in `Contents/Resources/Descriptor/
  mms_*.xml` and agree exactly with the schema inside each scene. The DM3 Editor's user scene
  library is under `~/Library/Application Support/Yamaha/DM3 Editor/SceneList/`.
- **Yamaha CL/QL**: QL Editor saves `.CLF` from File → Save As; the samples used here are in
  the private `patchferret-research` repo's `samples/`.
- **Yamaha SCP dictionaries**: the `yamaha-rcp` Companion module ships one text file per family
  (`CLQL Parameters-1.txt` …) that is the verbatim `prminfo` output of a desk; copied into
  `src-tauri/crates/songbook-yamaha/dict/` so the driver can size a pull without a desk answering
  `prmnum`.

## A&H archive, as read

- Blocks are `[u16 BE length][label\0][payload]` and the length covers the label. The
  `<class> Channel Name Colour Manager` payload is one version byte, then `n` names of 9 bytes,
  then `n` colour bytes; `n` follows from the length. The Avantis factory show holds 96 input
  names, 56 mono aux, 28 stereo aux, 16 DCA, and so on — the archive lists every possible bus,
  not the configured mix (`MixConfig.dat` is 13 bytes and not decoded), so an imported show shows
  84 auxes. A pull over MIDI shows only the buses that answer.
- Colour bytes are the MIDI protocol's numbering: 0 off, 1 red, 2 green, 3 yellow, 4 blue,
  5 purple, 6 light blue (cyan here), 7 white. Assumed identical; not diffed.
- A numbered scene blob starts `01 01 <name>\0\0<description>\0`.
- Channel Mapper: 3 bytes per channel, `[type][index BE u16]`; 0x00 local, 0x03 SLink
  confirmed by diff; 0x11, 0x22, 0x25 seen and unidentified (I/O Port, USB, unpatched are the
  candidates). Reported, not guessed.

## SQ protocol, as read from Issue 5

- Parameter numbers are 14-bit. Mute = strip index (Ip 0–47, Grp 48–59, FXRtn 60–67, LR 68,
  Aux 69–80, FXSnd 81–84, Mtx 85–87; DCA at `02:00`+, mute groups at `04:00`+). Level to LR =
  `40:00` + index. Ip→Aux `40:44` + 12·(ip−1) + (aux−1); Grp→Aux `45:04` +; FXRtn→Aux `46:14` +;
  Ip→FX `4C:14` + 4·(ip−1) + (fx−1); Grp→FX `4D:54`; FXRtn→FX `4E:04`; LR→Mtx `4E:24`; Aux→Mtx
  `4E:27` + 3·(aux−1); Grp→Mtx `4E:4B` + 3·(grp−1); masters `4F:00` LR, +1… aux, `4F:0D` FX,
  `4F:11` Mtx, `4F:20` DCA. Pan = level + `10:00`; assign = level + `20:00`.
- Linear taper: 118.7 units per dB from −89 dB (4630) to +10 dB (16383); 0 is −∞. The desk's
  NRPN Fader Law must be Linear (the default); Audio Taper is a different table and is not
  implemented.
- A `get` is the NRPN with `BN 60 7F`; the reply is the ordinary set message. Scene recall is
  `BN 00 bank, CN prog` with 128 scenes per bank.

## dLive / Avantis protocol

- Mutes are Note On with velocity ≥ 0x40 (then Note Off); fader is NRPN param `17` with one data
  byte (`(dB+54)/64·127`, 0 = −∞); sends are SysEx `0N 0D CH SndN SndCH LV`; names `01`/`02`/`03`,
  colours `04`/`05`/`06`; preamps (dLive only) gain by Pitch Bend `EN MP GV` with
  `GV = (dB−5)/55·127`, pad `07/08/09`, 48V `0A/0B/0C` on MixRack socket numbers `00–3F`,
  DX1/2 `40–5F`, DX3/4 `60–7F`. The `get` forms are `SysEx 0N 05 09 CH` (mute), `0N 05 0B 17 CH`
  (fader), `0N 05 0B 18 CH` (main assign), `0N 05 0F 0D CH SndN SndCH` (send).
- dLive uses running status; Avantis too. The parser keeps the last status byte.
- The Avantis article states no fader table; the dLive law is assumed.

## Yamaha SCP

- `set MIXER:Current/InCh/Label/Name 0 0 "Kick"`; colours are strings (`Blue`, `SkyBlue`,
  `Pink` on TF/DM3; `Cyan`, `Magenta` on CL/QL); levels centi-dB; pans −63…63; head amps on
  `IO:Current/InCh/HAGain` (DM3/DM7/TF, whole dB) or `MIXER:Current/InCh/Port/HA/Gain` (CL/QL/
  RIVAGE, centi-dB).
- Scenes: CL/QL `ssrecall_ex MIXER:Lib/Scene n`; TF/DM3 `ssrecall_ex scene_a n`; DM7 and RIVAGE
  PM `scpmode sstype "text"` then `ssrecallt_ex … "n.00"`. `ssinfo_ex` lists a slot's title,
  comment and `user`/`empty`. There is no scene-clear verb.
- The desk pushes `NOTIFY set …` to every client with no subscription; the client keeps them
  aside. `scpmode keepalive 60000` holds the session.

## Building the desk pictures without a desk

Every live driver test runs against a fake in the same crate (`live::fake`, `scp::fake`):
a `TcpListener` thread that implements exactly the documented replies from a state table. It
proves the codec and the driver logic, not the desk. The first real pull will be the test that
matters; the DM3 at `dm3.local:49280` when it is on the bench, and the SQ-5, are the desks to
try.

## The two checksums (solved 2026-09-20)

Both blocked writing until they were solved with controlled saves from the vendors' editors.

**SQ images** (`NVDATA.DAT`, `SCENEnnn.DAT`, 128 KiB each). The last four bytes are a
little-endian zlib CRC-32 (reflected polynomial `0xEDB88320`, init and xorout `0xFFFFFFFF`)
over `[0x14, 0x1fffc)`: the 20-byte header (image kind, `00 fe ff…`, `01 06 00 01 04 00 00 00`)
is outside it. Found by trying the standard CRC family over candidate spans against MixPad's
`CurrentShow` images and matching both (`3d1b8fc2`, `6d27dd16`) on the first span that skipped
the header. `sq::fix_image_checksum` recomputes it; `write_nvdata_patch` and `write_scene_name`
call it.

**CL/QL `.CLF`.** After the 0x28-byte file header and a section directory the file is a chain
of records: `ff 6c 00 00`, u32 LE header length (20), an 8-byte NUL-padded name, u32 LE data
length, then the data. The tables live in the `MEMAPI` record (data `[0x6620, 0x1003c)` in the
QL5 samples); its last four bytes are the **one's complement of the sum of the big-endian u32
words** of the data before them, stored big-endian. Found from the deltas between QL Editor
saves: a one-byte patch change moved the stored value by exactly that byte times its
position weight, a five-character rename by the same arithmetic over five bytes. Confirmed by
reproducing three of the editor's files byte-for-byte from a fourth (`clf::write_tests`, run
with `SONGBOOK_CLF_DIR=~/Documents`). The following `MMS` record's checksum field is 0 in every
sample; the `00 00 ae 64` at 0x6608 and the file's last four bytes never moved under MEMAPI
edits and are left alone.

**Confirmed in the editors' binaries (2026-09-20).** Both editors ship unstripped x86_64
Mach-Os. MixPad: `cScene::CalcSceneCRC(const u8*)` feeds bytes `0x14 … 0x1fffb` one at a time
to `_CRC32` (a zlib table at 0x260860); `cScene::CheckBufferCRC` compares. QL Editor:
`mem_CalcCheckSumLong(const void*, unsigned len)` sums `endian_swap` of every u32 but the last
(`len/4 − 1` words), returns `~sum` byte-swapped; `CCollectionBase::setCheckSumToBuffer` stores
it in the buffer's last word (and for collection type 4 a second one at the end of a shorter
inner span — the MEMAPI record is not one, or our byte-exact reproductions would have missed it).
There is a 16-bit sibling `mem_CalcCheckSum` (BE u16 halfwords, complemented) that no file
path calls. The loader's error table (`ErrorTranslator::setupErrorMessages`): 0 no error,
−1 parameter out of range, −2 **checksum**, −3 not available, −4 same parameter, −5 read only,
−6 data size, −7 different kinds of data, −8 cancel, −9 version, −10 edit flag, −11 protected.
`CCollectionBase::CheckSetDataValidity` clamps out-of-range parameters on load and *re-writes*
the checksum afterwards, so a value out of range costs −1, not a refusal.

The `.CLF` layout, from the section directory at 0x30 (`[u16 id][u16 0][u32 BE offset]`
entries, 35 of them in the QL5 file): every section starts with a 0x18-byte collection header
`01 00 00 00 00 00 00 08 | 01 70 <id> 00 | <count> | 02 00 00 00 | <element size>`. Section
0x10 (scene memory, at 0x5324) holds the `MEMAPI` (current mix) and `MMS` records; the file's
last four bytes are the same complement-sum over the trailing 0x600 bytes `[0x1c504, 0x1cb00)`.
The `00 00 ae 64` at 0x6608 is the last word of section 0x10's table before the MEMAPI record
and matches no sum rule tried; it never moves under MEMAPI edits.

**Verified in the editors (2026-09-20).** QL Editor loaded a Songbook-written `.CLF` (CH1
renamed, CH2 → DANTE33, CH3 unpatched, a long name cut to eight) and refused the same file with
one checksum bit flipped: `Load operation not complete due to an error. Checksum error. (-2)`.
MixPad (offline, SQ-7) loaded a Songbook-written `NVDATA.DAT` + `SCENE001.DAT`, showed Ip1 →
Local 7, Ip2 unpatched, Ip3 → Local 3 in its I/O Patch matrix and the scene name in its list,
and on logout re-saved `NVDATA.DAT` byte-identical to Songbook's output. Its re-save of the
scene image also taught three things: the scene name field is 16 bytes (the 19-character name
came back as 16 + zeros); `SCENE001.DAT` is listed as scene 2 (the file index is 0-based, and a
fresh store there is named "Scene 2"); and the scene image carries the same 48 patch records as
NVDATA (Ip1/Ip2 moved in both). The default SQ-7 show's records also settled the class byte:
Ip1–32 `(n, 1)`, Ip33–40 `(0, 0)`, Ip41–46 `(49–54, 1)` — the three stereo TRS pairs — and
Ip47/48 `(1–2, 3)` — USB 1/2 — so byte +2 is the socket class in MixPad's tab order, not a flag.

**Driving MixPad from a session.** Its JUCE views ignore System Events `click at`, but the
background `app_click` events *queue* and are delivered when the app is next activated — click
in the background, then `activate`, then look. Other sessions' GUI work steals focus mid-script,
so never type blind; check the frontmost process before every keystroke.

**Still to prove on a desk:** that a console accepts a written file; a console's loader may
check more than the editors do.

## Songbook Lite (2026-09-20)

The browser build follows PatchFerret's shape: the crates compiled for `wasm32-unknown-unknown`
behind one C-ABI call (`sb_call`: `u32 len | JSON | u32 n | (u32 len | bytes)*` both ways), no
wasm-bindgen, so the toolchain is `cargo build --profile wasm --target wasm32-unknown-unknown`
and the module has an empty import section (CI asserts it). What that cost: `chrono`'s default
`wasmbind` feature and `uuid`'s `getrandom` both import host functions, so the model takes a
clock (`set_clock`) and an ID seed (`seed_ids`) from JS and `uuid` is native-only. The `wasm`
profile (`opt-level = "s"`, LTO, one codegen unit, `panic = "abort"`) gives 1.6 MB raw / 313 KB
brotli; the SCP dictionaries are only 88 KB of that, the rest is code.

The front end is the desktop app's, with `__SONGBOOK_LITE__` defined by `lite/vite.config.ts`
and `src/lib/lite.ts` implementing the whole `api` over the wasm plus an IndexedDB library
(`shows`, `versions`, `vendor` stores; a save is a commit when the content hash moves, the diff
is Rust's through the wasm). File dialogs become `<input type=file>` pickers whose result is a
token the api spends (`browser-files.ts`), saves become downloads, and the SQ write-back is
always a zip because a page cannot write a folder. Measured in the tab: a QL5 `.CLF` imports in
15 ms, an SQ show in 40 ms; a written `.CLF` re-imports with its edits.

Vite dev traps: with `root: lite/`, the shared sources are served under
`/@fs/Users/…/src/…`, and after an HMR invalidation the live module URLs carry `?t=…` — a
console `import()` of the bare URL gets a *second* module instance with its own state (the file
token map), so pick the URL from `performance.getEntriesByType('resource')`.

## Things deliberately not done

- Writing the A&H archives and the Yamaha MBDF scenes back. Their blobs are only partly
  decoded, so a write could not promise to leave the rest intact. (The SQ images and the CLF are
  written — see the checksum section below.)
- Decoding the SQ scene images beyond the name, the CLF beyond patch and names, the A&H blobs
  beyond patch, names, colours and scene titles. Each needs its own controlled diff; the live
  driver reads the same data from the desk.
- Feedbacks on the Companion buttons (mute state colouring). The export carries actions only.

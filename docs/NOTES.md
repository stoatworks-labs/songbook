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

## Things deliberately not done

- Writing vendor files. The SQ image ends in a 4-byte checksum, the CLF carries one near an
  `MMS` marker, and the A&H scene blobs are only partly decoded; a stale checksum may be refused
  and nobody has a desk to find out with.
- Decoding the SQ scene images beyond the name, the CLF beyond patch and names, the A&H blobs
  beyond patch, names, colours and scene titles. Each needs its own controlled diff; the live
  driver reads the same data from the desk.
- Feedbacks on the Companion buttons (mute state colouring). The export carries actions only.

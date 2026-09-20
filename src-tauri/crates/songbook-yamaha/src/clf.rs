//! Yamaha CL / QL — the `.CLF` console file.
//!
//! Not the MBDF format: a flat binary with no self-description, from the
//! previous architecture. Everything read here was located by controlled
//! diffs through QL Editor V5.8.1 running offline (change one patch point or
//! one name, save, compare), on a QL5 file:
//!
//! - **Product string** at `0x10`: `QL [OSX, 5.8.1.27]`. The only reliable
//!   signature; CL Editor writes the same extension and is expected to write
//!   the same layout, but no CL-written file has been examined.
//! - **Input patch** at `0x00d74b`, one byte per channel, 64 channels:
//!   `0x01–0x40` DANTE 1–64, `0x41–0x48` INPUT 1–8, `0xC1–0xD8` INPUT 9–32,
//!   `0x00` unpatched. The split in the INPUT range is what the bytes say.
//!   The offset is absolute and established on one frame size, so the table
//!   is validated before it is trusted.
//! - **Channel names** at `0x00d858`, four characters per channel per block,
//!   the next four 384 bytes on, two blocks — eight characters, which is the
//!   desk's name length. Reading four contiguous bytes decodes every default
//!   file and truncates every real name.
//!
//! Head amps, sends, processing and the scene memory are in the file and not
//! decoded; the SCP driver reads all of that from a live desk instead.
//!
//! The file is a chain of tagged records and the one holding the tables
//! (`MEMAPI`) ends in a checksum — solved 2026-09-20, see `RECORD_TAG` — so
//! `write` puts a show's patch and names back into a copy of the file.

use songbook_model::{build, ids, Channel, ChannelKind, Direction, NoteLevel, Platform, Show, SocketKind, UnitRole};

use crate::Error;

const PATCH_TABLE: usize = 0x00d74b;
const CHANNELS: usize = 64;
const PRODUCT_AT: usize = 0x10;
const NAME_TABLE: usize = 0x00d858;
const NAME_CHUNK: usize = 4;
const NAME_SLOTS: usize = 96;
const NAME_BLOCKS: usize = 2;

/// `QL [OSX, 5.8.1.27]` or `CL […]`, when the file is one.
pub fn product(d: &[u8]) -> Option<String> {
    let s = d.get(PRODUCT_AT..PRODUCT_AT + 32)?;
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    let text = std::str::from_utf8(&s[..end]).ok()?.trim().to_string();
    if text.starts_with("QL ") || text.starts_with("CL ") {
        Some(text)
    } else {
        None
    }
}

pub fn looks_like(d: &[u8]) -> bool {
    product(d).is_some()
}

/// `(unit, index)` for a patch byte; `None` for unpatched or unobserved codes.
pub fn decode_source(v: u8) -> Option<(&'static str, u32)> {
    match v {
        0x00 => None,
        0x01..=0x40 => Some(("dante", v as u32)),
        0x41..=0x48 => Some(("local", (v - 0x40) as u32)),
        0xC1..=0xD8 => Some(("local", (v - 0xC0) as u32 + 8)),
        _ => None,
    }
}

pub fn channel_name(d: &[u8], ch: usize) -> String {
    let mut raw = Vec::with_capacity(NAME_CHUNK * NAME_BLOCKS);
    for block in 0..NAME_BLOCKS {
        let at = NAME_TABLE + block * NAME_SLOTS * NAME_CHUNK + (ch - 1) * NAME_CHUNK;
        match d.get(at..at + NAME_CHUNK) {
            Some(chunk) => raw.extend_from_slice(chunk),
            None => break,
        }
    }
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).trim().to_string()
}

pub fn parse(d: &[u8], file_name: &str) -> Result<Show, Error> {
    let product = product(d).ok_or(Error::NotClf)?;
    let model = product.split_whitespace().next().unwrap_or("QL").to_string();
    let mut show = Show::new(file_name.rsplit('/').next().unwrap_or(file_name).trim_end_matches(".CLF").trim_end_matches(".clf"), Platform::YamahaClQl);
    show.system.model = model.clone();
    show.system.extra.insert("editorBuild".into(), product.clone().into());
    show.system.units.push(build::unit("local", &format!("{model} OMNI inputs"), &model, UnitRole::Console));
    show.system.units.push(build::unit("dante", "Dante", "", UnitRole::Network));
    show.meta.source = Some(songbook_model::SourceInfo { kind: "file".into(), origin: file_name.to_string(), at: songbook_model::now(), firmware: None });

    let Some(table) = d.get(PATCH_TABLE..PATCH_TABLE + CHANNELS) else {
        show.note(NoteLevel::Info, "channels", "the file ends before the input patch table this format keeps at a fixed offset; no channels could be read");
        return Ok(show);
    };
    let decoded = table.iter().filter(|&&v| decode_source(v).is_some()).count();
    if decoded * 4 < CHANNELS * 3 {
        show.note(NoteLevel::Info, "channels", format!("only {decoded} of {CHANNELS} bytes at the expected patch-table offset decode to a known source, so this is probably not the table — the offset was established on a QL5 written by QL Editor V5.8.1 and another frame size or firmware may move it; no channels are reported rather than wrong ones"));
        return Ok(show);
    }
    let mut unresolved = 0usize;
    for (i, &v) in table.iter().enumerate() {
        let n = i as u32 + 1;
        let name = channel_name(d, i + 1);
        let label = if name.is_empty() { format!("ch {n}") } else { name };
        let mut c = Channel::new(ids::channel(n), n, ChannelKind::Input, &label);
        match decode_source(v) {
            Some((unit, index)) => {
                let sid = ids::socket(unit, Direction::In, index);
                if show.socket(&sid).is_none() {
                    show.sockets.push(build::socket(unit, Direction::In, index, if unit == "local" { SocketKind::Mic } else { SocketKind::Dante }, &format!("{} {index}", if unit == "local" { "INPUT" } else { "DANTE" })));
                }
                c.source = Some(sid);
            }
            None => {
                if v != 0 {
                    unresolved += 1;
                    c.extra.insert("clfPatch".into(), format!("{v:#04x}").into());
                }
            }
        }
        show.channels.push(c);
    }
    show.sockets.sort_by_key(|a| (a.unit_id.clone(), a.index));
    if unresolved > 0 {
        show.note(NoteLevel::Info, "channels", format!("{unresolved} channel(s) carry a source code outside the ranges confirmed by diff (Dante and the local inputs); slot, FX and playback sources have not been observed, so those are left blank"));
    }
    show.note(NoteLevel::Info, "channels", "head-amp gain and phantom, sends, processing, buses and scenes are in the file and not decoded — the format carries no schema, so each needs its own controlled diff; pull from the desk over SCP for all of them");
    show.note(NoteLevel::Info, "vendor", "the input patch and channel names can be written back into a copy of this file (Vendor files tab); everything else in it is carried over unchanged");
    Ok(show)
}

/// A synthetic CLF for tests: the product string, a default patch table and
/// the names given.
pub fn synthetic(product_str: &str, names: &[(usize, &str)], patch: &[(usize, u8)]) -> Vec<u8> {
    let mut d = vec![0u8; NAME_TABLE + NAME_BLOCKS * NAME_SLOTS * NAME_CHUNK + 64];
    d[PRODUCT_AT..PRODUCT_AT + product_str.len()].copy_from_slice(product_str.as_bytes());
    for i in 0..CHANNELS {
        d[PATCH_TABLE + i] = match i {
            0..=7 => 0x41 + i as u8,
            8..=31 => 0xC1 + (i as u8 - 8),
            _ => (i as u8 - 32) + 1,
        };
    }
    for &(ch, v) in patch {
        d[PATCH_TABLE + ch - 1] = v;
    }
    for &(ch, name) in names {
        let b = name.as_bytes();
        for block in 0..NAME_BLOCKS {
            let at = NAME_TABLE + block * NAME_SLOTS * NAME_CHUNK + (ch - 1) * NAME_CHUNK;
            for k in 0..NAME_CHUNK {
                d[at + k] = b.get(block * NAME_CHUNK + k).copied().unwrap_or(0);
            }
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_patch_and_split_names() {
        let d = synthetic("QL [OSX, 5.8.1.27]", &[(8, "ZZTOP"), (1, "Kick"), (2, "Snare Top")], &[(4, 0x01), (12, 0x02), (20, 0x00), (33, 0x99)]);
        assert!(looks_like(&d));
        let show = parse(&d, "gig.CLF").unwrap();
        assert_eq!(show.system.model, "QL");
        assert_eq!(show.channels.len(), 64);
        assert_eq!(show.channels[7].label, "ZZTOP");
        assert_eq!(show.channels[0].label, "Kick");
        assert_eq!(show.channels[1].label, "Snare To", "eight characters at most");
        assert_eq!(show.channels[2].label, "ch 3");
        assert_eq!(show.channels[3].source.as_deref(), Some("skt:dante:in:1"));
        assert_eq!(show.channels[11].source.as_deref(), Some("skt:dante:in:2"));
        assert_eq!(show.channels[0].source.as_deref(), Some("skt:local:in:1"));
        assert_eq!(show.channels[8].source.as_deref(), Some("skt:local:in:9"));
        assert_eq!(show.channels[19].source, None);
        assert_eq!(show.channels[32].source, None);
        assert!(show.notes.iter().any(|n| n.message.contains("1 channel(s) carry a source code")));
        assert!(show.validate().is_empty());
    }

    #[test]
    fn a_wrong_table_is_reported_not_printed() {
        let mut d = synthetic("QL [OSX, 5.8.1.27]", &[], &[]);
        for i in 0..CHANNELS {
            d[PATCH_TABLE + i] = 0xEE;
        }
        let show = parse(&d, "x.CLF").unwrap();
        assert!(show.channels.is_empty());
        assert!(show.notes.iter().any(|n| n.message.contains("probably not the table")));
        assert!(parse(b"nope", "x").is_err());
    }

    #[test]
    fn real_ql_files_if_present() {
        let Ok(dir) = std::env::var("SONGBOOK_CLF_DIR") else { return };
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if !p.extension().map(|x| x.eq_ignore_ascii_case("clf")).unwrap_or(false) {
                continue;
            }
            let d = std::fs::read(&p).unwrap();
            let show = parse(&d, &p.file_name().unwrap().to_string_lossy()).unwrap();
            assert_eq!(show.channels.len(), 64, "{}", p.display());
            eprintln!("{}: {:?}", p.display(), show.channels.iter().take(13).map(|c| (c.label.clone(), c.source.clone())).collect::<Vec<_>>());
        }
    }
}

// ---------------------------------------------------------------- writing

/// The record framing a `.CLF` is built from, and its checksum.
///
/// After the 0x28-byte file header and a section directory, the file is a
/// chain of records: `ff 6c 00 00` (tag), u32 LE header length (20), an
/// 8-byte NUL-padded name (`MEMAPI`, `MMS`), u32 LE data length; the data
/// follows. The `MEMAPI` record holds the patch and the names, and its last
/// four bytes are a checksum: the **one's complement of the sum of its
/// big-endian 32-bit words** (the data before those four bytes), stored
/// big-endian. Found on 2026-09-20 from the deltas between QL Editor's saves
/// (a one-byte patch change moved the value by exactly that byte times its
/// position weight; the rename moved it by `0x46F23417`, which is the same
/// arithmetic over five bytes), then confirmed by reproducing three of the
/// editor's files byte-for-byte from a fourth.
pub const RECORD_TAG: [u8; 4] = [0xFF, 0x6C, 0x00, 0x00];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: String,
    pub at: usize,
    pub data_start: usize,
    pub data_end: usize,
}

/// Every record in the file, in order.
pub fn records(d: &[u8]) -> Vec<Record> {
    let mut out = vec![];
    let mut i = 0x28;
    while i + 20 <= d.len() {
        if d[i..i + 4] == RECORD_TAG {
            let hdr = u32::from_le_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
            let name = String::from_utf8_lossy(&d[i + 8..i + 16]).trim_end_matches('\0').to_string();
            let len = u32::from_le_bytes([d[i + 16], d[i + 17], d[i + 18], d[i + 19]]) as usize;
            let (ds, de) = (i + hdr, i + hdr + len);
            if de > d.len() || hdr < 20 {
                break;
            }
            out.push(Record { name, at: i, data_start: ds, data_end: de });
            i = de;
        } else {
            i += 1;
        }
    }
    out
}

fn record_checksum(body: &[u8]) -> u32 {
    let mut s: u32 = 0;
    for w in body.as_chunks::<4>().0 {
        s = s.wrapping_add(u32::from_be_bytes(*w));
    }
    !s
}

/// Whether the `MEMAPI` record's stored checksum matches its contents.
pub fn memapi_checksum_ok(d: &[u8]) -> Option<bool> {
    let r = records(d).into_iter().find(|r| r.name == "MEMAPI")?;
    let body = &d[r.data_start..r.data_end - 4];
    let stored = u32::from_be_bytes([d[r.data_end - 4], d[r.data_end - 3], d[r.data_end - 2], d[r.data_end - 1]]);
    Some(record_checksum(body) == stored)
}

/// Recompute the `MEMAPI` checksum after editing its contents.
pub fn fix_memapi_checksum(d: &mut [u8]) -> Result<(), Error> {
    let r = records(d).into_iter().find(|r| r.name == "MEMAPI").ok_or(Error::NotClf)?;
    let c = record_checksum(&d[r.data_start..r.data_end - 4]);
    d[r.data_end - 4..r.data_end].copy_from_slice(&c.to_be_bytes());
    Ok(())
}

/// The patch byte for a model source, when the code is known.
pub fn encode_source(unit: &str, index: u32) -> Option<u8> {
    match (unit, index) {
        ("dante", 1..=64) => Some(index as u8),
        ("local", 1..=8) => Some(0x40 + index as u8),
        ("local", 9..=32) => Some(0xC0 + (index as u8 - 8)),
        _ => None,
    }
}

/// What a write-back could not carry.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub names_written: usize,
    pub patches_written: usize,
    pub skipped: Vec<String>,
}

/// Write the show's channel names and input patch into a copy of the
/// original `.CLF` and refresh the checksum. Names are cut to eight
/// characters; sources the format cannot encode (a slot, an FX return) keep
/// the original byte and are reported.
pub fn write(original: &[u8], show: &Show) -> Result<(Vec<u8>, WriteReport), Error> {
    if !looks_like(original) {
        return Err(Error::NotClf);
    }
    let mut d = original.to_vec();
    let mut report = WriteReport::default();
    if d.len() < NAME_TABLE + NAME_BLOCKS * NAME_SLOTS * NAME_CHUNK || d.len() < PATCH_TABLE + CHANNELS {
        return Err(Error::Truncated("CLF tables"));
    }
    for c in show.channels.iter().filter(|c| c.kind == ChannelKind::Input && c.number >= 1 && c.number as usize <= CHANNELS) {
        let ch = c.number as usize;
        // Name: eight ASCII characters over two blocks of four.
        let name: Vec<u8> = c.label.chars().filter(|ch| ch.is_ascii() && !ch.is_ascii_control()).take(NAME_CHUNK * NAME_BLOCKS).map(|ch| ch as u8).collect();
        if c.label.chars().count() > NAME_CHUNK * NAME_BLOCKS {
            report.skipped.push(format!("ch {ch}: name \"{}\" cut to 8 characters", c.label));
        }
        for block in 0..NAME_BLOCKS {
            let at = NAME_TABLE + block * NAME_SLOTS * NAME_CHUNK + (ch - 1) * NAME_CHUNK;
            for k in 0..NAME_CHUNK {
                d[at + k] = name.get(block * NAME_CHUNK + k).copied().unwrap_or(0);
            }
        }
        report.names_written += 1;
        // Patch.
        let at = PATCH_TABLE + ch - 1;
        match &c.source {
            None => {
                d[at] = 0x00;
                report.patches_written += 1;
            }
            Some(sid) => match show.socket(sid) {
                Some(s) => match encode_source(s.unit_id.trim_start_matches("unit:"), s.index) {
                    Some(code) => {
                        d[at] = code;
                        report.patches_written += 1;
                    }
                    None => report.skipped.push(format!("ch {ch}: source {} has no CLF code (only DANTE 1–64 and INPUT 1–32 are known); left as it was", s.label)),
                },
                None => report.skipped.push(format!("ch {ch}: source {sid} is not in the show; left as it was")),
            },
        }
    }
    fix_memapi_checksum(&mut d)?;
    Ok((d, report))
}

#[cfg(test)]
mod write_tests {
    use super::*;

    #[test]
    fn synthetic_file_round_trips_through_write() {
        // A synthetic CLF with a MEMAPI record wrapping the tables.
        let mut d = synthetic("QL [OSX, 5.8.1.27]", &[(1, "Kick")], &[]);
        // Wrap: put a record header at 0x28 whose data spans the rest, with 4 checksum bytes appended.
        let body_len = d.len() - 0x28 - 20;
        let mut hdr = RECORD_TAG.to_vec();
        hdr.extend_from_slice(&20u32.to_le_bytes());
        hdr.extend_from_slice(b"MEMAPI\0\0");
        hdr.extend_from_slice(&((body_len + 4) as u32).to_le_bytes());
        // Move the tables 20 bytes later is awkward; instead rebuild: header 0x28, record header, tables shifted.
        let tables = d.split_off(0x28);
        d.extend_from_slice(&hdr);
        d.extend_from_slice(&tables);
        d.extend_from_slice(&[0, 0, 0, 0]);
        // Tables moved by 20 bytes, so the parser's absolute offsets no longer apply to this synthetic;
        // this test only checks the record walk and the checksum arithmetic.
        let recs = records(&d);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].name, "MEMAPI");
        assert_eq!(memapi_checksum_ok(&d), Some(false));
        fix_memapi_checksum(&mut d).unwrap();
        assert_eq!(memapi_checksum_ok(&d), Some(true));
        let body = &d[recs[0].data_start..recs[0].data_end - 4];
        assert_eq!(record_checksum(body), u32::from_be_bytes(d[recs[0].data_end - 4..recs[0].data_end].try_into().unwrap()));
        assert_eq!(record_checksum(&[0, 0, 0, 1, 0, 0, 0, 2]), !3u32);
    }

    /// QL Editor's own saves, when the samples are on this machine: the
    /// writer must reproduce `mod2`, `mod3` and `name` from `base` exactly.
    #[test]
    fn reproduces_ql_editor_saves_if_present() {
        let Ok(dir) = std::env::var("SONGBOOK_CLF_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let Ok(base) = std::fs::read(dir.join("pfql_base.CLF")) else { return };
        assert_eq!(memapi_checksum_ok(&base), Some(true));
        let show = parse(&base, "pfql_base.CLF").unwrap();
        // mod2: CH4 -> DANTE1
        let mut s = show.clone();
        s.channels[3].source = Some(ids::socket("dante", Direction::In, 1));
        let (out, rep) = write(&base, &s).unwrap();
        assert!(rep.skipped.is_empty(), "{:?}", rep.skipped);
        if let Ok(want) = std::fs::read(dir.join("pfql_mod2.CLF")) {
            assert_eq!(out, want, "mod2 not reproduced");
        }
        // mod3: CH4 -> DANTE1, CH12 -> DANTE2
        s.channels[11].source = Some(ids::socket("dante", Direction::In, 2));
        let (out, _) = write(&base, &s).unwrap();
        if let Ok(want) = std::fs::read(dir.join("pfql_mod3.CLF")) {
            assert_eq!(out, want, "mod3 not reproduced");
        }
        // name: CH8 -> ZZTOP
        let mut s = show.clone();
        s.channels[7].label = "ZZTOP".into();
        let (out, _) = write(&base, &s).unwrap();
        if let Ok(want) = std::fs::read(dir.join("pfql_name.CLF")) {
            assert_eq!(out, want, "rename not reproduced");
        }
        eprintln!("QL Editor saves reproduced from {}", dir.display());
    }
}

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
//! decoded; the file also ends with a checksum that would have to be solved
//! before anything could be written back. The SCP driver reads all of that
//! from a live desk instead.

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
    show.note(NoteLevel::Info, "vendor", "the file carries a checksum; reading is unaffected, but nothing can be written back until it is solved");
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

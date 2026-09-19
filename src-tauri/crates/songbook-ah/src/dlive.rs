//! Allen & Heath dLive and Avantis: the show archive, and the MIDI protocol's
//! channel-selection tables.
//!
//! # The show archive
//!
//! A show is a gzipped tar of per-subsystem files under `Show/`; the scenes
//! are nested gzipped tars, one `StageBoxSceneNNN.tar.gz` and one
//! `SurfaceSceneNNN.tar.gz` per slot, and slot 65535 is the live state. The
//! scene blobs are binary but every block carries a readable label naming its
//! type and object (`"Parametric EQ, Input Channel 07"`), preceded by a
//! big-endian u16 length that covers the label and its payload. Three blocks
//! are read here:
//!
//! - `Channel Mapper` — the input patch, three bytes per channel:
//!   `[type][index u16 BE, 0-based]`, `0x00` local and `0x03` SLink confirmed
//!   by controlled diff in Avantis Director; other codes are reported.
//! - `<class> Channel Name Colour Manager` — one per object class: a version
//!   byte, then `n` names of 9 bytes (8 characters and a NUL), then `n`
//!   colour bytes in the same numbering the MIDI protocol uses
//!   (`00` off … `07` white). `n` follows from the block length. Located in
//!   this project from the factory shows; a rename-and-diff has not been done
//!   on it, so the layout is stated as observed.
//! - A numbered scene's own blob leads with its name and description.
//!
//! The archive does not say which console it came from: Avantis and dLive
//! shows have the same structure and a user-saved show names its scenes
//! whatever the user did. The platform comes from the caller.
//!
//! # The protocol tables
//!
//! Both desks select an audio channel with a MIDI channel offset from the
//! base `N` (inputs on `N`, groups `N+1`, auxes `N+2`, matrices `N+3`, the
//! rest on `N+4`) and a note number. The ranges differ per desk and are the
//! published ones (dLive MIDI over TCP/IP V2.0, Avantis MIDI TCP/IP for
//! firmware V2.0+).

use std::io::Read;

use songbook_model::{ids, BusKind, FADER_OFF};

/// Which desk's channel table applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Dlive,
    Avantis,
}

/// A selectable audio channel on a dLive / Avantis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    Input(u32),
    MonoGroup(u32),
    StereoGroup(u32),
    MonoAux(u32),
    StereoAux(u32),
    MonoMatrix(u32),
    StereoMatrix(u32),
    MonoFxSend(u32),
    StereoFxSend(u32),
    FxReturn(u32),
    Main(u32),
    Dca(u32),
    MuteGroup(u32),
}

/// Counts per family: (inputs, mono grp, stereo grp, mono aux, stereo aux,
/// mono mtx, stereo mtx, mono fx, stereo fx, fx rtn, mains, dcas, mute groups).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub inputs: u32,
    pub mono_groups: u32,
    pub stereo_groups: u32,
    pub mono_auxes: u32,
    pub stereo_auxes: u32,
    pub mono_matrices: u32,
    pub stereo_matrices: u32,
    pub mono_fx_sends: u32,
    pub stereo_fx_sends: u32,
    pub fx_returns: u32,
    pub mains: u32,
    pub dcas: u32,
    pub mute_groups: u32,
    /// Note number of mute group 1 on `N+4`.
    mute_group_note: u8,
    pub scenes: u32,
}

impl Family {
    pub fn limits(self) -> Limits {
        match self {
            Family::Dlive => Limits {
                inputs: 128,
                mono_groups: 62,
                stereo_groups: 31,
                mono_auxes: 62,
                stereo_auxes: 31,
                mono_matrices: 62,
                stereo_matrices: 31,
                mono_fx_sends: 16,
                stereo_fx_sends: 16,
                fx_returns: 16,
                mains: 6,
                dcas: 24,
                mute_groups: 8,
                mute_group_note: 0x4E,
                scenes: 500,
            },
            Family::Avantis => Limits {
                inputs: 96,
                mono_groups: 54,
                stereo_groups: 27,
                mono_auxes: 54,
                stereo_auxes: 27,
                mono_matrices: 54,
                stereo_matrices: 27,
                mono_fx_sends: 12,
                stereo_fx_sends: 12,
                fx_returns: 12,
                mains: 3,
                dcas: 16,
                mute_groups: 8,
                mute_group_note: 0x46,
                scenes: 500,
            },
        }
    }
}

impl Target {
    /// `(MIDI channel offset from N, note number)`, or `None` when the desk
    /// has no such channel.
    pub fn address(self, family: Family) -> Option<(u8, u8)> {
        let l = family.limits();
        let n = |v: u32, max: u32, base: u8| if v >= 1 && v <= max { Some(base + (v - 1) as u8) } else { None };
        Some(match self {
            Target::Input(v) => (0, n(v, l.inputs, 0x00)?),
            Target::MonoGroup(v) => (1, n(v, l.mono_groups, 0x00)?),
            Target::StereoGroup(v) => (1, n(v, l.stereo_groups, 0x40)?),
            Target::MonoAux(v) => (2, n(v, l.mono_auxes, 0x00)?),
            Target::StereoAux(v) => (2, n(v, l.stereo_auxes, 0x40)?),
            Target::MonoMatrix(v) => (3, n(v, l.mono_matrices, 0x00)?),
            Target::StereoMatrix(v) => (3, n(v, l.stereo_matrices, 0x40)?),
            Target::MonoFxSend(v) => (4, n(v, l.mono_fx_sends, 0x00)?),
            Target::StereoFxSend(v) => (4, n(v, l.stereo_fx_sends, 0x10)?),
            Target::FxReturn(v) => (4, n(v, l.fx_returns, 0x20)?),
            Target::Main(v) => (4, n(v, l.mains, 0x30)?),
            Target::Dca(v) => (4, n(v, l.dcas, 0x36)?),
            Target::MuteGroup(v) => (4, n(v, l.mute_groups, l.mute_group_note)?),
        })
    }

    /// The reverse: which channel a `(offset, note)` names.
    pub fn from_address(family: Family, offset: u8, note: u8) -> Option<Target> {
        let l = family.limits();
        let within = |base: u8, max: u32| -> Option<u32> {
            if note >= base && (note - base) < max as u8 {
                Some((note - base) as u32 + 1)
            } else {
                None
            }
        };
        match offset {
            0 => within(0x00, l.inputs).map(Target::Input),
            1 => within(0x40, l.stereo_groups).map(Target::StereoGroup).or_else(|| within(0x00, l.mono_groups).map(Target::MonoGroup)),
            2 => within(0x40, l.stereo_auxes).map(Target::StereoAux).or_else(|| within(0x00, l.mono_auxes).map(Target::MonoAux)),
            3 => within(0x40, l.stereo_matrices).map(Target::StereoMatrix).or_else(|| within(0x00, l.mono_matrices).map(Target::MonoMatrix)),
            4 => within(l.mute_group_note, l.mute_groups)
                .map(Target::MuteGroup)
                .or_else(|| within(0x36, l.dcas).map(Target::Dca))
                .or_else(|| within(0x30, l.mains).map(Target::Main))
                .or_else(|| within(0x20, l.fx_returns).map(Target::FxReturn))
                .or_else(|| within(0x10, l.stereo_fx_sends).map(Target::StereoFxSend))
                .or_else(|| within(0x00, l.mono_fx_sends).map(Target::MonoFxSend)),
            _ => None,
        }
    }

    /// The model id this target maps to. Mono and stereo mixes of the same
    /// kind share one numbering in the model (`bus:aux:1` may be mono aux 1
    /// or stereo aux 1 on the desk; `extra.dliveStereo` says which).
    pub fn model_id(self) -> String {
        match self {
            Target::Input(n) => ids::channel(n),
            Target::FxReturn(n) => ids::fx_return(n),
            Target::MonoGroup(n) | Target::StereoGroup(n) => ids::bus(BusKind::Group, n),
            Target::MonoAux(n) | Target::StereoAux(n) => ids::bus(BusKind::Aux, n),
            Target::MonoMatrix(n) | Target::StereoMatrix(n) => ids::bus(BusKind::Matrix, n),
            Target::MonoFxSend(n) | Target::StereoFxSend(n) => ids::bus(BusKind::FxSend, n),
            Target::Main(n) => ids::bus(BusKind::Main, n),
            Target::Dca(n) => ids::dca(n),
            Target::MuteGroup(n) => ids::mute_group(n),
        }
    }

    pub fn is_stereo(self) -> bool {
        matches!(self, Target::StereoGroup(_) | Target::StereoAux(_) | Target::StereoMatrix(_) | Target::StereoFxSend(_))
    }
}

/// Fader / send level: `LV` 0…127, −∞ at 0, +10 dB at 127, 64 dB across.
pub fn level_to_db(lv: u8) -> f64 {
    if lv == 0 {
        FADER_OFF
    } else {
        ((lv.min(127) as f64 / 127.0 * 64.0 - 54.0) * 10.0).round() / 10.0
    }
}

pub fn db_to_level(db: f64) -> u8 {
    if db <= FADER_OFF + 0.5 {
        0
    } else {
        (((db.clamp(-54.0, 10.0) + 54.0) / 64.0) * 127.0).round().clamp(1.0, 127.0) as u8
    }
}

/// Socket preamp gain (dLive): `GV` 0…127 spans +5…+60 dB.
pub fn gain_to_db(gv: u8) -> f64 {
    ((5.0 + gv.min(127) as f64 / 127.0 * 55.0) * 10.0).round() / 10.0
}

pub fn db_to_gain(db: f64) -> u8 {
    (((db.clamp(5.0, 60.0) - 5.0) / 55.0) * 127.0).round() as u8
}

/// The desk's colour numbering, shared by the MIDI protocol and the show file.
pub const COLORS: &[&str] = &["off", "red", "green", "yellow", "blue", "purple", "cyan", "white"];

pub fn color_name(code: u8) -> Option<&'static str> {
    COLORS.get(code as usize).copied()
}

pub fn color_code(name: &str) -> Option<u8> {
    COLORS.iter().position(|c| *c == name).map(|p| p as u8)
}

/// dLive MixRack socket numbers for preamp messages: MixRack 1–64 → `00–3F`,
/// DX 1/2 1–32 → `40–5F`, DX 3/4 1–32 → `60–7F`.
pub fn preamp_socket(unit: &str, index: u32) -> Option<u8> {
    let i = index.checked_sub(1)? as u8;
    match unit {
        "mixrack" if index <= 64 => Some(i),
        "dx12" if index <= 32 => Some(0x40 + i),
        "dx34" if index <= 32 => Some(0x60 + i),
        _ => None,
    }
}

pub fn preamp_socket_unit(mp: u8) -> (&'static str, u32) {
    match mp {
        0x00..=0x3F => ("mixrack", mp as u32 + 1),
        0x40..=0x5F => ("dx12", (mp - 0x40) as u32 + 1),
        _ => ("dx34", (mp & 0x1F) as u32 + 1),
    }
}

// ---------------------------------------------------------------- the archive

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not an Allen & Heath show archive (no Show/ directory inside the tar)")]
    NotAShow,
    #[error("the show has no live scene (StageBoxScene65535)")]
    NoLiveScene,
}

pub struct Entry {
    pub path: String,
    pub data: Vec<u8>,
}

/// Gunzip + untar.
pub fn open(bytes: &[u8]) -> Result<Vec<Entry>, ArchiveError> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut ar = tar::Archive::new(gz);
    let mut out = vec![];
    for e in ar.entries()? {
        let mut e = e?;
        if !e.header().entry_type().is_file() {
            continue;
        }
        let path = e.path()?.to_string_lossy().into_owned();
        let mut data = Vec::with_capacity(e.size() as usize);
        e.read_to_end(&mut data)?;
        out.push(Entry { path, data });
    }
    Ok(out)
}

pub fn is_show_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1F, 0x8B]) && open(bytes).map(|es| es.iter().any(|e| e.path.contains("Show/"))).unwrap_or(false)
}

/// One `Name Colour Manager` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBlock {
    /// The object class: `Input`, `Mono Aux`, `DCA`, …
    pub class: String,
    pub names: Vec<String>,
    pub colors: Vec<u8>,
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return vec![];
    }
    hay.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}

/// The object classes a scene's name blocks are seen to cover. The block
/// label is `<class> Channel Name Colour Manager`; the class is matched from
/// this list because the byte before it is the low half of the block length,
/// which is often a printable letter.
pub const NAME_CLASSES: &[&str] = &[
    "Input",
    "Mono Group",
    "Stereo Group",
    "Mono Aux",
    "Stereo Aux",
    "Mono FX Send",
    "Stereo FX Send",
    "Stereo Ultra FX Send",
    "Main",
    "Mono Matrix",
    "Stereo Matrix",
    "FX Return",
    "Ultra FX Return",
    "DCA",
    "Monitor",
];

/// Every `<class> Channel Name Colour Manager` block in a scene blob.
pub fn name_blocks(scene: &[u8]) -> Vec<NameBlock> {
    const TAIL: &[u8] = b" Channel Name Colour Manager";
    let mut out = vec![];
    for at in find_all(scene, TAIL) {
        let Some(class) = NAME_CLASSES.iter().copied().filter(|c| at >= c.len() && &scene[at - c.len()..at] == c.as_bytes()).max_by_key(|c| c.len()) else {
            continue;
        };
        let start = at - class.len();
        if start < 2 {
            continue;
        }
        let len = u16::from_be_bytes([scene[start - 2], scene[start - 1]]) as usize;
        let label_len = class.len() + TAIL.len() + 1; // including the NUL
        if len <= label_len + 1 {
            continue;
        }
        let payload_at = start + label_len + 1; // skip the version byte
        let n = (len - label_len - 1) / 10;
        let Some(payload) = scene.get(payload_at..payload_at + n * 10) else { continue };
        let names = (0..n)
            .map(|i| {
                let s = &payload[i * 9..i * 9 + 9];
                let end = s.iter().position(|&b| b == 0).unwrap_or(9);
                String::from_utf8_lossy(&s[..end]).trim().to_string()
            })
            .collect();
        let colors = payload[n * 9..n * 10].to_vec();
        out.push(NameBlock { class: class.to_string(), names, colors });
    }
    out
}

/// The input patch out of the `Channel Mapper` block: `(type code, 0-based
/// index)` per input channel, for the first `inputs` entries.
pub fn channel_mapper(scene: &[u8], inputs: usize) -> Option<Vec<(u8, u16)>> {
    const LABEL: &[u8] = b"Channel Mapper";
    let at = scene.windows(LABEL.len()).position(|w| w == LABEL)?;
    let base = at + LABEL.len() + 2;
    let mut out = vec![];
    for ch in 0..inputs {
        let o = base + ch * 3;
        let e = scene.get(o..o + 3)?;
        out.push((e[0], u16::from_be_bytes([e[1], e[2]])));
    }
    Some(out)
}

/// The name and description a numbered scene blob starts with (`01 01 name
/// \0\0 description \0`).
pub fn scene_title(blob: &[u8]) -> Option<(String, String)> {
    let b = blob.get(2..)?;
    let end = b.iter().position(|&c| c == 0)?;
    let name = String::from_utf8_lossy(&b[..end]).trim().to_string();
    if name.is_empty() || !name.chars().all(|c| !c.is_control()) {
        return None;
    }
    let rest = &b[end..];
    let skip = rest.iter().position(|&c| c != 0).unwrap_or(rest.len());
    let rest = &rest[skip..];
    let dend = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
    let desc = String::from_utf8_lossy(&rest[..dend]);
    let desc = if desc.chars().all(|c| !c.is_control()) { desc.trim().to_string() } else { String::new() };
    Some((name, desc))
}

/// Count objects of a class from the block labels (`", Input Channel 07"`).
pub fn count_objects(scene: &[u8], class: &str) -> u32 {
    let needle = format!(", {class} ");
    let mut highest = 0u32;
    for i in find_all(scene, needle.as_bytes()) {
        let tail = &scene[i + needle.len()..(i + needle.len() + 4).min(scene.len())];
        let digits: String = tail.iter().take_while(|b| b.is_ascii_digit()).map(|&b| b as char).collect();
        if let Ok(n) = digits.parse::<u32>() {
            highest = highest.max(n);
        }
    }
    highest
}

#[cfg(test)]
pub(crate) mod build {
    //! Synthesises show archives to the observed layout, for tests.
    use std::io::Write;

    pub fn name_block(class: &str, names: &[&str], colors: &[u8]) -> Vec<u8> {
        let label = format!("{class} Channel Name Colour Manager\0");
        let mut payload = vec![0x01u8];
        for n in names {
            let mut f = [0u8; 9];
            let b = n.as_bytes();
            f[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
            payload.extend_from_slice(&f);
        }
        payload.extend_from_slice(colors);
        let len = (label.len() + payload.len()) as u16;
        let mut out = len.to_be_bytes().to_vec();
        out.extend_from_slice(label.as_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn mapper(entries: &[(u8, u16)]) -> Vec<u8> {
        let mut out = b"Channel Mapper\0\x02".to_vec();
        for (t, i) in entries {
            out.push(*t);
            out.extend_from_slice(&i.to_be_bytes());
        }
        out
    }

    pub fn targz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut ar = tar::Builder::new(enc);
        for (name, body) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            ar.append_data(&mut h, name, *body).unwrap();
        }
        let enc = ar.into_inner().unwrap();
        let mut gz = enc.finish().unwrap();
        gz.flush().unwrap();
        gz
    }

    /// A scene blob with labelled blocks, a mapper and name blocks.
    pub fn scene_blob(title: Option<(&str, &str)>, inputs: u32, mapper_entries: &[(u8, u16)], blocks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![];
        if let Some((n, d)) = title {
            out.extend_from_slice(&[0x01, 0x01]);
            out.extend_from_slice(n.as_bytes());
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(d.as_bytes());
            out.push(0);
        }
        for i in 1..=inputs {
            out.extend_from_slice(format!("Parametric EQ, Input Channel {i:02}\0").as_bytes());
        }
        for i in 1..=2 {
            out.extend_from_slice(format!("Delay, Mono Aux Channel {i:02}\0").as_bytes());
        }
        for b in blocks {
            out.extend_from_slice(b);
        }
        out.extend_from_slice(&mapper(mapper_entries));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_addresses_follow_the_published_tables() {
        assert_eq!(Target::Input(1).address(Family::Dlive), Some((0, 0x00)));
        assert_eq!(Target::Input(128).address(Family::Dlive), Some((0, 0x7F)));
        assert_eq!(Target::Input(97).address(Family::Avantis), None);
        assert_eq!(Target::StereoGroup(1).address(Family::Dlive), Some((1, 0x40)));
        assert_eq!(Target::MonoAux(62).address(Family::Dlive), Some((2, 0x3D)));
        assert_eq!(Target::StereoMatrix(27).address(Family::Avantis), Some((3, 0x5A)));
        assert_eq!(Target::StereoFxSend(1).address(Family::Dlive), Some((4, 0x10)));
        assert_eq!(Target::FxReturn(16).address(Family::Dlive), Some((4, 0x2F)));
        assert_eq!(Target::Main(6).address(Family::Dlive), Some((4, 0x35)));
        assert_eq!(Target::Main(3).address(Family::Avantis), Some((4, 0x32)));
        assert_eq!(Target::Dca(24).address(Family::Dlive), Some((4, 0x4D)));
        assert_eq!(Target::Dca(16).address(Family::Avantis), Some((4, 0x45)));
        assert_eq!(Target::MuteGroup(1).address(Family::Dlive), Some((4, 0x4E)));
        assert_eq!(Target::MuteGroup(8).address(Family::Avantis), Some((4, 0x4D)));
        for t in [Target::Input(5), Target::StereoAux(3), Target::MonoMatrix(9), Target::Dca(2), Target::MuteGroup(8), Target::FxReturn(1), Target::Main(2)] {
            for f in [Family::Dlive, Family::Avantis] {
                let (o, n) = t.address(f).unwrap();
                assert_eq!(Target::from_address(f, o, n), Some(t), "{t:?} on {f:?}");
            }
        }
    }

    #[test]
    fn laws_match_the_document_tables() {
        assert_eq!(level_to_db(0x7F), 10.0);
        assert_eq!(level_to_db(0x6B), -0.1);
        assert_eq!(level_to_db(0x1B), -40.4);
        assert_eq!(level_to_db(0), FADER_OFF);
        assert_eq!(db_to_level(0.0), 0x6B);
        assert_eq!(db_to_level(-40.0), 0x1C);
        assert_eq!(db_to_level(10.0), 0x7F);
        assert_eq!(db_to_level(FADER_OFF), 0);
        assert_eq!(gain_to_db(0x7F), 60.0);
        assert_eq!(gain_to_db(0x00), 5.0);
        assert_eq!(db_to_gain(45.0), 0x5C);
        assert_eq!(color_name(6), Some("cyan"));
        assert_eq!(color_code("white"), Some(7));
        assert_eq!(preamp_socket("dx12", 1), Some(0x40));
        assert_eq!(preamp_socket_unit(0x61), ("dx34", 2));
    }

    #[test]
    fn reads_names_colours_patch_and_titles_from_a_synthetic_show() {
        let inputs = build::name_block("Input", &["Kick", "Snare", "Vox Lead", ""], &[1, 2, 7, 0]);
        let auxes = build::name_block("Mono Aux", &["Wedge 1", "Wedge 2"], &[6, 6]);
        let live = build::scene_blob(None, 4, &[(0x03, 0), (0x03, 1), (0x00, 4), (0x25, 0)], &[inputs, auxes]);
        let s1 = build::scene_blob(Some(("Opening", "Band walks on")), 4, &[], &[]);
        let show = build::targz(&[
            ("Show/InputConfig/InputConfig.dat", b"1\n0\n"),
            ("Show/Scenes/StageBoxScene001.tar.gz", &build::targz(&[("StageBoxScene001.dat", &s1)])),
            ("Show/Scenes/StageBoxScene65535.tar.gz", &build::targz(&[("StageBoxScene65535.dat", &live)])),
        ]);
        assert!(is_show_archive(&show));
        let entries = open(&show).unwrap();
        let live_e = entries.iter().find(|e| e.path.contains("65535")).unwrap();
        let inner = open(&live_e.data).unwrap();
        let blob = &inner[0].data;
        let blocks = name_blocks(blob);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].class, "Input");
        assert_eq!(blocks[0].names, vec!["Kick", "Snare", "Vox Lead", ""]);
        assert_eq!(blocks[0].colors, vec![1, 2, 7, 0]);
        assert_eq!(blocks[1].class, "Mono Aux");
        assert_eq!(channel_mapper(blob, 4), Some(vec![(3, 0), (3, 1), (0, 4), (0x25, 0)]));
        assert_eq!(count_objects(blob, "Input Channel"), 4);
        assert_eq!(count_objects(blob, "Mono Aux Channel"), 2);
        let s1_e = entries.iter().find(|e| e.path.contains("Scene001")).unwrap();
        let s1_blob = &open(&s1_e.data).unwrap()[0].data;
        assert_eq!(scene_title(s1_blob), Some(("Opening".into(), "Band walks on".into())));
    }
}

//! Allen & Heath SQ-5 / SQ-6 / SQ-7.
//!
//! Two halves: the **show files** (`NVDATA.DAT` and `SCENEnnn.DAT`, flat
//! 128 KiB NVRAM images that a show folder or a USB `AHSQ/Shows/<name>/`
//! directory holds) and the **MIDI protocol** address tables (SQ MIDI Protocol
//! Issue 5, firmware V1.5.0+), which the live driver in [`crate::live`] uses.
//!
//! # What the files give up
//!
//! Only the input patch and the scene names are decoded. The patch was
//! located by controlled diff (move Ip3 from Local 3 to Local 10 in MixPad,
//! compare): one byte per input channel in a 336-byte record, found by the
//! `ff ff ff [patch] 00 ?? fe` signature and taken from the longest run of
//! records one stride apart. The byte is a socket *number* with no class —
//! only a Local patch has ever been observed — so the socket is labelled as an
//! input socket rather than asserted to be Local. Channel names have not been
//! found in the image (MixPad offline cannot rename a channel, so the diff
//! that would locate them has not been possible); a show saved from a real SQ
//! with named channels would settle it. The live driver reads everything the
//! protocol exposes instead: mutes, levels, pans and assignments.
//!
//! # The NRPN address space
//!
//! Every parameter is a 14-bit number, `MSB × 128 + LSB`, and the tables in
//! the protocol document are arithmetic once read that way: the same source
//! index selects a mute at `0x00:idx`, a fader to LR at `0x40:idx`, and the
//! aux, FX and matrix sends sit in per-source blocks of 12, 4 and 3. Pan is
//! the level address plus `0x10:00` and an assignment is the level address
//! plus `0x20:00`. Everything here was checked against the worked examples in
//! the document; it has not yet been checked against a desk.

use songbook_model::{ids, BusKind, Direction, SocketKind, FADER_OFF};

use crate::midi::param14;

/// Every SQ NVRAM image is exactly this size.
pub const IMAGE_LEN: usize = 131_072;
/// Bytes between one input channel's record and the next.
const CHANNEL_STRIDE: usize = 336;
/// Input channels on every SQ.
pub const INPUT_CHANNELS: usize = 48;
/// The input-channel records at the head of the 122-record strip table
/// (Ip1–Ip48; the stereo inputs, FX returns and the rest follow at the same
/// stride with other class codes).
const PATCH_RECORDS: usize = INPUT_CHANNELS;
/// The scene name field at 0x14 is sixteen bytes: MixPad rewrote a longer
/// name as its first sixteen characters and zeros (2026-09-20).
pub const SCENE_NAME_LEN: usize = 16;

/// What a MixPad or console image is, from its first byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Nvdata,
    Scene,
    Unknown,
}

pub fn image_kind(d: &[u8]) -> ImageKind {
    if d.len() != IMAGE_LEN || d[1] != 0x00 || d[2] != 0xFE || !d[3..12].iter().all(|&b| b == 0xFF) || d[0x0C..0x10] != [0x01, 0x06, 0x00, 0x01] {
        return ImageKind::Unknown;
    }
    match d[0] {
        0xB5 => ImageKind::Nvdata,
        0xA1 => ImageKind::Scene,
        _ => ImageKind::Unknown,
    }
}

/// The scene name a `SCENEnnn.DAT` carries at 0x14 (sixteen bytes).
pub fn scene_name(d: &[u8]) -> String {
    let s = &d[0x14..(0x14 + SCENE_NAME_LEN).min(d.len())];
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).trim().to_string()
}

/// `SCENEnnn.DAT` is scene nnn + 1: MixPad lists `SCENE001.DAT` as scene 2
/// and names a fresh store there "Scene 2" (2026-09-20).
pub fn scene_number_of_file(file_index: u32) -> u32 {
    file_index + 1
}
pub fn scene_file_name(scene_number: u32) -> String {
    format!("SCENE{:03}.DAT", scene_number.saturating_sub(1))
}

/// A patched socket as an image spells it: a class byte and a 0-based index.
///
/// The class is byte +2 of the record and follows MixPad's I/O Patch tab
/// order — 0 unpatched, 1 Local, 2 SLink, 3 USB, 4 I/O Port. Seen in the
/// default SQ-7 show: Ip1–32 → Local 1–32, Ip41–46 → Local 49–54 (the
/// three stereo TRS pairs, which the family numbers after 48 XLR slots),
/// Ip47/48 → USB 1/2. SLink and I/O Port are the tab order, not yet seen
/// in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatchSocket {
    pub class: u8,
    /// 1-based within the class.
    pub index: u32,
}

pub const CLASS_LOCAL: u8 = 1;
pub const CLASS_SLINK: u8 = 2;
pub const CLASS_USB: u8 = 3;
pub const CLASS_IO_PORT: u8 = 4;
/// The Local indices MixPad's default show gives the stereo TRS inputs.
pub const LOCAL_STEREO_FIRST: u32 = 49;
pub const LOCAL_STEREO_LAST: u32 = 54;

impl PatchSocket {
    /// The unit the socket belongs to, as the model names it.
    pub fn unit(&self) -> String {
        match self.class {
            CLASS_LOCAL => "local".into(),
            CLASS_SLINK => "slink".into(),
            CLASS_USB => "usb".into(),
            CLASS_IO_PORT => "ioport".into(),
            k => format!("class{k}"),
        }
    }
    pub fn id(&self) -> String {
        ids::socket(&self.unit(), Direction::In, self.index)
    }
    pub fn label(&self) -> String {
        match (self.class, self.index) {
            (CLASS_LOCAL, n @ LOCAL_STEREO_FIRST..=LOCAL_STEREO_LAST) => format!("ST{} {}", (n - LOCAL_STEREO_FIRST) / 2 + 1, if (n - LOCAL_STEREO_FIRST).is_multiple_of(2) { "L" } else { "R" }),
            (CLASS_LOCAL, n) => format!("Local {n}"),
            (CLASS_SLINK, n) => format!("SLink {n}"),
            (CLASS_USB, n) => format!("USB {n}"),
            (CLASS_IO_PORT, n) => format!("I/O Port {n}"),
            (k, n) => format!("Class {k} socket {n}"),
        }
    }
    pub fn kind(&self) -> SocketKind {
        match self.class {
            CLASS_LOCAL if self.index >= LOCAL_STEREO_FIRST => SocketKind::Line,
            CLASS_LOCAL => SocketKind::Mic,
            CLASS_SLINK => SocketKind::SLink,
            CLASS_USB => SocketKind::Usb,
            CLASS_IO_PORT => SocketKind::Card,
            _ => SocketKind::Other,
        }
    }
    /// The class an ID's unit stands for, when it is one an image can hold.
    pub fn from_unit(unit: &str, index: u32) -> Option<PatchSocket> {
        let class = match unit.trim_start_matches("unit:") {
            "local" => CLASS_LOCAL,
            "slink" => CLASS_SLINK,
            "usb" => CLASS_USB,
            "ioport" => CLASS_IO_PORT,
            u => u.strip_prefix("class")?.parse().ok()?,
        };
        (1..=256).contains(&index).then_some(PatchSocket { class, index })
    }
}

/// One input channel's patch, from an NVDATA or scene image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputPatchEntry {
    /// 1-based input channel.
    pub channel: u32,
    /// The socket, when the channel is patched.
    pub socket: Option<PatchSocket>,
}

/// Offsets of every input channel's patch byte: the longest run of records
/// one stride apart, then only the leading forty.
fn patch_offsets(d: &[u8]) -> Vec<usize> {
    let mut hits = Vec::new();
    for i in 3..d.len().saturating_sub(4) {
        if d[i - 3..i] == [0xFF, 0xFF, 0xFF] && d[i + 1] == 0x00 && d[i + 3] == 0xFE {
            hits.push(i);
        }
    }
    let mut best: Vec<usize> = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for &h in &hits {
        match run.last() {
            Some(&prev) if h - prev == CHANNEL_STRIDE => run.push(h),
            _ => {
                if run.len() > best.len() {
                    best = std::mem::take(&mut run);
                } else {
                    run.clear();
                }
                run.push(h);
            }
        }
    }
    if run.len() > best.len() {
        best = run;
    }
    best.truncate(PATCH_RECORDS);
    best
}

/// Read the input patch out of an NVDATA or scene image (both carry the
/// same strip table; a scene stores the patch it was saved with).
pub fn input_patch(d: &[u8]) -> Vec<InputPatchEntry> {
    patch_offsets(d)
        .iter()
        .enumerate()
        .map(|(i, &off)| {
            let class = d.get(off + 2).copied().unwrap_or(0);
            let socket = (class != 0).then(|| PatchSocket { class, index: d[off] as u32 + 1 });
            InputPatchEntry { channel: i as u32 + 1, socket }
        })
        .collect()
}

/// `input_patch`, by its older name.
pub fn nvdata_patch(d: &[u8]) -> Vec<InputPatchEntry> {
    input_patch(d)
}

// ---------------------------------------------------------------- the desk

/// SQ model facts the driver and the capability table share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqModel {
    pub name: &'static str,
    pub local_inputs: u32,
    pub local_outputs: u32,
    pub soft_keys: u32,
}

pub const MODELS: &[SqModel] = &[
    SqModel { name: "SQ-5", local_inputs: 16, local_outputs: 12, soft_keys: 8 },
    SqModel { name: "SQ-6", local_inputs: 24, local_outputs: 14, soft_keys: 16 },
    SqModel { name: "SQ-7", local_inputs: 32, local_outputs: 16, soft_keys: 16 },
];

pub fn model(name: &str) -> SqModel {
    let n = name.to_uppercase().replace(' ', "");
    MODELS.iter().copied().find(|m| n.contains(&m.name.replace('-', "")) || n.contains(m.name)).unwrap_or(MODELS[0])
}

pub const AUXES: u32 = 12;
pub const GROUPS: u32 = 12;
pub const FX_SENDS: u32 = 4;
pub const FX_RETURNS: u32 = 8;
pub const MATRICES: u32 = 3;
pub const DCAS: u32 = 8;
pub const MUTE_GROUPS: u32 = 8;
pub const SCENES: u32 = 300;

/// A source strip in the protocol's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    Input(u32),
    Group(u32),
    FxReturn(u32),
    Lr,
    Aux(u32),
    FxSend(u32),
    Matrix(u32),
    Dca(u32),
    MuteGroup(u32),
}

impl Strip {
    /// The index shared by the mute and "to LR" tables.
    fn index(self) -> Option<u16> {
        Some(match self {
            Strip::Input(n) => n as u16 - 1,
            Strip::Group(n) => 0x30 + n as u16 - 1,
            Strip::FxReturn(n) => 0x3C + n as u16 - 1,
            Strip::Lr => 0x44,
            Strip::Aux(n) => 0x45 + n as u16 - 1,
            Strip::FxSend(n) => 0x51 + n as u16 - 1,
            Strip::Matrix(n) => 0x55 + n as u16 - 1,
            Strip::Dca(_) | Strip::MuteGroup(_) => return None,
        })
    }

    /// The model's bus id for a mix strip.
    pub fn bus_id(self) -> Option<String> {
        Some(match self {
            Strip::Lr => ids::bus(BusKind::Main, 1),
            Strip::Aux(n) => ids::bus(BusKind::Aux, n),
            Strip::Group(n) => ids::bus(BusKind::Group, n),
            Strip::FxSend(n) => ids::bus(BusKind::FxSend, n),
            Strip::Matrix(n) => ids::bus(BusKind::Matrix, n),
            _ => return None,
        })
    }

    pub fn label(self) -> String {
        match self {
            Strip::Input(n) => format!("Ip{n}"),
            Strip::Group(n) => format!("Grp{n}"),
            Strip::FxReturn(n) => format!("FX{n}Rtn"),
            Strip::Lr => "LR".into(),
            Strip::Aux(n) => format!("Aux{n}"),
            Strip::FxSend(n) => format!("FX{n}Snd"),
            Strip::Matrix(n) => format!("Mtx{n}"),
            Strip::Dca(n) => format!("DCA{n}"),
            Strip::MuteGroup(n) => format!("MGrp{n}"),
        }
    }
}

/// The mute parameter of a strip.
pub fn mute_param(s: Strip) -> u16 {
    match s {
        Strip::Dca(n) => param14(0x02, n as u8 - 1),
        Strip::MuteGroup(n) => param14(0x04, n as u8 - 1),
        other => other.index().unwrap_or(0),
    }
}

/// The level parameter of a send from `from` into `to`, or of a master fader
/// when `to` is `None`. `None` when the protocol has no such send.
pub fn level_param(from: Strip, to: Option<Strip>) -> Option<u16> {
    let base = |m: u8, l: u8| param14(m, l);
    Some(match (from, to) {
        // Masters.
        (Strip::Lr, None) => base(0x4F, 0x00),
        (Strip::Aux(n), None) => base(0x4F, 0x00) + n as u16,
        (Strip::FxSend(n), None) => base(0x4F, 0x0C) + n as u16,
        (Strip::Matrix(n), None) => base(0x4F, 0x10) + n as u16,
        (Strip::Dca(n), None) => base(0x4F, 0x1F) + n as u16,
        (Strip::Input(_) | Strip::Group(_) | Strip::FxReturn(_) | Strip::MuteGroup(_), None) => return None,
        // To LR: the channel fader.
        (s, Some(Strip::Lr)) => match s {
            Strip::Input(_) | Strip::Group(_) | Strip::FxReturn(_) => base(0x40, 0x00) + s.index()?,
            _ => return None,
        },
        (Strip::Input(n), Some(Strip::Aux(a))) => base(0x40, 0x44) + (n as u16 - 1) * 12 + (a as u16 - 1),
        (Strip::Group(n), Some(Strip::Aux(a))) => base(0x45, 0x04) + (n as u16 - 1) * 12 + (a as u16 - 1),
        (Strip::FxReturn(n), Some(Strip::Aux(a))) => base(0x46, 0x14) + (n as u16 - 1) * 12 + (a as u16 - 1),
        (Strip::Input(n), Some(Strip::FxSend(f))) => base(0x4C, 0x14) + (n as u16 - 1) * 4 + (f as u16 - 1),
        (Strip::Group(n), Some(Strip::FxSend(f))) => base(0x4D, 0x54) + (n as u16 - 1) * 4 + (f as u16 - 1),
        (Strip::FxReturn(n), Some(Strip::FxSend(f))) => base(0x4E, 0x04) + (n as u16 - 1) * 4 + (f as u16 - 1),
        (Strip::Lr, Some(Strip::Matrix(m))) => base(0x4E, 0x24) + (m as u16 - 1),
        (Strip::Aux(n), Some(Strip::Matrix(m))) => base(0x4E, 0x27) + (n as u16 - 1) * 3 + (m as u16 - 1),
        (Strip::Group(n), Some(Strip::Matrix(m))) => base(0x4E, 0x4B) + (n as u16 - 1) * 3 + (m as u16 - 1),
        _ => return None,
    })
}

/// Pan/balance parameter: the level address plus `0x10:00`. Masters have a
/// balance too (LR, Aux, Mtx), DCAs and FX sends do not.
pub fn pan_param(from: Strip, to: Option<Strip>) -> Option<u16> {
    if matches!((from, to), (Strip::Dca(_) | Strip::FxSend(_), None)) {
        return None;
    }
    level_param(from, to).map(|p| p + param14(0x10, 0x00))
}

/// Mix assignment parameter: the level address plus `0x20:00`.
pub fn assign_param(from: Strip, to: Strip) -> Option<u16> {
    level_param(from, Some(to)).map(|p| p + param14(0x20, 0x00))
}

/// The protocol document's linear-taper table, `(dB, 14-bit value)`, in
/// order. The desk resolves to 0.1 dB between these; the law is a straight
/// line of 118.7 units per dB from −89 dB up, with 0 meaning −∞.
const LINEAR: &[(f64, u16)] = &[
    (-89.0, 4630), (-85.0, 5105), (-80.0, 5698), (-75.0, 6292), (-70.0, 6885), (-65.0, 7479), (-60.0, 8073), (-55.0, 8666),
    (-50.0, 9260), (-45.0, 9853), (-40.0, 10447), (-38.0, 10684), (-36.0, 10922), (-35.0, 11041), (-34.0, 11159), (-33.0, 11278),
    (-32.0, 11397), (-31.0, 11516), (-30.0, 11634), (-29.0, 11753), (-28.0, 11872), (-27.0, 11990), (-26.0, 12109), (-25.0, 12228),
    (-24.0, 12347), (-23.0, 12465), (-22.0, 12584), (-21.0, 12703), (-20.0, 12822), (-19.0, 12940), (-18.0, 13059), (-17.0, 13178),
    (-16.0, 13296), (-15.0, 13415), (-14.0, 13534), (-13.0, 13653), (-12.0, 13771), (-11.0, 13890), (-10.0, 14009), (-9.0, 14127),
    (-8.0, 14246), (-7.0, 14365), (-6.0, 14484), (-5.0, 14602), (-4.0, 14721), (-3.0, 14840), (-2.0, 14959), (-1.0, 15077),
    (0.0, 15196), (1.0, 15315), (2.0, 15433), (3.0, 15552), (4.0, 15671), (5.0, 15790), (6.0, 15908), (7.0, 16027), (8.0, 16146),
    (9.0, 16264), (10.0, 16383),
];

/// dB → 14-bit linear-taper level.
pub fn db_to_level(db: f64) -> u16 {
    if db <= FADER_OFF + 0.5 || db < -89.0 {
        return 0;
    }
    let db = db.min(10.0);
    for w in LINEAR.windows(2) {
        let (d0, v0) = w[0];
        let (d1, v1) = w[1];
        if db <= d1 {
            let t = (db - d0) / (d1 - d0);
            return (v0 as f64 + t * (v1 as f64 - v0 as f64)).round() as u16;
        }
    }
    16383
}

/// 14-bit linear-taper level → dB.
pub fn level_to_db(v: u16) -> f64 {
    if v == 0 {
        return FADER_OFF;
    }
    let v = v.min(16383);
    if v <= LINEAR[0].1 {
        return LINEAR[0].0;
    }
    for w in LINEAR.windows(2) {
        let (d0, v0) = w[0];
        let (d1, v1) = w[1];
        if v <= v1 {
            let t = (v as f64 - v0 as f64) / (v1 as f64 - v0 as f64);
            return ((d0 + t * (d1 - d0)) * 10.0).round() / 10.0;
        }
    }
    10.0
}

/// −1 … +1 → the 14-bit pan value (`00 00` full left, `3F 7F` centre, `7F 7F` full right).
pub fn pan_to_value(pan: f64) -> u16 {
    ((pan.clamp(-1.0, 1.0) + 1.0) / 2.0 * 16383.0).floor() as u16
}

pub fn value_to_pan(v: u16) -> f64 {
    ((v.min(16383) as f64 / 16383.0) * 2.0 - 1.0).clamp(-1.0, 1.0)
}

/// A Local socket's ID, the common case.
pub fn socket_id(n: u32) -> String {
    PatchSocket { class: CLASS_LOCAL, index: n }.id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_addresses_match_the_document() {
        // Ip1 to LR 40 00; Ip40 to Aux5 44 1C; Grp4 to Aux8 45 2F; Ip36 to FX3 4D 22.
        assert_eq!(level_param(Strip::Input(1), Some(Strip::Lr)), Some(param14(0x40, 0x00)));
        assert_eq!(level_param(Strip::Input(40), Some(Strip::Aux(5))), Some(param14(0x44, 0x1C)));
        assert_eq!(level_param(Strip::Group(4), Some(Strip::Aux(8))), Some(param14(0x45, 0x2F)));
        assert_eq!(level_param(Strip::Input(36), Some(Strip::FxSend(3))), Some(param14(0x4D, 0x22)));
        // Ip6 to Aux1 crosses the 7-bit LSB boundary: 41 00.
        assert_eq!(level_param(Strip::Input(6), Some(Strip::Aux(1))), Some(param14(0x41, 0x00)));
        // FX2Rtn to Aux3 46 22; Grp5 to LR 40 34.
        assert_eq!(level_param(Strip::FxReturn(2), Some(Strip::Aux(3))), Some(param14(0x46, 0x22)));
        assert_eq!(level_param(Strip::Group(5), Some(Strip::Lr)), Some(param14(0x40, 0x34)));
        // Aux5 to Mtx1 4E 33; Aux7 to Mtx1 4E 39; LR to Mtx3 4E 26.
        assert_eq!(level_param(Strip::Aux(5), Some(Strip::Matrix(1))), Some(param14(0x4E, 0x33)));
        assert_eq!(level_param(Strip::Aux(7), Some(Strip::Matrix(1))), Some(param14(0x4E, 0x39)));
        assert_eq!(level_param(Strip::Lr, Some(Strip::Matrix(3))), Some(param14(0x4E, 0x26)));
        // Masters: LR 4F 00, Aux12 4F 0C, FX1Snd 4F 0D, Mtx3 4F 13, DCA8 4F 27.
        assert_eq!(level_param(Strip::Lr, None), Some(param14(0x4F, 0x00)));
        assert_eq!(level_param(Strip::Aux(12), None), Some(param14(0x4F, 0x0C)));
        assert_eq!(level_param(Strip::FxSend(1), None), Some(param14(0x4F, 0x0D)));
        assert_eq!(level_param(Strip::Matrix(3), None), Some(param14(0x4F, 0x13)));
        assert_eq!(level_param(Strip::Dca(8), None), Some(param14(0x4F, 0x27)));
        assert_eq!(level_param(Strip::Input(1), None), None);
    }

    #[test]
    fn pan_mute_and_assign_addresses_match_the_document() {
        // Ip24 to Aux5 pan 52 5C; Grp3 to Aux2 55 1D; LR to Mtx3 5E 26; Ip30 to Aux5 pan 53 24.
        assert_eq!(pan_param(Strip::Input(24), Some(Strip::Aux(5))), Some(param14(0x52, 0x5C)));
        assert_eq!(pan_param(Strip::Group(3), Some(Strip::Aux(2))), Some(param14(0x55, 0x1D)));
        assert_eq!(pan_param(Strip::Lr, Some(Strip::Matrix(3))), Some(param14(0x5E, 0x26)));
        assert_eq!(pan_param(Strip::Input(30), Some(Strip::Aux(5))), Some(param14(0x53, 0x24)));
        // Mutes: Ip1 00 00; LR 00 44; Mute Grp 4 04 03; DCA1 02 00; Mtx3 00 57.
        assert_eq!(mute_param(Strip::Input(1)), 0);
        assert_eq!(mute_param(Strip::Lr), param14(0x00, 0x44));
        assert_eq!(mute_param(Strip::MuteGroup(4)), param14(0x04, 0x03));
        assert_eq!(mute_param(Strip::Dca(1)), param14(0x02, 0x00));
        assert_eq!(mute_param(Strip::Matrix(3)), param14(0x00, 0x57));
        // Assign: Ip1 to LR 60 00; FX1Rtn to Aux7 66 1A; Grp2 to Mtx2 6E 4F.
        assert_eq!(assign_param(Strip::Input(1), Strip::Lr), Some(param14(0x60, 0x00)));
        assert_eq!(assign_param(Strip::FxReturn(1), Strip::Aux(7)), Some(param14(0x66, 0x1A)));
        assert_eq!(assign_param(Strip::Group(2), Strip::Matrix(2)), Some(param14(0x6E, 0x4F)));
    }

    #[test]
    fn fader_law_round_trips_the_document_examples() {
        // 0 dB = 76 5C; -20 dB = 64 16; -12 dB = 6B 4B; -24 dB = 60 3B.
        assert_eq!(db_to_level(0.0), param14(0x76, 0x5C));
        assert_eq!(db_to_level(-20.0), param14(0x64, 0x16));
        assert_eq!(db_to_level(-12.0), param14(0x6B, 0x4B));
        assert_eq!(db_to_level(-24.0), param14(0x60, 0x3B));
        assert_eq!(db_to_level(FADER_OFF), 0);
        assert_eq!(level_to_db(0), FADER_OFF);
        assert_eq!(level_to_db(param14(0x76, 0x5C)), 0.0);
        assert_eq!(level_to_db(param14(0x64, 0x16)), -20.0);
        assert_eq!(level_to_db(16383), 10.0);
        for db in [-60.0, -31.0, -3.5, 4.2] {
            assert!((level_to_db(db_to_level(db)) - db).abs() < 0.11, "{db}");
        }
    }

    #[test]
    fn pan_law() {
        assert_eq!(pan_to_value(0.0), param14(0x3F, 0x7F));
        assert_eq!(pan_to_value(-1.0), 0);
        assert_eq!(pan_to_value(1.0), 16383);
        assert!((value_to_pan(param14(0x1F, 0x7F)) + 0.5).abs() < 0.01);
        assert!((value_to_pan(param14(0x5F, 0x7F)) - 0.5).abs() < 0.01);
    }

    /// A synthetic NVDATA image with the record signature at a 336-byte stride.
    fn image(kind: u8, patches: &[(u8, bool)]) -> Vec<u8> {
        let mut d = vec![0u8; IMAGE_LEN];
        d[0] = kind;
        d[1] = 0x00;
        d[2] = 0xFE;
        for b in &mut d[3..12] {
            *b = 0xFF;
        }
        d[0x0C..0x10].copy_from_slice(&[0x01, 0x06, 0x00, 0x01]);
        for (i, &(socket, patched)) in patches.iter().enumerate() {
            let at = 0x38C + i * CHANNEL_STRIDE;
            d[at - 3..at].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
            d[at] = socket;
            d[at + 1] = 0x00;
            d[at + 2] = u8::from(patched);
            d[at + 3] = 0xFE;
        }
        d
    }

    #[test]
    fn reads_the_patch_including_unpatched_channels() {
        let mut p: Vec<(u8, bool)> = (0..48).map(|i| (i as u8, true)).collect();
        p[2] = (9, true);
        p[39] = (39, false);
        let d = image(0xB5, &p);
        assert_eq!(image_kind(&d), ImageKind::Nvdata);
        let patch = input_patch(&d);
        assert_eq!(patch.len(), 48);
        assert_eq!(patch[2], InputPatchEntry { channel: 3, socket: Some(PatchSocket { class: CLASS_LOCAL, index: 10 }) });
        assert_eq!(patch[39], InputPatchEntry { channel: 40, socket: None });
        let mut s = image(0xA1, &[]);
        s[0x14..0x1B].copy_from_slice(b"Scene 2");
        assert_eq!(image_kind(&s), ImageKind::Scene);
        assert_eq!(scene_name(&s), "Scene 2");
        assert_eq!(image_kind(b"short"), ImageKind::Unknown);
    }

    #[test]
    fn model_lookup() {
        assert_eq!(model("SQ-7").name, "SQ-7");
        assert_eq!(model("sq6").name, "SQ-6");
        assert_eq!(model("").name, "SQ-5");
    }
}

// ---------------------------------------------------------------- writing

/// The image checksum: CRC-32 (the zlib one — reflected `0xEDB88320`, init
/// and xor-out `0xFFFFFFFF`) over bytes `0x14..0x1fffc`, stored little-endian
/// in the last four bytes. Identified on 2026-09-20 by matching two
/// independent MixPad images (an NVDATA and a scene); the 20-byte header
/// before `0x14` is outside it.
pub const CRC_START: usize = 0x14;

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// The checksum an image should carry.
pub fn image_checksum(d: &[u8]) -> Option<u32> {
    if d.len() != IMAGE_LEN {
        return None;
    }
    Some(crc32(&d[CRC_START..IMAGE_LEN - 4]))
}

/// Whether the stored checksum matches the contents.
pub fn image_checksum_ok(d: &[u8]) -> bool {
    match image_checksum(d) {
        Some(c) => u32::from_le_bytes([d[IMAGE_LEN - 4], d[IMAGE_LEN - 3], d[IMAGE_LEN - 2], d[IMAGE_LEN - 1]]) == c,
        None => false,
    }
}

/// Recompute and store the checksum.
pub fn fix_image_checksum(d: &mut [u8]) {
    if let Some(c) = image_checksum(d) {
        d[IMAGE_LEN - 4..].copy_from_slice(&c.to_le_bytes());
    }
}

/// Write an input patch into an NVDATA or scene image (`None` leaves the
/// channel unpatched: class 0, index 0, as MixPad writes it). Channels the
/// image has no record for are skipped. Returns the channels written.
pub fn write_patch(d: &mut [u8], patch: &[InputPatchEntry]) -> Result<usize, String> {
    if image_kind(d) == ImageKind::Unknown {
        return Err("not an SQ image".into());
    }
    let offsets = patch_offsets(d);
    let mut written = 0;
    for p in patch {
        let Some(&off) = offsets.get(p.channel as usize - 1) else { continue };
        match p.socket {
            Some(s) => {
                if s.index == 0 || s.index > 256 {
                    return Err(format!("Ip{}: socket index {} is out of range", p.channel, s.index));
                }
                d[off] = (s.index - 1) as u8;
                d[off + 2] = s.class;
            }
            None => {
                d[off] = 0x00;
                d[off + 2] = 0x00;
            }
        }
        written += 1;
    }
    fix_image_checksum(d);
    Ok(written)
}

/// `write_patch`, by its older name.
pub fn write_nvdata_patch(d: &mut [u8], patch: &[InputPatchEntry]) -> Result<usize, String> {
    write_patch(d, patch)
}

/// Write a scene's name (ASCII, up to `SCENE_NAME_LEN` characters) into a
/// scene image.
pub fn write_scene_name(d: &mut [u8], name: &str) -> Result<(), String> {
    if image_kind(d) != ImageKind::Scene {
        return Err("not an SQ scene image".into());
    }
    let mut field = [0u8; SCENE_NAME_LEN];
    for (i, c) in name.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(SCENE_NAME_LEN).enumerate() {
        field[i] = c as u8;
    }
    d[0x14..0x14 + SCENE_NAME_LEN].copy_from_slice(&field);
    fix_image_checksum(d);
    Ok(())
}

#[cfg(test)]
mod write_tests {
    use super::*;

    fn image(kind: u8) -> Vec<u8> {
        let mut d = vec![0u8; IMAGE_LEN];
        d[0] = kind;
        d[2] = 0xFE;
        for b in &mut d[3..12] {
            *b = 0xFF;
        }
        d[0x0C..0x10].copy_from_slice(&[0x01, 0x06, 0x00, 0x01]);
        // The default SQ-7 layout: Ip1–32 Local, 33–40 unpatched, 41–46 the
        // stereo TRS pairs (Local 49–54), 47/48 USB 1/2.
        for i in 0..48usize {
            let at = 0x38C + i * CHANNEL_STRIDE;
            d[at - 3..at].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
            let (idx, class) = match i {
                0..=31 => (i as u8, CLASS_LOCAL),
                32..=39 => (0, 0),
                40..=45 => (48 + (i - 40) as u8, CLASS_LOCAL),
                _ => ((i - 46) as u8, CLASS_USB),
            };
            d[at] = idx;
            d[at + 2] = class;
            d[at + 3] = 0xFE;
        }
        fix_image_checksum(&mut d);
        d
    }

    #[test]
    fn reads_the_default_show_layout() {
        let p = input_patch(&image(0xB5));
        assert_eq!(p.len(), 48);
        assert_eq!(p[0].socket, Some(PatchSocket { class: CLASS_LOCAL, index: 1 }));
        assert_eq!(p[32].socket, None);
        assert_eq!(p[40].socket.unwrap().label(), "ST1 L");
        assert_eq!(p[45].socket.unwrap().label(), "ST3 R");
        assert_eq!(p[46].socket.unwrap().label(), "USB 1");
        assert_eq!(p[46].socket.unwrap().id(), "skt:usb:in:1");
        assert_eq!(p[47].socket.unwrap().id(), "skt:usb:in:2");
        assert_eq!(PatchSocket::from_unit("unit:usb", 2), p[47].socket);
        assert_eq!(scene_number_of_file(1), 2);
        assert_eq!(scene_file_name(2), "SCENE001.DAT");
    }

    #[test]
    fn crc_is_the_zlib_one() {
        // Known answer: CRC-32 of "123456789" is 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let d = image(0xB5);
        assert!(image_checksum_ok(&d));
        let mut bad = d.clone();
        bad[0x1000] ^= 1;
        assert!(!image_checksum_ok(&bad));
        // The header is outside the checksum.
        let mut hdr = d.clone();
        hdr[0x02] ^= 1;
        assert!(image_checksum_ok(&hdr));
    }

    #[test]
    fn writes_patch_and_scene_name_and_refreshes_the_checksum() {
        let mut d = image(0xB5);
        let local = |n| Some(PatchSocket { class: CLASS_LOCAL, index: n });
        let n = write_patch(&mut d, &[InputPatchEntry { channel: 3, socket: local(10) }, InputPatchEntry { channel: 1, socket: None }, InputPatchEntry { channel: 48, socket: Some(PatchSocket { class: CLASS_SLINK, index: 7 }) }]).unwrap();
        assert_eq!(n, 3);
        assert!(image_checksum_ok(&d));
        let p = input_patch(&d);
        assert_eq!(p[2].socket, local(10));
        assert_eq!(p[0].socket, None);
        assert_eq!(d[0x38C], 0, "unpatched writes index 0 like MixPad");
        assert_eq!(p[1].socket, local(2));
        assert_eq!(p[47].socket.unwrap().id(), "skt:slink:in:7");
        let mut s = image(0xA1);
        write_scene_name(&mut s, "Songbook test scene name that is long").unwrap();
        assert!(image_checksum_ok(&s));
        assert_eq!(scene_name(&s), "Songbook test sc");
        assert_eq!(&s[0x24..0x34], &[0u8; 16], "bytes after the 16-byte field are untouched");
    }

    /// MixPad's own images, when present: their stored checksum must be the
    /// one this code computes.
    #[test]
    fn real_mixpad_images_if_present() {
        let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/sq7")];
        let ah = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/Allen & Heath");
        if let Ok(rd) = std::fs::read_dir(&ah) {
            dirs.extend(rd.flatten().map(|e| e.path().join("CurrentShow")));
        }
        let mut checked = 0;
        for show in dirs {
            let Ok(files) = std::fs::read_dir(&show) else { continue };
            for f in files.flatten() {
                let Ok(d) = std::fs::read(f.path()) else { continue };
                if image_kind(&d) != ImageKind::Unknown {
                    assert!(image_checksum_ok(&d), "{}", f.path().display());
                    checked += 1;
                }
            }
        }
        assert!(checked >= 2, "the fixtures under fixtures/sq7 should have been checked");
        eprintln!("checked {checked} MixPad images");
    }

    /// The MixPad fixture reads as the default SQ-7 show it is.
    #[test]
    fn mixpad_fixture_reads_the_default_layout() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/sq7");
        let nv = std::fs::read(dir.join("NVDATA.DAT")).expect("fixtures/sq7/NVDATA.DAT");
        let p = input_patch(&nv);
        assert_eq!(p.len(), 48);
        assert_eq!(p[2].socket, Some(PatchSocket { class: CLASS_LOCAL, index: 10 }));
        assert_eq!(p[32].socket, None);
        assert_eq!(p[40].socket.unwrap().label(), "ST1 L");
        assert_eq!(p[46].socket.unwrap().label(), "USB 1");
        let sc = std::fs::read(dir.join("SCENE001.DAT")).expect("fixtures/sq7/SCENE001.DAT");
        assert_eq!(scene_name(&sc), "Scene 2");
        // The scene was stored before Ip3 moved to Local 10: it keeps Local 3.
        assert_eq!(input_patch(&sc)[2].socket, Some(PatchSocket { class: CLASS_LOCAL, index: 3 }), "the scene stores the patch it was saved with");
    }
}

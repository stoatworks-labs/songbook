//! What each desk can hold. Figures are from the vendors' spec sheets and
//! the protocol dictionaries (the Yamaha SCP `prminfo` tables and the A&H
//! MIDI protocol channel tables state the counts directly); `notes` carries
//! the caveats a single number hides.

use serde::{Deserialize, Serialize};
use songbook_model::Platform;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub platform: Platform,
    pub model: String,
    /// Mono input channels (a stereo pair costs two on desks that count that way).
    pub input_channels: u32,
    pub stereo_inputs: u32,
    pub fx_returns: u32,
    /// Local mic/line sockets on the desk / mix rack.
    pub local_inputs: u32,
    pub local_outputs: u32,
    /// Mix buses that can be auxes.
    pub auxes: u32,
    /// Buses that can be subgroups (0 where groups come out of the aux pool).
    pub groups: u32,
    /// Auxes and groups share one pool of this size (0 = separate).
    pub mix_pool: u32,
    pub matrices: u32,
    pub fx_sends: u32,
    pub mains: u32,
    pub dcas: u32,
    pub mute_groups: u32,
    pub scene_slots: u32,
    /// Characters a strip name can hold.
    pub name_length: u32,
    /// The desk's colour palette, in the model's spellings.
    pub colors: Vec<String>,
    pub eq_bands: u32,
    /// Whether a channel's head amp can be read and written from software.
    pub preamp_control: bool,
    pub cues: bool,
    pub notes: Vec<String>,
}

pub fn models(platform: Platform) -> Vec<&'static str> {
    match platform {
        Platform::AhSq => vec!["SQ-5", "SQ-6", "SQ-7"],
        Platform::AhDlive => vec!["dLive S3000", "dLive S5000", "dLive S7000", "dLive C1500", "dLive C2500", "dLive C3500", "dLive DM0"],
        Platform::AhAvantis => vec!["Avantis", "Avantis Solo"],
        Platform::AhQu => vec!["Qu-5", "Qu-6", "Qu-7", "Qu-16", "Qu-24", "Qu-32"],
        Platform::AhCq => vec!["CQ-12T", "CQ-18T", "CQ-20B"],
        Platform::YamahaClQl => vec!["QL1", "QL5", "CL1", "CL3", "CL5"],
        Platform::YamahaTf => vec!["TF1", "TF3", "TF5", "TF-Rack"],
        Platform::YamahaDm3 => vec!["DM3", "DM3S"],
        Platform::YamahaDm7 => vec!["DM7", "DM7 Compact"],
        Platform::YamahaRivage => vec!["PM3", "PM5", "PM7", "PM10"],
        Platform::Generic => vec!["Generic"],
    }
}

fn ah_colors() -> Vec<String> {
    ["off", "red", "green", "yellow", "blue", "purple", "cyan", "white"].iter().map(|s| s.to_string()).collect()
}

fn yamaha_colors() -> Vec<String> {
    ["off", "red", "green", "yellow", "blue", "purple", "cyan", "orange", "pink"].iter().map(|s| s.to_string()).collect()
}

pub fn capabilities(platform: Platform, model: &str) -> Capabilities {
    let m = model.to_uppercase().replace([' ', '-'], "");
    let base = Capabilities {
        platform,
        model: model.to_string(),
        input_channels: 32,
        stereo_inputs: 0,
        fx_returns: 0,
        local_inputs: 16,
        local_outputs: 8,
        auxes: 8,
        groups: 0,
        mix_pool: 0,
        matrices: 0,
        fx_sends: 0,
        mains: 1,
        dcas: 0,
        mute_groups: 0,
        scene_slots: 100,
        name_length: 8,
        colors: vec![],
        eq_bands: 4,
        preamp_control: false,
        cues: false,
        notes: vec![],
    };
    match platform {
        Platform::AhSq => {
            let (li, lo) = if m.contains("SQ7") {
                (32, 16)
            } else if m.contains("SQ6") {
                (24, 14)
            } else {
                (16, 12)
            };
            Capabilities {
                input_channels: 48,
                stereo_inputs: 3,
                fx_returns: 8,
                local_inputs: li,
                local_outputs: lo,
                auxes: 12,
                groups: 12,
                mix_pool: 12,
                matrices: 3,
                fx_sends: 4,
                mains: 1,
                dcas: 8,
                mute_groups: 8,
                scene_slots: 300,
                name_length: 8,
                colors: ah_colors(),
                eq_bands: 4,
                preamp_control: false,
                cues: false,
                notes: vec![
                    "12 mixes shared between auxes and groups; each mix is mono or stereo".into(),
                    "the SQ MIDI protocol carries mutes, levels, pans and assignments but no names, colours or preamps".into(),
                ],
                ..base
            }
        }
        Platform::AhDlive => {
            // A DM0 MixRack on its own has no surface sockets; every surface has 8/8.
            let (li, lo, chans) = if m.contains("DM0") { (0, 0, 128) } else { (8, 8, 128) };
            Capabilities {
                input_channels: chans,
                stereo_inputs: 0,
                fx_returns: 16,
                local_inputs: li,
                local_outputs: lo,
                auxes: 62,
                groups: 62,
                mix_pool: 64,
                matrices: 62,
                fx_sends: 16,
                mains: 6,
                dcas: 24,
                mute_groups: 8,
                scene_slots: 500,
                name_length: 8,
                colors: ah_colors(),
                eq_bands: 4,
                preamp_control: true,
                cues: true,
                notes: vec![
                    "64 configurable mix buses across groups, auxes, matrices and mains (mono or stereo); the counts here are the protocol's maxima, not a fixed allocation".into(),
                    "the surface's local sockets are few; the MixRack and DX expanders hold the preamps".into(),
                ],
                ..base
            }
        }
        Platform::AhAvantis => Capabilities {
            input_channels: 64,
            stereo_inputs: 0,
            fx_returns: 12,
            local_inputs: 12,
            local_outputs: 12,
            auxes: 54,
            groups: 54,
            mix_pool: 42,
            matrices: 54,
            fx_sends: 12,
            mains: 3,
            dcas: 16,
            mute_groups: 8,
            scene_slots: 500,
            name_length: 8,
            colors: ah_colors(),
            eq_bands: 4,
            preamp_control: false,
            cues: true,
            notes: vec!["42 configurable mix buses across groups, auxes, matrices and mains; the per-kind counts are the protocol's address ranges".into(), "the Avantis MIDI protocol has no preamp messages".into()],
            ..base
        },
        Platform::AhQu => {
            let classic = m.contains("QU16") || m.contains("QU24") || m.contains("QU32");
            let (li, lo, ch) = if m.contains("QU7") || m.contains("QU32") {
                (32, 16, if classic { 32 } else { 48 })
            } else if m.contains("QU6") || m.contains("QU24") {
                (24, 12, if classic { 24 } else { 48 })
            } else {
                (16, 8, if classic { 16 } else { 48 })
            };
            Capabilities {
                input_channels: ch,
                stereo_inputs: 3,
                fx_returns: if classic { 4 } else { 8 },
                local_inputs: li,
                local_outputs: lo,
                auxes: if classic { 10 } else { 12 },
                groups: if classic { 4 } else { 12 },
                mix_pool: if classic { 0 } else { 12 },
                matrices: if classic { 2 } else { 3 },
                fx_sends: 4,
                mains: 1,
                dcas: if classic { 4 } else { 8 },
                mute_groups: if classic { 4 } else { 8 },
                scene_slots: if classic { 100 } else { 300 },
                name_length: 8,
                colors: ah_colors(),
                eq_bands: 4,
                preamp_control: false,
                cues: false,
                notes: vec!["the 2025 Qu-5/6/7 share the SQ's protocol shape; the classic Qu-16/24/32 have a different table (descriptor only here)".into()],
                ..base
            }
        }
        Platform::AhCq => {
            let (li, ch) = if m.contains("CQ20") {
                (20, 20)
            } else if m.contains("CQ18") {
                (18, 18)
            } else {
                (12, 12)
            };
            Capabilities {
                input_channels: ch,
                stereo_inputs: 0,
                fx_returns: 4,
                local_inputs: li,
                local_outputs: if m.contains("CQ12") { 6 } else { 8 },
                auxes: if m.contains("CQ12") { 6 } else { 8 },
                groups: 0,
                mix_pool: 0,
                matrices: 0,
                fx_sends: 4,
                mains: 1,
                dcas: 0,
                mute_groups: 0,
                scene_slots: 100,
                name_length: 8,
                colors: ah_colors(),
                eq_bands: 4,
                preamp_control: false,
                cues: false,
                notes: vec!["descriptor only: no CQ file or protocol driver yet".into()],
                ..base
            }
        }
        Platform::YamahaClQl => {
            let (ch, li, lo) = if m.contains("CL5") {
                (72, 8, 8)
            } else if m.contains("CL3") {
                (64, 8, 8)
            } else if m.contains("CL1") {
                (48, 8, 8)
            } else if m.contains("QL5") {
                (64, 32, 16)
            } else {
                (32, 16, 8)
            };
            Capabilities {
                input_channels: ch,
                stereo_inputs: 8,
                fx_returns: 0,
                local_inputs: li,
                local_outputs: lo,
                auxes: 24,
                groups: 0,
                mix_pool: 0,
                matrices: 8,
                fx_sends: 0,
                mains: 1,
                dcas: 16,
                mute_groups: 8,
                scene_slots: 300,
                name_length: 8,
                colors: yamaha_colors(),
                eq_bands: 4,
                preamp_control: true,
                cues: false,
                notes: vec!["24 MIX buses (VARI/FIXED) and 8 MATRIX; groups are MIX buses in FIXED mode; FX are rack inserts, not sends".into(), "CL and QL expose Dante 64×64 in addition to the local sockets".into()],
                ..base
            }
        }
        Platform::YamahaTf => {
            // TF-Rack has the TF1's I/O; every TF has 40 input channels.
            let (ch, li, lo) = if m.contains("TF5") {
                (40, 32, 16)
            } else if m.contains("TF3") {
                (40, 24, 16)
            } else {
                (40, 16, 16)
            };
            Capabilities {
                input_channels: ch,
                stereo_inputs: 2,
                fx_returns: 2,
                local_inputs: li,
                local_outputs: lo,
                auxes: 20,
                groups: 8,
                mix_pool: 0,
                matrices: 4,
                fx_sends: 2,
                mains: 1,
                dcas: 8,
                mute_groups: 6,
                scene_slots: 200,
                name_length: 8,
                colors: yamaha_colors(),
                eq_bands: 4,
                preamp_control: true,
                cues: false,
                notes: vec!["20 AUX (8 mono + 6 stereo pairs) plus 8 groups and a SUB bus".into()],
                ..base
            }
        }
        Platform::YamahaDm3 => Capabilities {
            input_channels: 16,
            stereo_inputs: 2,
            fx_returns: 4,
            local_inputs: 16,
            local_outputs: 8,
            auxes: 6,
            groups: 0,
            mix_pool: 0,
            matrices: 2,
            fx_sends: 2,
            mains: 1,
            dcas: 0,
            mute_groups: 6,
            scene_slots: 200,
            name_length: 8,
            colors: yamaha_colors(),
            eq_bands: 4,
            preamp_control: true,
            cues: false,
            notes: vec!["16 mono inputs + 1 stereo line pair; 6 MIX + 2 MATRIX + 2 FX; 100 scenes in each of banks A and B".into(), "head-amp gain is global, not part of a scene".into()],
            ..base
        },
        Platform::YamahaDm7 => Capabilities {
            input_channels: 120,
            stereo_inputs: 0,
            fx_returns: 0,
            local_inputs: if m.contains("COMPACT") { 16 } else { 32 },
            local_outputs: 16,
            auxes: 48,
            groups: 0,
            mix_pool: 0,
            matrices: 12,
            fx_sends: 0,
            mains: 1,
            dcas: 24,
            mute_groups: 12,
            scene_slots: 500,
            name_length: 8,
            colors: yamaha_colors(),
            eq_bands: 4,
            preamp_control: true,
            cues: false,
            notes: vec!["48 MIX, 12 MATRIX, 24 DCA, 12 mute groups; scenes are numbered 1.00–500.99".into()],
            ..base
        },
        Platform::YamahaRivage => Capabilities {
            input_channels: if m.contains("PM3") { 120 } else if m.contains("PM5") { 144 } else { 288 },
            stereo_inputs: 0,
            fx_returns: 0,
            local_inputs: 0,
            local_outputs: 0,
            auxes: if m.contains("PM3") { 48 } else { 60 },
            groups: 0,
            mix_pool: 0,
            matrices: if m.contains("PM3") { 24 } else { 36 },
            fx_sends: 0,
            mains: 2,
            dcas: 24,
            mute_groups: 12,
            scene_slots: 500,
            name_length: 8,
            colors: yamaha_colors(),
            eq_bands: 4,
            preamp_control: true,
            cues: false,
            notes: vec!["all I/O lives on RPio racks over TWINLANe/Dante; the surface has no audio sockets of its own".into(), "scenes are numbered 1.00–999.99 with a text argument on the wire".into()],
            ..base
        },
        Platform::Generic => Capabilities { input_channels: 128, stereo_inputs: 16, fx_returns: 16, auxes: 64, groups: 64, matrices: 64, fx_sends: 16, mains: 8, dcas: 24, mute_groups: 12, scene_slots: 1000, name_length: 32, colors: songbook_model::COLORS.iter().map(|s| s.to_string()).collect(), preamp_control: true, cues: true, ..base },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_has_models_and_a_table() {
        for p in Platform::all() {
            let ms = models(*p);
            assert!(!ms.is_empty(), "{p:?}");
            for m in ms {
                let c = capabilities(*p, m);
                assert!(c.input_channels > 0, "{p:?} {m}");
                assert!(c.scene_slots > 0);
                assert!(c.name_length >= 8);
            }
        }
        assert_eq!(capabilities(Platform::AhSq, "SQ-7").local_inputs, 32);
        assert_eq!(capabilities(Platform::YamahaClQl, "CL5").input_channels, 72);
        assert_eq!(capabilities(Platform::YamahaDm3, "DM3").auxes, 6);
    }
}

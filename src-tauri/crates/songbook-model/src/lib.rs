//! The canonical mixing desk show model.
//!
//! Every platform driver (`songbook-ah`, `songbook-yamaha`, …) reads a vendor
//! show file or a live desk into a [`Show`], and writes one back out.
//! Everything above the drivers — the inspector, the documentation, the
//! conversion, the history — only ever sees this model. That is what makes
//! conversion possible: a show is a set of sockets, channels, buses, DCAs,
//! mute groups, scenes and cues, and the vendor spellings are pushed down into
//! `extra` bags so nothing is lost on a round trip but nothing vendor-specific
//! leaks up.
//!
//! Four rules the model keeps:
//!
//! - **IDs are stable strings**, scoped by kind (`ch:…`, `bus:aux:…`, `dca:…`,
//!   `scene:…`, `skt:…`) and chosen by the driver so that re-importing the
//!   same vendor file yields the same IDs and the history diff stays readable.
//! - **Levels are dB, pans are −1…+1, frequencies are Hz.** A vendor's own
//!   units (an SQ NRPN fader value, a Yamaha centi-dB integer) are converted by
//!   the driver. Fader off is [`FADER_OFF`], because JSON has no −∞.
//! - **Preamps belong to the socket, not the channel.** Gain, pad and phantom
//!   live on the physical connector; on a shared stage box every desk on it
//!   shares them. A channel names its socket, and the socket carries the preamp.
//! - **`extra` is a bag, not a model.** Anything a driver cannot express in the
//!   shared fields goes in `extra` keyed by the vendor's own name. Conversion
//!   never reads `extra`; the same driver can write it back.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod diff;
pub mod ids;
pub mod summary;

/// Schema tag written into every show file. Bump when a field changes meaning.
pub const SCHEMA: &str = "songbook/1";

/// The dB value that stands for a fader or send at −∞. JSON cannot carry an
/// infinity, and no desk resolves below −90 dB, so anything at or below this
/// is "off".
pub const FADER_OFF: f64 = -144.0;

pub type Extra = Map<String, Value>;

fn extra_is_empty(e: &Extra) -> bool {
    e.is_empty()
}

fn yes() -> bool {
    true
}

/// The platform a show was authored for: the family that decides which driver
/// reads and writes it. The specific desk is `System::model`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    /// Allen & Heath SQ-5 / SQ-6 / SQ-7.
    AhSq,
    /// Allen & Heath dLive (S-Class and C-Class surfaces, DM / CDM MixRacks).
    AhDlive,
    /// Allen & Heath Avantis / Avantis Solo.
    AhAvantis,
    /// Allen & Heath Qu-5 / Qu-6 / Qu-7 (and the classic Qu-16/24/32).
    AhQu,
    /// Allen & Heath CQ-12T / CQ-18T / CQ-20B.
    AhCq,
    /// Yamaha CL1 / CL3 / CL5 and QL1 / QL5.
    YamahaClQl,
    /// Yamaha TF1 / TF3 / TF5 / TF-Rack.
    YamahaTf,
    /// Yamaha DM3 / DM3S.
    YamahaDm3,
    /// Yamaha DM7 / DM7 Compact.
    YamahaDm7,
    /// Yamaha RIVAGE PM3 / PM5 / PM7 / PM10.
    YamahaRivage,
    /// No vendor: a show authored from scratch in Songbook.
    Generic,
}

impl Platform {
    pub fn label(self) -> &'static str {
        match self {
            Platform::AhSq => "Allen & Heath SQ",
            Platform::AhDlive => "Allen & Heath dLive",
            Platform::AhAvantis => "Allen & Heath Avantis",
            Platform::AhQu => "Allen & Heath Qu",
            Platform::AhCq => "Allen & Heath CQ",
            Platform::YamahaClQl => "Yamaha CL / QL",
            Platform::YamahaTf => "Yamaha TF",
            Platform::YamahaDm3 => "Yamaha DM3",
            Platform::YamahaDm7 => "Yamaha DM7",
            Platform::YamahaRivage => "Yamaha RIVAGE PM",
            Platform::Generic => "Generic",
        }
    }

    pub fn vendor(self) -> &'static str {
        match self {
            Platform::AhSq | Platform::AhDlive | Platform::AhAvantis | Platform::AhQu | Platform::AhCq => "Allen & Heath",
            Platform::YamahaClQl | Platform::YamahaTf | Platform::YamahaDm3 | Platform::YamahaDm7 | Platform::YamahaRivage => "Yamaha",
            Platform::Generic => "",
        }
    }

    pub fn all() -> &'static [Platform] {
        &[
            Platform::AhSq,
            Platform::AhDlive,
            Platform::AhAvantis,
            Platform::AhQu,
            Platform::AhCq,
            Platform::YamahaClQl,
            Platform::YamahaTf,
            Platform::YamahaDm3,
            Platform::YamahaDm7,
            Platform::YamahaRivage,
            Platform::Generic,
        ]
    }

    pub fn is_ah(self) -> bool {
        matches!(self, Platform::AhSq | Platform::AhDlive | Platform::AhAvantis | Platform::AhQu | Platform::AhCq)
    }

    pub fn is_yamaha(self) -> bool {
        matches!(
            self,
            Platform::YamahaClQl | Platform::YamahaTf | Platform::YamahaDm3 | Platform::YamahaDm7 | Platform::YamahaRivage
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub schema: String,
    pub id: String,
    pub meta: Meta,
    pub platform: Platform,
    pub system: System,
    /// Physical connectors on every unit: local XLRs, stage box sockets,
    /// Dante / SLink / USB streams.
    #[serde(default)]
    pub sockets: Vec<Socket>,
    /// Head amp state per input socket, where the socket has one.
    #[serde(default)]
    pub preamps: Vec<Preamp>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub buses: Vec<Bus>,
    #[serde(default)]
    pub dcas: Vec<Dca>,
    #[serde(default)]
    pub mute_groups: Vec<MuteGroup>,
    /// What each output socket carries.
    #[serde(default)]
    pub output_patch: Vec<OutputPatch>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub cues: Vec<Cue>,
    /// Vendor files kept beside the model — an SQ `NVDATA.DAT`, a dLive show
    /// archive, a `.CLF`, a `.dm3s` — so a show can go back to its own
    /// hardware byte-exact.
    #[serde(default)]
    pub vendor: Vec<VendorBlob>,
    /// What an import or a conversion could not express faithfully.
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl Show {
    pub fn new(name: &str, platform: Platform) -> Show {
        Show {
            schema: SCHEMA.to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            meta: Meta {
                name: name.to_string(),
                notes: String::new(),
                tags: vec![],
                created: now(),
                modified: now(),
                author: None,
                source: None,
            },
            platform,
            system: System::default(),
            sockets: vec![],
            preamps: vec![],
            channels: vec![],
            buses: vec![],
            dcas: vec![],
            mute_groups: vec![],
            output_patch: vec![],
            scenes: vec![],
            cues: vec![],
            vendor: vec![],
            notes: vec![],
        }
    }

    pub fn touch(&mut self) {
        self.meta.modified = now();
    }

    pub fn socket(&self, id: &str) -> Option<&Socket> {
        self.sockets.iter().find(|x| x.id == id)
    }
    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|x| x.id == id)
    }
    pub fn bus(&self, id: &str) -> Option<&Bus> {
        self.buses.iter().find(|x| x.id == id)
    }
    pub fn dca(&self, id: &str) -> Option<&Dca> {
        self.dcas.iter().find(|x| x.id == id)
    }
    pub fn mute_group(&self, id: &str) -> Option<&MuteGroup> {
        self.mute_groups.iter().find(|x| x.id == id)
    }
    pub fn scene(&self, id: &str) -> Option<&Scene> {
        self.scenes.iter().find(|x| x.id == id)
    }
    pub fn preamp(&self, socket_id: &str) -> Option<&Preamp> {
        self.preamps.iter().find(|p| p.socket_id == socket_id)
    }
    pub fn preamp_mut(&mut self, socket_id: &str) -> Option<&mut Preamp> {
        self.preamps.iter_mut().find(|p| p.socket_id == socket_id)
    }
    pub fn unit(&self, id: &str) -> Option<&Unit> {
        self.system.units.iter().find(|u| u.id == id)
    }

    pub fn buses_of(&self, kind: BusKind) -> impl Iterator<Item = &Bus> {
        self.buses.iter().filter(move |b| b.kind == kind)
    }

    /// Add a note, keeping the list free of exact duplicates.
    pub fn note(&mut self, level: NoteLevel, path: impl Into<String>, message: impl Into<String>) {
        let n = Note { level, path: path.into(), message: message.into() };
        if !self.notes.contains(&n) {
            self.notes.push(n);
        }
    }

    /// Validate the reference graph: every ID a field points at must exist.
    /// Returns human-readable problems; an empty list is a consistent show.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = vec![];
        for s in &self.sockets {
            if self.unit(&s.unit_id).is_none() {
                problems.push(format!("socket {} belongs to missing unit {}", s.id, s.unit_id));
            }
        }
        for p in &self.preamps {
            if self.socket(&p.socket_id).is_none() {
                problems.push(format!("preamp on missing socket {}", p.socket_id));
            }
        }
        let check_strip = |owner: &str, sends: &[Send], dcas: &[String], mgs: &[String], problems: &mut Vec<String>| {
            for s in sends {
                if self.bus(&s.bus_id).is_none() {
                    problems.push(format!("{owner} sends to missing bus {}", s.bus_id));
                }
            }
            for d in dcas {
                if self.dca(d).is_none() {
                    problems.push(format!("{owner} is in missing DCA {d}"));
                }
            }
            for m in mgs {
                if self.mute_group(m).is_none() {
                    problems.push(format!("{owner} is in missing mute group {m}"));
                }
            }
        };
        for c in &self.channels {
            if let Some(src) = &c.source {
                if self.socket(src).is_none() {
                    problems.push(format!("channel {} is patched from missing socket {}", c.id, src));
                }
            }
            check_strip(&c.id, &c.sends, &c.dca_ids, &c.mute_group_ids, &mut problems);
        }
        for b in &self.buses {
            check_strip(&b.id, &b.sends, &b.dca_ids, &b.mute_group_ids, &mut problems);
        }
        for o in &self.output_patch {
            if self.socket(&o.socket_id).is_none() {
                problems.push(format!("output patch names missing socket {}", o.socket_id));
            }
            if let Some(r) = &o.source.ref_id {
                let ok = match o.source.kind {
                    OutputSourceKind::Bus => self.bus(r).is_some(),
                    OutputSourceKind::Channel | OutputSourceKind::DirectOut => self.channel(r).is_some(),
                    OutputSourceKind::Socket => self.socket(r).is_some(),
                    OutputSourceKind::Other => true,
                };
                if !ok {
                    problems.push(format!("output {} carries missing {:?} {}", o.socket_id, o.source.kind, r));
                }
            }
        }
        for c in &self.cues {
            for s in &c.steps {
                if let Some(sc) = &s.scene_id {
                    if self.scene(sc).is_none() {
                        problems.push(format!("cue {} recalls missing scene {}", c.id, sc));
                    }
                }
            }
        }
        problems
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created: String,
    pub modified: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Where the model came from, when it was imported or captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// `file` or `device`.
    pub kind: String,
    /// File name, or host address.
    pub origin: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct System {
    /// Desk model as the vendor names it: "SQ-5", "dLive S5000", "QL5", "DM3".
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub firmware: String,
    /// The desk's own name.
    #[serde(default)]
    pub name: String,
    /// Sample rate, Hz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    /// `internal`, `word-clock`, `dante`, `slink` — as the vendor reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_source: Option<String>,
    /// The boxes: the console/surface, its mix rack, expanders and stage
    /// boxes, cards and network streams. Every socket belongs to one.
    #[serde(default)]
    pub units: Vec<Unit>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum UnitRole {
    /// A console with its own I/O (SQ, Qu, CL, TF, DM3).
    Console,
    /// A control surface with no or little audio I/O (dLive S-Class, Rivage CS).
    Surface,
    /// The box the DSP and the local sockets live in (dLive MixRack, Rivage DSP engine).
    MixRack,
    /// A stage box or expander (DX, AB, GX, DT, Rio, Tio).
    StageBox,
    /// An option card (I/O port, slot, Dante card).
    Card,
    /// A network audio stream seen as a set of sockets (Dante, SLink, gigaACE, AES50).
    Network,
    /// USB audio, the internal player/recorder, talkback, the signal generator.
    Internal,
}

/// One box or stream that owns sockets.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    /// `unit:local`, `unit:slink`, `unit:dante`, `unit:dx1`, `unit:slot1`.
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub model: String,
    pub role: UnitRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum SocketKind {
    /// A mic/line XLR with a head amp.
    Mic,
    /// A line-level analogue jack or XLR without a head amp.
    Line,
    Aes,
    Dante,
    /// Allen & Heath SLink / gigaACE / dSnake / ME.
    SLink,
    Usb,
    Madi,
    /// A slot / option card channel whose transport is not known.
    Card,
    /// Internal: player, signal generator, talkback, FX.
    Internal,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    In,
    Out,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Socket {
    /// `skt:<unit>:<in|out>:<n>`, e.g. `skt:local:in:3`.
    pub id: String,
    pub unit_id: String,
    pub kind: SocketKind,
    pub direction: Direction,
    /// Socket number on the unit, 1-based, as printed on the panel.
    pub index: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

/// Head amp state for one input socket.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preamp {
    pub socket_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phantom: Option<bool>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelKind {
    /// A mono or stereo input channel.
    Input,
    /// A stereo line input (Yamaha ST IN, SQ ST1–3).
    StereoInput,
    /// An FX return.
    FxReturn,
}

/// Fader, on, pan and the processing on one strip. Every field is optional
/// because every driver knows a different subset; `None` is "not read", never
/// "off".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Strip {
    /// Fader in dB; [`FADER_OFF`] for −∞.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fader_db: Option<f64>,
    /// Channel on (the inverse of mute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    /// −1 full left … +1 full right; balance on a stereo strip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
    /// Assigned to the main mix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_assign: Option<bool>,
    /// Polarity inverted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity: Option<bool>,
    /// Digital trim after the head amp, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hpf: Option<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lpf: Option<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<Eq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<Dynamics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp: Option<Dynamics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_on: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freq_hz: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum BandKind {
    LowShelf,
    Bell,
    HighShelf,
    Notch,
    LowPass,
    HighPass,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub kind: BandKind,
    pub freq_hz: f64,
    pub gain_db: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<f64>,
    #[serde(default = "yes")]
    pub on: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Eq {
    pub on: bool,
    #[serde(default)]
    pub bands: Vec<EqBand>,
}

/// A gate or a compressor, in the terms every desk shares.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dynamics {
    pub on: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_ms: Option<f64>,
    /// Gate range / compressor make-up, dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knee: Option<String>,
}

/// A send from a channel (or a bus) into a bus.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Send {
    pub bus_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    /// Pre-fader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// `ch:<n>`, `st:<n>`, `fxr:<n>`.
    pub id: String,
    pub number: u32,
    pub kind: ChannelKind,
    pub label: String,
    /// One of [`COLORS`], or `None` when the desk has no colour for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub stereo: bool,
    /// The socket this channel is patched from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The right-hand socket of a stereo channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_right: Option<String>,
    #[serde(default)]
    pub strip: Strip,
    #[serde(default)]
    pub sends: Vec<Send>,
    #[serde(default)]
    pub dca_ids: Vec<String>,
    #[serde(default)]
    pub mute_group_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

impl Channel {
    pub fn new(id: String, number: u32, kind: ChannelKind, label: &str) -> Channel {
        Channel {
            id,
            number,
            kind,
            label: label.to_string(),
            color: None,
            stereo: false,
            source: None,
            source_right: None,
            strip: Strip::default(),
            sends: vec![],
            dca_ids: vec![],
            mute_group_ids: vec![],
            extra: Extra::new(),
        }
    }

    pub fn send_mut(&mut self, bus_id: &str) -> &mut Send {
        if let Some(i) = self.sends.iter().position(|s| s.bus_id == bus_id) {
            &mut self.sends[i]
        } else {
            self.sends.push(Send { bus_id: bus_id.to_string(), level_db: None, on: None, pre: None, pan: None });
            self.sends.last_mut().unwrap()
        }
    }
}

/// The colour names every desk's palette is mapped onto. Vendor palettes are
/// close to this set (the dLive MIDI colour table is exactly it); a desk with
/// more colours keeps its own name in `extra`.
pub const COLORS: &[&str] = &["off", "red", "green", "yellow", "blue", "purple", "cyan", "white", "orange", "pink"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum BusKind {
    /// The main mix: LR, Stereo, Mains.
    Main,
    /// A mix / aux bus.
    Aux,
    /// A subgroup.
    Group,
    /// A matrix.
    Matrix,
    /// An FX send bus.
    FxSend,
    /// Mono / centre / a monitor bus.
    Other,
}

impl BusKind {
    pub fn label(self) -> &'static str {
        match self {
            BusKind::Main => "Main",
            BusKind::Aux => "Aux",
            BusKind::Group => "Group",
            BusKind::Matrix => "Matrix",
            BusKind::FxSend => "FX send",
            BusKind::Other => "Other",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bus {
    /// `bus:main:1`, `bus:aux:3`, `bus:grp:2`, `bus:mtx:1`, `bus:fx:1`.
    pub id: String,
    pub number: u32,
    pub kind: BusKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub stereo: bool,
    #[serde(default)]
    pub strip: Strip,
    /// Sends out of this bus (aux → matrix, group → aux).
    #[serde(default)]
    pub sends: Vec<Send>,
    #[serde(default)]
    pub dca_ids: Vec<String>,
    #[serde(default)]
    pub mute_group_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

impl Bus {
    pub fn new(kind: BusKind, number: u32, label: &str, stereo: bool) -> Bus {
        Bus {
            id: ids::bus(kind, number),
            number,
            kind,
            label: label.to_string(),
            color: None,
            stereo,
            strip: Strip::default(),
            sends: vec![],
            dca_ids: vec![],
            mute_group_ids: vec![],
            extra: Extra::new(),
        }
    }

    pub fn send_mut(&mut self, bus_id: &str) -> &mut Send {
        if let Some(i) = self.sends.iter().position(|s| s.bus_id == bus_id) {
            &mut self.sends[i]
        } else {
            self.sends.push(Send { bus_id: bus_id.to_string(), level_db: None, on: None, pre: None, pan: None });
            self.sends.last_mut().unwrap()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dca {
    pub id: String,
    pub number: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fader_db: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MuteGroup {
    pub id: String,
    pub number: u32,
    pub label: String,
    /// Mute group active (its members muted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum OutputSourceKind {
    Bus,
    Channel,
    DirectOut,
    /// A straight socket-to-socket route (port-to-port).
    Socket,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputSource {
    pub kind: OutputSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    /// The vendor's own name for the source, kept for the document when
    /// `ref_id` is not resolved.
    #[serde(default)]
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputPatch {
    pub socket_id: String,
    pub source: OutputSource,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

/// What a scene stores, when the driver could read it: the same shapes as the
/// live state, so a scene can be inspected with the same tabs and diffed
/// against the current mix.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub buses: Vec<Bus>,
    #[serde(default)]
    pub dcas: Vec<Dca>,
    #[serde(default)]
    pub mute_groups: Vec<MuteGroup>,
    #[serde(default)]
    pub preamps: Vec<Preamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    /// `scene:<n>` — the slot number, or `scene:a:<n>` on a desk with banks.
    pub id: String,
    /// Slot number as the operator sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    /// Scene bank on desks that have them (Yamaha `A`/`B`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank: Option<String>,
    pub label: String,
    #[serde(default)]
    pub notes: String,
    /// The scene's recall filter / safes, as the vendor names them.
    #[serde(default)]
    pub filters: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Box<Snapshot>>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum CueStepKind {
    RecallScene,
    Wait,
    Midi,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CueStep {
    pub kind: CueStepKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    pub label: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub steps: Vec<CueStep>,
    #[serde(default, skip_serializing_if = "extra_is_empty")]
    pub extra: Extra,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VendorBlob {
    pub platform: Platform,
    /// `sq-show`, `ah-show`, `clf`, `mbdf-scene`, `songbook`.
    pub kind: String,
    pub sha256: String,
    /// Path relative to the show directory.
    pub file: String,
    pub size: u64,
    pub captured_at: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum NoteLevel {
    /// Carried over as-is, or a fact worth knowing.
    Info,
    /// Carried over with an adaptation — the nearest thing the target has.
    Adapted,
    /// Could not be carried; the target has nothing like it.
    Dropped,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub level: NoteLevel,
    /// Which entity: `channels/ch:3/sends/bus:aux:5`.
    pub path: String,
    pub message: String,
}

/// Helpers that build the standard entities so every driver spells them the
/// same way.
pub mod build {
    use super::*;

    pub fn unit(id: &str, label: &str, model: &str, role: UnitRole) -> Unit {
        Unit { id: ids::unit(id), label: label.into(), model: model.into(), role, address: None, extra: Extra::new() }
    }

    pub fn socket(unit: &str, direction: Direction, index: u32, kind: SocketKind, label: &str) -> Socket {
        Socket {
            id: ids::socket(unit, direction, index),
            unit_id: ids::unit(unit),
            kind,
            direction,
            index,
            label: label.into(),
            extra: Extra::new(),
        }
    }

    /// `n` sockets on a unit, labelled `<prefix> <i>`.
    pub fn sockets(unit: &str, direction: Direction, n: u32, kind: SocketKind, prefix: &str) -> Vec<Socket> {
        (1..=n).map(|i| socket(unit, direction, i, kind, &format!("{prefix} {i}"))).collect()
    }

    pub fn dca(n: u32, label: &str) -> Dca {
        Dca { id: ids::dca(n), number: n, label: label.into(), color: None, fader_db: None, on: None, extra: Extra::new() }
    }

    pub fn mute_group(n: u32, label: &str) -> MuteGroup {
        MuteGroup { id: ids::mute_group(n), number: n, label: label.into(), on: None, extra: Extra::new() }
    }

    pub fn scene(n: u32, label: &str) -> Scene {
        Scene {
            id: ids::scene(n),
            number: Some(n),
            bank: None,
            label: label.into(),
            notes: String::new(),
            filters: vec![],
            snapshot: None,
            extra: Extra::new(),
        }
    }
}

/// Format a level for people: `−∞`, `+3.5 dB`, `0 dB`.
pub fn fmt_db(v: f64) -> String {
    if v <= FADER_OFF + 0.5 {
        "−∞".into()
    } else if (v - v.round()).abs() < 0.05 {
        format!("{}{} dB", if v > 0.0 { "+" } else { "" }, v.round() as i64)
    } else {
        format!("{}{:.1} dB", if v > 0.0 { "+" } else { "" }, v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Show {
        let mut show = Show::new("Test", Platform::AhSq);
        show.system.units.push(build::unit("local", "SQ-5 local", "SQ-5", UnitRole::Console));
        show.sockets.extend(build::sockets("local", Direction::In, 4, SocketKind::Mic, "Local"));
        show.sockets.extend(build::sockets("local", Direction::Out, 2, SocketKind::Line, "Out"));
        show.preamps.push(Preamp { socket_id: ids::socket("local", Direction::In, 1), gain_db: Some(32.0), pad: Some(false), phantom: Some(true), extra: Extra::new() });
        show.buses.push(Bus::new(BusKind::Main, 1, "LR", true));
        show.buses.push(Bus::new(BusKind::Aux, 1, "Aux 1", false));
        show.dcas.push(build::dca(1, "Band"));
        show.mute_groups.push(build::mute_group(1, "Vox"));
        let mut ch = Channel::new(ids::channel(1), 1, ChannelKind::Input, "Kick");
        ch.source = Some(ids::socket("local", Direction::In, 1));
        ch.strip.fader_db = Some(-3.0);
        ch.strip.on = Some(true);
        ch.sends.push(Send { bus_id: ids::bus(BusKind::Aux, 1), level_db: Some(-10.0), on: Some(true), pre: Some(true), pan: None });
        ch.dca_ids.push(ids::dca(1));
        ch.mute_group_ids.push(ids::mute_group(1));
        show.channels.push(ch);
        show.output_patch.push(OutputPatch {
            socket_id: ids::socket("local", Direction::Out, 1),
            source: OutputSource { kind: OutputSourceKind::Bus, ref_id: Some(ids::bus(BusKind::Main, 1)), label: "LR L".into() },
            extra: Extra::new(),
        });
        show.scenes.push(build::scene(1, "Opening"));
        show.cues.push(Cue { id: ids::cue(1), number: Some(1), label: "Go".into(), notes: String::new(), steps: vec![CueStep { kind: CueStepKind::RecallScene, scene_id: Some(ids::scene(1)), delay_ms: None, extra: Extra::new() }], extra: Extra::new() });
        show
    }

    #[test]
    fn round_trips_through_json_and_validates() {
        let show = sample();
        let json = serde_json::to_string_pretty(&show).unwrap();
        let back: Show = serde_json::from_str(&json).unwrap();
        assert_eq!(show, back);
        assert!(show.validate().is_empty(), "{:?}", show.validate());
    }

    #[test]
    fn validate_reports_dangling_references() {
        let mut show = sample();
        show.channels[0].source = Some("skt:nowhere:in:9".into());
        show.channels[0].sends[0].bus_id = "bus:aux:99".into();
        show.cues[0].steps[0].scene_id = Some("scene:404".into());
        let problems = show.validate();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("skt:nowhere:in:9")));
        assert!(problems.iter().any(|p| p.contains("bus:aux:99")));
        assert!(problems.iter().any(|p| p.contains("scene:404")));
    }

    #[test]
    fn levels_format_for_people() {
        assert_eq!(fmt_db(FADER_OFF), "−∞");
        assert_eq!(fmt_db(0.0), "0 dB");
        assert_eq!(fmt_db(-3.5), "-3.5 dB");
        assert_eq!(fmt_db(10.0), "+10 dB");
    }

    #[test]
    fn notes_do_not_duplicate() {
        let mut show = sample();
        show.note(NoteLevel::Info, "a", "b");
        show.note(NoteLevel::Info, "a", "b");
        assert_eq!(show.notes.len(), 1);
    }
}

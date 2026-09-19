//! An MBDF scene (`.dm3s`, `.tfs`, `.dm7s`) → `Show`.
//!
//! One decoder serves the range because the payload carries its own schema;
//! every read below goes by collection and parameter *name*, and what a
//! model lacks is simply absent. The mapping was established on the factory
//! scenes inside DM3 Editor V3 and TF Editor V4.50 — real content such as
//! `Pulpit / Omni Lav / Headset / Handhld1` with its head-amp gains, mix
//! names and output patches — and has not been checked against a console's
//! own screens.
//!
//! # Encodings, as observed on the DM3
//!
//! - `Fader/Level`, `ToMix/Level`, `Input/Gain` are centi-dB (`-32768` = −∞).
//! - `ToStereo/Pan` is −63…+63.
//! - `HeadAmp/Gain` (in the `Process` record) is centi-dB; `48V` is 0/1.
//! - A patch word is `type << 16 | index`, 0-based: `0x0140` local mic input,
//!   `0x0160` the stereo line pair, `0x0420` a mix bus, `0x0460` a matrix,
//!   `0x0520` the stereo bus, `0x0640` an FX return, `0` unpatched. Other
//!   types are kept raw in `extra` and reported.

use songbook_model::{build, ids, Bus, BusKind, Channel, ChannelKind, Direction, Eq, Extra, Filter, NoteLevel, OutputPatch, OutputSource, OutputSourceKind, Platform, Preamp, Send, Show, SocketKind, UnitRole, FADER_OFF};

use crate::mbdf::Container;
use crate::mms::Payload;
use crate::Error;

/// A strip collection and what it becomes.
#[derive(Clone, Copy)]
enum Table {
    Channel(ChannelKind),
    Bus(BusKind),
    Dca,
}

/// Top-level strip collections across the range.
const TABLES: &[(&str, Table)] = &[
    ("InputChannel", Table::Channel(ChannelKind::Input)),
    ("StInChannel", Table::Channel(ChannelKind::StereoInput)),
    ("FxRtnChannel", Table::Channel(ChannelKind::FxReturn)),
    ("Group", Table::Bus(BusKind::Group)),
    ("Mix", Table::Bus(BusKind::Aux)),
    ("StMix", Table::Bus(BusKind::Aux)),
    ("Matrix", Table::Bus(BusKind::Matrix)),
    ("FxBus", Table::Bus(BusKind::FxSend)),
    ("Stereo", Table::Bus(BusKind::Main)),
    ("Mono", Table::Bus(BusKind::Other)),
    ("DCA", Table::Dca),
];

const PATCH_COLLECTIONS: &[&str] = &["Patch", "InPatch"];

pub fn platform_for(model: &str) -> Platform {
    let m = model.to_uppercase();
    if m.starts_with("DM3") {
        Platform::YamahaDm3
    } else if m.starts_with("DM7") {
        Platform::YamahaDm7
    } else if m.starts_with("TF") {
        Platform::YamahaTf
    } else {
        Platform::Generic
    }
}

/// The desk's colour word → the model's palette; the desk's own word is
/// kept in `extra.yamahaColor`.
pub fn color_name(s: &str) -> Option<&'static str> {
    Some(match s.trim().to_lowercase().replace(' ', "").as_str() {
        "" | "off" | "none" => return None,
        "red" => "red",
        "green" => "green",
        "yellow" => "yellow",
        "blue" => "blue",
        "purple" => "purple",
        "skyblue" | "cyan" | "lightblue" => "cyan",
        "white" => "white",
        "orange" => "orange",
        "pink" | "magenta" => "pink",
        _ => return None,
    })
}

fn db(v: i64) -> f64 {
    if v <= -32768 {
        FADER_OFF
    } else {
        v as f64 / 100.0
    }
}

fn pan(v: i64) -> f64 {
    (v as f64 / 63.0).clamp(-1.0, 1.0)
}

/// Decode a patch word into a `(unit, index)` for the sockets the model
/// keeps, or a bus reference for outputs.
fn decode_source(word: u64) -> Decoded {
    let index = (word & 0xFFFF) as u32 + 1;
    match (word >> 16) & 0xFFFF {
        0 => Decoded::None,
        0x0140 => Decoded::Socket("local", SocketKind::Mic, index),
        0x0160 => Decoded::Socket("stin", SocketKind::Line, index),
        0x0420 => Decoded::Bus(BusKind::Aux, index),
        0x0460 => Decoded::Bus(BusKind::Matrix, index),
        0x0520 => Decoded::Bus(BusKind::Main, index),
        0x0640 => Decoded::FxReturn(index),
        _ => Decoded::Unknown,
    }
}

enum Decoded {
    None,
    Socket(&'static str, SocketKind, u32),
    Bus(BusKind, u32),
    /// An FX return, by number.
    FxReturn(u32),
    Unknown,
}

pub struct Parsed {
    pub show: Show,
    pub model: String,
}

/// Parse an MBDF scene. A preset (one channel strip) parses too, but says
/// what it is instead of pretending to be a show.
pub fn parse(bytes: &[u8], file_name: &str) -> Result<Parsed, Error> {
    let container = Container::parse(bytes)?;
    let model = if container.model.is_empty() { "Yamaha".to_string() } else { container.model.clone() };
    let mut show = Show::new(file_name.rsplit('/').next().unwrap_or(file_name).rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name), platform_for(&model));
    show.system.model = model.clone();
    show.system.extra.insert("mbdfSubtype".into(), container.subtype.clone().into());
    show.system.extra.insert("mbdfVersion".into(), container.version.iter().map(|b| format!("{b:02x}")).collect::<String>().into());

    let mut title = None;
    let mut comment = String::new();
    if let Some(info) = container.record("Scene").or_else(|| container.record("Preset")) {
        if let Ok(p) = Payload::parse(&info.payload) {
            let get = |f: &str| p.string(&["Info", f], &[]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            title = get("Title");
            comment = get("Comment").unwrap_or_default();
            if let Some(sub) = get("ProductSubName") {
                show.system.model = sub;
            }
            if let Some(owner) = get("OwnerName") {
                show.meta.author = Some(owner);
            }
        }
    }
    if let Some(t) = &title {
        show.meta.name = t.clone();
    }

    if container.subtype == "Preset" {
        show.note(NoteLevel::Info, "system", "this is a channel preset, not a scene — it carries one strip's processing and no patch, routing or mix");
    }

    let Some(mixing) = container.record("Mixing") else {
        show.note(NoteLevel::Info, "system", format!("no Mixing record in this {} container; records present: {}", container.subtype, container.records.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", ")));
        return Ok(Parsed { show, model });
    };
    let p = Payload::parse(&mixing.payload)?;
    if !p.consistent() {
        show.note(NoteLevel::Info, "system", format!("the Mixing schema's children do not sum to the declared {} bytes; values below the mismatch are not trustworthy", p.declared_size()));
    }

    // Units the patch words can name.
    show.system.units.push(build::unit("local", &format!("{model} inputs"), &model, UnitRole::Console));
    show.system.units.push(build::unit("stin", "Stereo line inputs", &model, UnitRole::Console));
    show.system.units.push(build::unit("omni", "OMNI outputs", &model, UnitRole::Console));

    let mut unknown_patch = 0usize;
    let mut bus_numbers: std::collections::HashMap<BusKind, u32> = Default::default();

    for (table, what) in TABLES {
        let Some(count) = p.array_len(&[table]) else { continue };
        // The Stereo table holds L and R as two elements of one bus.
        let count = if *table == "Stereo" { count.min(1) } else { count };
        for i in 0..count {
            let n = i + 1;
            let idx = [i];
            let name = p.string(&[table, "Label", "Name"], &idx).map(|s| s.trim().to_string()).unwrap_or_default();
            let color_raw = p.string(&[table, "Label", "Color"], &idx).unwrap_or_default();
            let icon = p.string(&[table, "Label", "Icon"], &idx).unwrap_or_default();
            let fader = p.number(&[table, "Fader", "Level"], &idx).map(db);
            let on = p.number(&[table, "Fader", "On"], &idx).map(|v| v != 0);
            let mut extra = Extra::new();
            if !color_raw.is_empty() {
                extra.insert("yamahaColor".into(), color_raw.clone().into());
            }
            if !icon.is_empty() {
                extra.insert("yamahaIcon".into(), icon.into());
            }
            extra.insert("yamahaTable".into(), (*table).into());
            let color = color_name(&color_raw).map(str::to_string);
            match what {
                Table::Channel(kind) => {
                    let id = match kind {
                        ChannelKind::Input => ids::channel(n),
                        ChannelKind::StereoInput => ids::stereo_input(n),
                        ChannelKind::FxReturn => ids::fx_return(n),
                    };
                    let default = match kind {
                        ChannelKind::Input => format!("ch{n}"),
                        ChannelKind::StereoInput => format!("ST IN {n}"),
                        ChannelKind::FxReturn => format!("FX RTN {n}"),
                    };
                    let mut c = Channel::new(id, n, *kind, if name.is_empty() { &default } else { &name });
                    c.stereo = *kind != ChannelKind::Input;
                    c.color = color;
                    c.strip.fader_db = fader;
                    c.strip.on = on;
                    c.strip.pan = p.number(&[table, "ToStereo", "Pan"], &idx).map(pan);
                    c.strip.main_assign = p.number(&[table, "ToStereo", "Assign"], &idx).map(|v| v != 0);
                    c.strip.polarity = p.number(&[table, "Input", "Phase"], &idx).map(|v| v != 0);
                    c.strip.trim_db = p.number(&[table, "Input", "Gain"], &idx).map(|v| v as f64 / 100.0);
                    c.strip.eq = p.number(&[table, "PEQ", "On"], &idx).map(|v| Eq { on: v != 0, bands: vec![] });
                    c.strip.gate = p.number(&[table, "Gate", "On"], &idx).map(|v| songbook_model::Dynamics { on: v != 0, ..Default::default() });
                    c.strip.comp = p.number(&[table, "Comp", "On"], &idx).map(|v| songbook_model::Dynamics { on: v != 0, ..Default::default() });
                    if p.number(&[table, "Delay", "On"], &idx) == Some(1) {
                        c.strip.delay_ms = p.number(&[table, "Delay", "Time"], &idx).map(|v| v as f64 / 1000.0);
                    }
                    if let Some(hpf) = p.number(&[table, "HPF", "On"], &idx) {
                        c.strip.hpf = Some(Filter { on: hpf != 0, freq_hz: p.number(&[table, "HPF", "Freq"], &idx).map(|v| v as f64 / 100.0) });
                    }
                    c.strip.insert_on = p.number(&[table, "Insert", "On"], &idx).map(|v| v != 0);
                    // Sends.
                    for (coll, kind) in [("ToMix", BusKind::Aux), ("ToMatrix", BusKind::Matrix), ("ToFX", BusKind::FxSend)] {
                        let Some(sends) = p.array_len(&[table, coll]) else { continue };
                        for s in 0..sends {
                            let sidx = [i, s];
                            let level = p.number(&[table, coll, "Level"], &sidx).map(db);
                            if level.is_none() {
                                continue;
                            }
                            c.sends.push(Send {
                                bus_id: ids::bus(kind, s + 1),
                                level_db: level,
                                on: p.number(&[table, coll, "On"], &sidx).map(|v| v != 0),
                                pre: p.number(&[table, coll, "Pre"], &sidx).map(|v| v != 0),
                                pan: p.number(&[table, coll, "Pan"], &sidx).map(pan),
                            });
                        }
                    }
                    if let Some(groups) = p.array_len(&[table, "MuteGroup"]) {
                        for g in 0..groups {
                            if p.number(&[table, "MuteGroup", "Assign"], &[i, g]) == Some(1) {
                                c.mute_group_ids.push(ids::mute_group(g + 1));
                            }
                        }
                    }
                    if let Some(dcas) = p.array_len(&[table, "DCAGroup"]) {
                        for g in 0..dcas {
                            if p.number(&[table, "DCAGroup", "Assign"], &[i, g]) == Some(1) {
                                c.dca_ids.push(ids::dca(g + 1));
                            }
                        }
                    }
                    // Patch.
                    if let Some(pc) = PATCH_COLLECTIONS.iter().find(|pc| p.node(&[table, pc]).is_some()) {
                        let param = p.node(&[table, pc]).and_then(|n| n.children().iter().find(|c| !c.is_collection()).map(|c| c.name.clone()));
                        if let Some(param) = param {
                            let word = p.uint(&[table, pc, &param], &idx);
                            let width = p.node(&[table, pc, &param]).map(|n| n.datasize).unwrap_or(0);
                            match word.filter(|_| width >= 4).map(decode_source) {
                                Some(Decoded::Socket(unit, skind, index)) => {
                                    let sid = ids::socket(unit, Direction::In, index);
                                    if show.socket(&sid).is_none() {
                                        show.sockets.push(build::socket(unit, Direction::In, index, skind, &format!("{} {index}", if unit == "local" { "INPUT" } else { "ST IN" })));
                                    }
                                    c.source = Some(sid);
                                }
                                Some(Decoded::None) => {}
                                Some(_) | None => {
                                    if let Some(w) = word {
                                        c.extra.insert("yamahaPatch".into(), format!("{w:#x}").into());
                                        if width >= 4 && w != 0 {
                                            unknown_patch += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    c.extra.extend(extra);
                    show.channels.push(c);
                }
                Table::Bus(kind) => {
                    let counter = bus_numbers.entry(*kind).or_insert(0);
                    *counter += 1;
                    let number = *counter;
                    let default = format!("{} {n}", match kind {
                        BusKind::Aux => "MIX",
                        BusKind::Group => "GROUP",
                        BusKind::Matrix => "MTX",
                        BusKind::FxSend => "FX",
                        BusKind::Main => "ST",
                        BusKind::Other => "MONO",
                    });
                    let relation = p.number(&[table, "Signal", "Relation"], &idx).unwrap_or(0);
                    let stereo = matches!(kind, BusKind::Main) || *table == "StMix";
                    if relation != 0 {
                        // A linked pair: 128 marks the left member, 1 the right. Both
                        // stay separate buses because sends address them separately.
                        extra.insert("yamahaLink".into(), if relation & 0x80 != 0 { "L" } else { "R" }.into());
                    }
                    let mut b = Bus::new(*kind, number, if name.is_empty() { &default } else { &name }, stereo);
                    b.color = color;
                    b.strip.fader_db = fader;
                    b.strip.on = on;
                    b.strip.pan = p.number(&[table, "Output", "Balance"], &idx).or_else(|| p.number(&[table, "ToStereo", "Pan"], &idx)).map(pan);
                    b.strip.main_assign = p.number(&[table, "ToStereo", "Assign"], &idx).map(|v| v != 0);
                    b.strip.eq = p.number(&[table, "PEQ", "On"], &idx).map(|v| Eq { on: v != 0, bands: vec![] });
                    b.strip.comp = p.number(&[table, "Comp", "On"], &idx).map(|v| songbook_model::Dynamics { on: v != 0, ..Default::default() });
                    if let Some(bt) = p.string(&[table, "BusType"], &idx) {
                        if !bt.is_empty() {
                            extra.insert("yamahaBusType".into(), bt.into());
                        }
                    }
                    if let Some(sends) = p.array_len(&[table, "ToMatrix"]) {
                        for s in 0..sends {
                            let sidx = [i, s];
                            let Some(level) = p.number(&[table, "ToMatrix", "Level"], &sidx).map(db) else { continue };
                            b.sends.push(Send { bus_id: ids::bus(BusKind::Matrix, s + 1), level_db: Some(level), on: p.number(&[table, "ToMatrix", "On"], &sidx).map(|v| v != 0), pre: p.number(&[table, "ToMatrix", "Pre"], &sidx).map(|v| v != 0), pan: p.number(&[table, "ToMatrix", "Pan"], &sidx).map(pan) });
                        }
                    }
                    extra.insert("yamahaNumber".into(), n.into());
                    b.extra.extend(extra);
                    show.buses.push(b);
                }
                Table::Dca => {
                    let label = if name.is_empty() { format!("DCA {n}") } else { name.clone() };
                    let mut d = build::dca(n, &label);
                    d.color = color;
                    d.fader_db = fader;
                    d.on = on;
                    d.extra.extend(extra);
                    show.dcas.push(d);
                }
            }
        }
    }

    // Mute groups: as many as the input channels assign to.
    let mg = p.array_len(&["InputChannel", "MuteGroup"]).unwrap_or(0);
    for g in 1..=mg {
        show.mute_groups.push(build::mute_group(g, &format!("Mute Group {g}")));
    }
    if unknown_patch > 0 {
        show.note(NoteLevel::Info, "channels", format!("{unknown_patch} channel(s) carry a patch word whose type code is not one of the observed ones; the raw word is kept in extra.yamahaPatch"));
    }

    // Head amps and output patches from the Process record.
    if let Some(proc) = container.record("Process") {
        if let Ok(pp) = Payload::parse(&proc.payload) {
            for (unit, coll) in [("local", "BuiltIn"), ("dante", "Dante"), ("slot", "Slot")] {
                let Some(n) = pp.array_len(&[coll, "InputPort"]) else { continue };
                for i in 0..n {
                    let gain = pp.number(&[coll, "InputPort", "HeadAmp", "Gain"], &[i]);
                    let phantom = pp.number(&[coll, "InputPort", "HeadAmp", "48V"], &[i]);
                    if gain.is_none() && phantom.is_none() {
                        continue;
                    }
                    if unit != "local" && show.unit(&ids::unit(unit)).is_none() {
                        show.system.units.push(build::unit(unit, if unit == "dante" { "Dante" } else { "Slot" }, "", if unit == "dante" { UnitRole::Network } else { UnitRole::Card }));
                    }
                    let sid = ids::socket(unit, Direction::In, i + 1);
                    if show.socket(&sid).is_none() {
                        show.sockets.push(build::socket(unit, Direction::In, i + 1, if unit == "local" { SocketKind::Mic } else if unit == "dante" { SocketKind::Dante } else { SocketKind::Card }, &format!("{} {}", if unit == "local" { "INPUT" } else if unit == "dante" { "DANTE" } else { "SLOT" }, i + 1)));
                    }
                    show.preamps.push(Preamp { socket_id: sid, gain_db: gain.map(|g| g as f64 / 100.0), pad: None, phantom: phantom.map(|v| v != 0), extra: Extra::new() });
                }
            }
            for (unit, coll, label, kind) in [("omni", "Omni", "OMNI", SocketKind::Line), ("dante", "Dante", "DANTE", SocketKind::Dante), ("usb", "USBToHost", "USB", SocketKind::Usb), ("local", "BuiltIn", "OUT", SocketKind::Line)] {
                let Some(n) = pp.array_len(&[coll, "OutputPort"]) else { continue };
                for i in 0..n {
                    let Some(word) = pp.uint(&[coll, "OutputPort", "Patch", "Source"], &[i]) else { continue };
                    if word == 0 {
                        continue;
                    }
                    if show.unit(&ids::unit(unit)).is_none() {
                        show.system.units.push(build::unit(unit, label, "", if unit == "dante" { UnitRole::Network } else if unit == "usb" { UnitRole::Internal } else { UnitRole::Console }));
                    }
                    let sid = ids::socket(unit, Direction::Out, i + 1);
                    if show.socket(&sid).is_none() {
                        show.sockets.push(build::socket(unit, Direction::Out, i + 1, kind, &format!("{label} {}", i + 1)));
                    }
                    let source = match decode_source(word) {
                        Decoded::Bus(BusKind::Main, index) => {
                            let id = show.buses.iter().find(|b| b.kind == BusKind::Main).map(|b| b.id.clone());
                            OutputSource { kind: OutputSourceKind::Bus, ref_id: id, label: format!("ST {}", if index == 1 { "L" } else { "R" }) }
                        }
                        Decoded::Bus(kind, index) => {
                            let id = show.buses.iter().find(|b| b.kind == kind && b.extra.get("yamahaNumber").and_then(|v| v.as_u64()) == Some(index as u64)).map(|b| b.id.clone());
                            OutputSource { kind: OutputSourceKind::Bus, ref_id: id, label: format!("{} {index}", kind.label()) }
                        }
                        Decoded::Socket(u, _, index) => OutputSource { kind: OutputSourceKind::Socket, ref_id: Some(ids::socket(u, Direction::In, index)).filter(|s| show.socket(s).is_some()), label: format!("{} {index}", if u == "local" { "INPUT" } else { "ST IN" }) },
                        Decoded::FxReturn(index) => OutputSource { kind: OutputSourceKind::Channel, ref_id: Some(ids::fx_return(index)).filter(|c| show.channel(c).is_some()), label: format!("FX RTN {index}") },
                        _ => OutputSource { kind: OutputSourceKind::Other, ref_id: None, label: format!("{word:#x}") },
                    };
                    show.output_patch.push(OutputPatch { socket_id: sid, source, extra: Extra::new() });
                }
            }
        }
    }

    // The scene itself.
    let mut sc = build::scene(0, title.as_deref().unwrap_or(&show.meta.name));
    sc.number = None;
    sc.notes = comment.clone();
    sc.id = "scene:file".into();
    show.scenes.push(sc);
    if !comment.is_empty() {
        show.meta.notes = comment;
    }
    show.note(NoteLevel::Info, "channels", format!("read {} collections and {} parameters from the Mixing record; EQ bands, gate and compressor settings are present but only their on/off is carried", p.collections, p.parameters));
    show.meta.source = Some(songbook_model::SourceInfo { kind: "file".into(), origin: file_name.to_string(), at: songbook_model::now(), firmware: None });
    Ok(Parsed { show, model })
}

/// A DM3-shaped scene for tests and the demo: two inputs, a stereo input,
/// two mixes, the stereo bus, a DCA, head amps and omni outputs.
pub fn synthetic_dm3(title: &str) -> Vec<u8> {
    use crate::mbdf::build as cb;
    use crate::mms::build::*;
    use crate::mms::{TYPE_SIGNED, TYPE_STRING, TYPE_UNSIGNED};
    // InputChannel: Label{Name 8, Color 8} 16 | Patch{Source u32} 4 | Input{Phase u8, Gain i16} 3 | Fader{Level i16, On u8} 3 | ToStereo{Pan i8, Assign u8} 2 | ToMix[2]{Level i16, On u8} 6 | MuteGroup[2]{Assign u8} 2 => 36
    // Mix[2]: Label 16 | Fader 3 | Signal{Relation u8} 1 => 20
    // Stereo[1]: Label 16 | Fader 3 => 19
    // DCA[1]: Label 16 | Fader 3 => 19
    let ic = 36u32;
    let mut schema = vec![col("Mixing", 0, ic * 2 + 20 * 2 + 19 + 19, 1)];
    schema.extend([
        col("InputChannel", 0, ic, 2),
        col("Label", 0, 16, 1),
        pr("Name", TYPE_STRING, 8, 1),
        pr("Color", TYPE_STRING, 8, 1),
        col("Patch", 16, 4, 1),
        pr("Source", TYPE_UNSIGNED, 4, 1),
        col("Input", 20, 3, 1),
        pr("Phase", TYPE_UNSIGNED, 1, 1),
        pr("Gain", TYPE_SIGNED, 2, 1),
        col("Fader", 23, 3, 1),
        pr("Level", TYPE_SIGNED, 2, 1),
        pr("On", TYPE_UNSIGNED, 1, 1),
        col("ToStereo", 26, 2, 1),
        pr("Pan", TYPE_SIGNED, 1, 1),
        pr("Assign", TYPE_UNSIGNED, 1, 1),
        col("ToMix", 28, 3, 2),
        pr("Level", TYPE_SIGNED, 2, 1),
        pr("On", TYPE_UNSIGNED, 1, 1),
        col("MuteGroup", 34, 1, 2),
        pr("Assign", TYPE_UNSIGNED, 1, 1),
        col("Mix", ic * 2, 20, 2),
        col("Label", 0, 16, 1),
        pr("Name", TYPE_STRING, 8, 1),
        pr("Color", TYPE_STRING, 8, 1),
        col("Fader", 16, 3, 1),
        pr("Level", TYPE_SIGNED, 2, 1),
        pr("On", TYPE_UNSIGNED, 1, 1),
        col("Signal", 19, 1, 1),
        pr("Relation", TYPE_UNSIGNED, 1, 1),
        col("Stereo", ic * 2 + 40, 19, 1),
        col("Label", 0, 16, 1),
        pr("Name", TYPE_STRING, 8, 1),
        pr("Color", TYPE_STRING, 8, 1),
        col("Fader", 16, 3, 1),
        pr("Level", TYPE_SIGNED, 2, 1),
        pr("On", TYPE_UNSIGNED, 1, 1),
        col("DCA", ic * 2 + 59, 19, 1),
        col("Label", 0, 16, 1),
        pr("Name", TYPE_STRING, 8, 1),
        pr("Color", TYPE_STRING, 8, 1),
        col("Fader", 16, 3, 1),
        pr("Level", TYPE_SIGNED, 2, 1),
        pr("On", TYPE_UNSIGNED, 1, 1),
    ]);
    fn s8(s: &str) -> [u8; 8] {
        let mut b = [0u8; 8];
        let x = s.as_bytes();
        b[..x.len().min(8)].copy_from_slice(&x[..x.len().min(8)]);
        b
    }
    let mut v = Vec::new();
    for (name, color, patch, gain, level, on, pan, assign, m1, m2, mg) in [
        ("Kick", "Blue", 0x0140_0000u32, 0i16, -300i16, 1u8, 0i8, 1u8, -1000i16, -32768i16, [1u8, 0u8]),
        ("Vox", "Green", 0x0140_0007, 250, 0, 0, 20, 1, -600, -1200, [0, 1]),
    ] {
        v.extend_from_slice(&s8(name));
        v.extend_from_slice(&s8(color));
        v.extend_from_slice(&patch.to_le_bytes());
        v.push(0);
        v.extend_from_slice(&gain.to_le_bytes());
        v.extend_from_slice(&level.to_le_bytes());
        v.push(on);
        v.push(pan as u8);
        v.push(assign);
        v.extend_from_slice(&m1.to_le_bytes());
        v.push(1);
        v.extend_from_slice(&m2.to_le_bytes());
        v.push(1);
        v.extend_from_slice(&mg);
    }
    for (name, color, level, rel) in [("Wedge", "SkyBlue", 0i16, 0u8), ("IEM L", "Pink", -600, 128)] {
        v.extend_from_slice(&s8(name));
        v.extend_from_slice(&s8(color));
        v.extend_from_slice(&level.to_le_bytes());
        v.push(1);
        v.push(rel);
    }
    v.extend_from_slice(&s8("Stereo"));
    v.extend_from_slice(&s8("White"));
    v.extend_from_slice(&(-100i16).to_le_bytes());
    v.push(1);
    v.extend_from_slice(&s8("Band"));
    v.extend_from_slice(&s8("Red"));
    v.extend_from_slice(&(0i16).to_le_bytes());
    v.push(1);
    let mixing = payload("Mixing", schema, &v);

    // Process: BuiltIn{InputPort[2]{HeadAmp{48V u8, Gain i16}}} | Omni{OutputPort[2]{Patch{Source u32}}}
    let pschema = vec![
        col("Processing", 0, 6 + 8, 1),
        col("BuiltIn", 0, 6, 1),
        col("InputPort", 0, 3, 2),
        col("HeadAmp", 0, 3, 1),
        pr("48V", TYPE_UNSIGNED, 1, 1),
        pr("Gain", TYPE_SIGNED, 2, 1),
        col("Omni", 6, 8, 1),
        col("OutputPort", 0, 4, 2),
        col("Patch", 0, 4, 1),
        pr("Source", TYPE_UNSIGNED, 4, 1),
    ];
    let mut pv = vec![1u8];
    pv.extend_from_slice(&(3200i16).to_le_bytes());
    pv.push(0);
    pv.extend_from_slice(&(1500i16).to_le_bytes());
    pv.extend_from_slice(&0x0420_0000u32.to_le_bytes());
    pv.extend_from_slice(&0x0520_0001u32.to_le_bytes());
    let process = payload("Processing", pschema, &pv);

    let sschema = vec![col("SceneInfo", 0, 64 + 128, 1), col("Info", 0, 192, 1), pr("Title", TYPE_STRING, 64, 1), pr("Comment", TYPE_STRING, 128, 1)];
    let mut sv = vec![0u8; 192];
    sv[..title.len().min(64)].copy_from_slice(&title.as_bytes()[..title.len().min(64)]);
    sv[64..64 + 8].copy_from_slice(b"For test");
    let scene = payload("SceneInfo", sschema, &sv);

    cb::container("Scene", "DM3", &[("Scene", b"", &scene), ("Mixing", b"", &mixing), ("Process", b"", &process)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_synthetic_dm3_scene() {
        let raw = synthetic_dm3("Opening");
        let r = parse(&raw, "opening.dm3s").unwrap();
        let show = r.show;
        assert_eq!(show.platform, Platform::YamahaDm3);
        assert_eq!(show.meta.name, "Opening");
        assert_eq!(show.meta.notes, "For test");
        assert_eq!(show.channels.len(), 2);
        let kick = &show.channels[0];
        assert_eq!(kick.label, "Kick");
        assert_eq!(kick.color.as_deref(), Some("blue"));
        assert_eq!(kick.source.as_deref(), Some("skt:local:in:1"));
        assert_eq!(kick.strip.fader_db, Some(-3.0));
        assert_eq!(kick.strip.on, Some(true));
        assert_eq!(kick.strip.pan, Some(0.0));
        assert_eq!(kick.sends.len(), 2);
        assert_eq!(kick.sends[0].bus_id, "bus:aux:1");
        assert_eq!(kick.sends[0].level_db, Some(-10.0));
        assert_eq!(kick.sends[1].level_db, Some(FADER_OFF));
        assert_eq!(kick.mute_group_ids, vec!["mg:1"]);
        let vox = &show.channels[1];
        assert_eq!(vox.source.as_deref(), Some("skt:local:in:8"));
        assert_eq!(vox.strip.on, Some(false));
        assert!((vox.strip.pan.unwrap() - 20.0 / 63.0).abs() < 0.001);
        assert_eq!(vox.strip.trim_db, Some(2.5));
        assert_eq!(vox.mute_group_ids, vec!["mg:2"]);
        let auxes: Vec<_> = show.buses_of(BusKind::Aux).collect();
        assert_eq!(auxes.len(), 2);
        assert_eq!(auxes[0].label, "Wedge");
        assert_eq!(auxes[0].color.as_deref(), Some("cyan"));
        assert!(!auxes[0].stereo);
        assert!(!auxes[1].stereo, "a linked pair stays two buses");
        assert_eq!(show.buses_of(BusKind::Main).next().unwrap().strip.fader_db, Some(-1.0));
        assert_eq!(show.dcas[0].label, "Band");
        assert_eq!(show.mute_groups.len(), 2);
        assert_eq!(show.preamps.len(), 2);
        assert_eq!(show.preamps[0].gain_db, Some(32.0));
        assert_eq!(show.preamps[0].phantom, Some(true));
        assert_eq!(show.preamps[1].socket_id, "skt:local:in:2");
        assert_eq!(show.output_patch.len(), 2);
        assert_eq!(show.output_patch[0].source.ref_id.as_deref(), Some("bus:aux:1"));
        assert_eq!(show.output_patch[1].source.ref_id.as_deref(), Some("bus:main:1"));
        assert_eq!(show.output_patch[1].source.label, "ST R");
        assert_eq!(auxes[1].extra["yamahaLink"], "L");
        assert_eq!(show.scenes.len(), 1);
        assert!(show.validate().is_empty(), "{:?}", show.validate());
    }

    #[test]
    fn a_preset_and_a_missing_mixing_record_are_reported() {
        use crate::mbdf::build as cb;
        let raw = cb::container("Preset", "TF", &[("Process", b"CH\0\0\0\0\0\x01", b"x")]);
        let r = parse(&raw, "vox.tfp").unwrap();
        assert_eq!(r.show.platform, Platform::YamahaTf);
        assert!(r.show.notes.iter().any(|n| n.message.contains("channel preset")));
        assert!(r.show.notes.iter().any(|n| n.message.contains("no Mixing record")));
    }

    /// The factory scenes inside DM3 Editor / TF Editor, when installed.
    #[test]
    fn real_factory_scenes_if_present() {
        let Ok(dir) = std::env::var("SONGBOOK_YAMAHA_SCENES") else { return };
        let mut n = 0;
        for e in walk(std::path::Path::new(&dir)) {
            let name = e.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if !(name.ends_with(".dm3s") || name.ends_with(".tfs") || name.ends_with(".dm7s")) {
                continue;
            }
            let bytes = std::fs::read(&e).unwrap();
            let r = parse(&bytes, &name).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(!r.show.channels.is_empty(), "{name}");
            assert!(r.show.validate().is_empty(), "{name}: {:?}", r.show.validate());
            n += 1;
            eprintln!(
                "{} [{}] {:?}: {} ch, {} buses, {} preamps, {} outputs, patched {}; {:?}",
                r.show.meta.name,
                r.model,
                r.show.platform,
                r.show.channels.len(),
                r.show.buses.len(),
                r.show.preamps.len(),
                r.show.output_patch.len(),
                r.show.channels.iter().filter(|c| c.source.is_some()).count(),
                r.show.channels.iter().take(4).map(|c| (c.label.clone(), c.strip.fader_db)).collect::<Vec<_>>()
            );
        }
        assert!(n > 0, "no scenes under {dir}");
    }

    fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    out.extend(walk(&path));
                } else {
                    out.push(path);
                }
            }
        }
        out
    }
}

//! Convert a show for another desk, and say exactly what happened to it.
//!
//! The rules are held in one place ([`capabilities`]) and applied the same
//! way to every platform: keep what fits, adapt what nearly fits (a name
//! cut to the target's length, a colour the target does not have mapped to
//! its nearest), drop what does not (channel 49 on a 48-channel desk, an aux
//! beyond the target's mix count and every send into it), and write a
//! [`Note`] for each adaptation and drop so the report is the show's own
//! record. Nothing vendor-specific crosses: every `extra` bag is emptied,
//! because a target driver addresses strips by number and kind.

pub mod capabilities;

use std::collections::HashSet;

use serde::Serialize;
use songbook_model::{build, ids, BusKind, ChannelKind, Direction, Extra, Note, NoteLevel, Platform, Show, SocketKind, UnitRole};

pub use capabilities::{capabilities, models, Capabilities};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Conversion {
    pub show: Show,
    pub notes: Vec<Note>,
    pub carried: usize,
    pub adapted: usize,
    pub dropped: usize,
}

struct Report {
    notes: Vec<Note>,
    carried: usize,
}

impl Report {
    fn adapted(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.notes.push(Note { level: NoteLevel::Adapted, path: path.into(), message: msg.into() });
    }
    fn dropped(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.notes.push(Note { level: NoteLevel::Dropped, path: path.into(), message: msg.into() });
    }
    fn info(&mut self, path: impl Into<String>, msg: impl Into<String>) {
        self.notes.push(Note { level: NoteLevel::Info, path: path.into(), message: msg.into() });
    }
}

/// The nearest colour the target palette has.
fn nearest_color(color: &str, palette: &[String]) -> Option<String> {
    if palette.iter().any(|c| c == color) {
        return Some(color.to_string());
    }
    let candidates: &[&str] = match color {
        "cyan" => &["blue", "green", "white"],
        "orange" => &["yellow", "red"],
        "pink" => &["purple", "red"],
        "white" => &["off", "yellow"],
        "purple" => &["pink", "blue"],
        "off" => &["white"],
        _ => &[],
    };
    candidates.iter().find(|c| palette.iter().any(|p| p == *c)).map(|c| c.to_string())
}

fn fit_name(name: &str, max: u32) -> String {
    name.chars().take(max as usize).collect::<String>().trim_end().to_string()
}

pub fn convert(source: &Show, target: Platform, model: &str) -> Conversion {
    let cap = capabilities(target, model);
    let mut r = Report { notes: vec![], carried: 0 };
    let mut out = Show::new(&format!("{} (for {})", source.meta.name, model), target);
    out.system.model = model.to_string();
    out.meta.tags = source.meta.tags.clone();
    out.meta.notes = source.meta.notes.clone();
    out.meta.author = source.meta.author.clone();
    out.meta.source = Some(songbook_model::SourceInfo { kind: "conversion".into(), origin: format!("{} ({} {})", source.meta.name, source.platform.label(), source.system.model), at: songbook_model::now(), firmware: None });

    // ---- units and sockets: the target's own local I/O, plus one network
    // unit for anything the source had on a stage box or a stream.
    out.system.units.push(build::unit("local", &format!("{model} local sockets"), model, UnitRole::Console));
    out.sockets.extend(build::sockets("local", Direction::In, cap.local_inputs, SocketKind::Mic, "Local"));
    out.sockets.extend(build::sockets("local", Direction::Out, cap.local_outputs, SocketKind::Line, "Out"));
    let mut network_used = false;
    let mut map_socket = |sid: &str, out: &mut Show, r: &mut Report, path: &str| -> Option<String> {
        let s = source.socket(sid)?;
        if s.direction != Direction::In {
            return None;
        }
        let unit = s.unit_id.trim_start_matches("unit:");
        if unit == "local" || unit == "input" {
            if s.index <= cap.local_inputs {
                return Some(ids::socket("local", Direction::In, s.index));
            }
            r.dropped(path, format!("patched from local socket {} and {model} has {} local inputs; left unpatched", s.index, cap.local_inputs));
            return None;
        }
        // Stage box / network: keep the number on a generic network unit.
        if !network_used {
            out.system.units.push(build::unit("network", "Network / stage box inputs", "", UnitRole::Network));
            network_used = true;
        }
        let nid = ids::socket("network", Direction::In, s.index);
        if out.socket(&nid).is_none() {
            out.sockets.push(build::socket("network", Direction::In, s.index, SocketKind::Other, &format!("Network {}", s.index)));
        }
        r.adapted(path, format!("patched from {} {} on the source; carried as network input {} — assign the real socket on the target", source.unit(&s.unit_id).map(|u| u.label.as_str()).unwrap_or(unit), s.index, s.index));
        Some(nid)
    };

    // ---- buses
    let mut kept_bus: HashSet<String> = HashSet::new();
    let mut pool_used = 0u32;
    for b in &source.buses {
        let path = format!("buses/{}", b.id);
        let limit = match b.kind {
            BusKind::Main => cap.mains,
            BusKind::Aux => cap.auxes,
            BusKind::Group => cap.groups,
            BusKind::Matrix => cap.matrices,
            BusKind::FxSend => cap.fx_sends,
            BusKind::Other => 1,
        };
        let count_so_far = out.buses.iter().filter(|x| x.kind == b.kind).count() as u32;
        if count_so_far >= limit {
            if b.kind == BusKind::Group && cap.groups == 0 && cap.mix_pool > 0 && pool_used < cap.mix_pool {
                // A desk whose groups come out of the aux pool.
            } else {
                r.dropped(&path, format!("{} {} \"{}\": {model} has {limit} {}s", b.kind.label(), b.number, b.label, b.kind.label().to_lowercase()));
                continue;
            }
        }
        if cap.mix_pool > 0 && matches!(b.kind, BusKind::Aux | BusKind::Group) {
            if pool_used >= cap.mix_pool {
                r.dropped(&path, format!("{} {} \"{}\": {model}'s {} mix buses are all used", b.kind.label(), b.number, b.label, cap.mix_pool));
                continue;
            }
            pool_used += 1;
        }
        let mut nb = b.clone();
        nb.extra = Extra::new();
        nb.sends.clear();
        nb.dca_ids.clear();
        nb.mute_group_ids.clear();
        nb.label = fit_name(&b.label, cap.name_length);
        if nb.label != b.label {
            r.adapted(&path, format!("name \"{}\" cut to \"{}\" ({} characters)", b.label, nb.label, cap.name_length));
        }
        if let Some(c) = &b.color {
            match nearest_color(c, &cap.colors) {
                Some(n) if &n == c => {}
                Some(n) => {
                    r.adapted(&path, format!("colour {c} is not in the {model} palette; {n} used"));
                    nb.color = Some(n);
                }
                None => {
                    r.adapted(&path, format!("colour {c} is not in the {model} palette; no colour"));
                    nb.color = None;
                }
            }
        }
        kept_bus.insert(nb.id.clone());
        r.carried += 1;
        out.buses.push(nb);
    }
    // Bus-to-bus sends (aux → matrix) once every bus is known.
    for b in &source.buses {
        if !kept_bus.contains(&b.id) {
            continue;
        }
        let mut sends = vec![];
        for s in &b.sends {
            if kept_bus.contains(&s.bus_id) {
                sends.push(s.clone());
            } else {
                r.dropped(format!("buses/{}/sends/{}", b.id, s.bus_id), format!("send from \"{}\" into a bus the target does not have", b.label));
            }
        }
        if let Some(nb) = out.buses.iter_mut().find(|x| x.id == b.id) {
            nb.sends = sends;
        }
    }

    // ---- DCAs and mute groups
    for d in &source.dcas {
        if d.number > cap.dcas {
            r.dropped(format!("dcas/{}", d.id), format!("DCA {} \"{}\": {model} has {} DCAs", d.number, d.label, cap.dcas));
            continue;
        }
        let mut nd = d.clone();
        nd.extra = Extra::new();
        nd.label = fit_name(&d.label, cap.name_length);
        if nd.label != d.label {
            r.adapted(format!("dcas/{}", d.id), format!("name \"{}\" cut to \"{}\"", d.label, nd.label));
        }
        nd.color = nd.color.as_deref().and_then(|c| nearest_color(c, &cap.colors));
        r.carried += 1;
        out.dcas.push(nd);
    }
    for m in &source.mute_groups {
        if m.number > cap.mute_groups {
            r.dropped(format!("muteGroups/{}", m.id), format!("mute group {} \"{}\": {model} has {} mute groups", m.number, m.label, cap.mute_groups));
            continue;
        }
        let mut nm = m.clone();
        nm.extra = Extra::new();
        nm.label = fit_name(&m.label, cap.name_length);
        r.carried += 1;
        out.mute_groups.push(nm);
    }
    let kept_dca: HashSet<String> = out.dcas.iter().map(|d| d.id.clone()).collect();
    let kept_mg: HashSet<String> = out.mute_groups.iter().map(|m| m.id.clone()).collect();
    for b in &source.buses {
        if let Some(nb) = out.buses.iter_mut().find(|x| x.id == b.id) {
            nb.dca_ids = b.dca_ids.iter().filter(|d| kept_dca.contains(*d)).cloned().collect();
            nb.mute_group_ids = b.mute_group_ids.iter().filter(|m| kept_mg.contains(*m)).cloned().collect();
        }
    }

    // ---- channels
    let mut counts: std::collections::HashMap<ChannelKind, u32> = Default::default();
    for c in &source.channels {
        let path = format!("channels/{}", c.id);
        let limit = match c.kind {
            ChannelKind::Input => cap.input_channels,
            ChannelKind::StereoInput => cap.stereo_inputs,
            ChannelKind::FxReturn => cap.fx_returns,
        };
        let n = counts.entry(c.kind).or_insert(0);
        *n += 1;
        if *n > limit {
            let what = match c.kind {
                ChannelKind::Input => "input channels",
                ChannelKind::StereoInput => "stereo inputs",
                ChannelKind::FxReturn => "FX returns",
            };
            r.dropped(&path, format!("{} \"{}\": {model} has {limit} {what}", c.id, c.label));
            continue;
        }
        let mut nc = c.clone();
        nc.extra = Extra::new();
        nc.label = fit_name(&c.label, cap.name_length);
        if nc.label != c.label {
            r.adapted(&path, format!("name \"{}\" cut to \"{}\" ({} characters)", c.label, nc.label, cap.name_length));
        }
        if let Some(col) = &c.color {
            match nearest_color(col, &cap.colors) {
                Some(x) if &x == col => {}
                Some(x) => {
                    r.adapted(&path, format!("colour {col} is not in the {model} palette; {x} used"));
                    nc.color = Some(x);
                }
                None => {
                    r.adapted(&path, format!("colour {col} is not in the {model} palette; no colour"));
                    nc.color = None;
                }
            }
        }
        nc.source = c.source.as_deref().and_then(|s| map_socket(s, &mut out, &mut r, &path));
        nc.source_right = c.source_right.as_deref().and_then(|s| map_socket(s, &mut out, &mut r, &format!("{path}/right")));
        let mut sends = vec![];
        let mut lost = 0usize;
        for s in &c.sends {
            if kept_bus.contains(&s.bus_id) {
                sends.push(s.clone());
            } else {
                lost += 1;
            }
        }
        if lost > 0 {
            r.dropped(format!("{path}/sends"), format!("{lost} send(s) from \"{}\" into buses the target does not have", c.label));
        }
        nc.sends = sends;
        nc.dca_ids = c.dca_ids.iter().filter(|d| kept_dca.contains(*d)).cloned().collect();
        nc.mute_group_ids = c.mute_group_ids.iter().filter(|m| kept_mg.contains(*m)).cloned().collect();
        if let Some(eq) = &mut nc.strip.eq {
            if eq.bands.len() as u32 > cap.eq_bands {
                r.dropped(format!("{path}/eq"), format!("{} EQ bands on \"{}\"; {model} has {} — the extra bands are dropped", eq.bands.len(), c.label, cap.eq_bands));
                eq.bands.truncate(cap.eq_bands as usize);
            }
        }
        r.carried += 1;
        out.channels.push(nc);
    }

    // ---- preamps travel with their sockets
    for p in &source.preamps {
        let Some(sk) = source.socket(&p.socket_id) else { continue };
        let unit = sk.unit_id.trim_start_matches("unit:");
        let target_id = if unit == "local" || unit == "input" {
            if sk.index <= cap.local_inputs {
                Some(ids::socket("local", Direction::In, sk.index))
            } else {
                None
            }
        } else {
            let nid = ids::socket("network", Direction::In, sk.index);
            out.socket(&nid).map(|_| nid)
        };
        let Some(tid) = target_id else { continue };
        if !cap.preamp_control {
            r.info(format!("preamps/{}", p.socket_id), format!("{model} does not expose its head amps to software; gain {:?} / phantom {:?} are kept for the document but cannot be pushed", p.gain_db, p.phantom));
        }
        let mut np = p.clone();
        np.socket_id = tid;
        np.extra = Extra::new();
        out.preamps.push(np);
    }

    // ---- output patch: only what lands on a carried bus and a local output
    for o in &source.output_patch {
        let Some(sk) = source.socket(&o.socket_id) else { continue };
        let unit = sk.unit_id.trim_start_matches("unit:");
        if !(unit == "local" || unit == "omni") || sk.index > cap.local_outputs {
            r.dropped(format!("outputs/{}", o.socket_id), format!("output {} on {}: {model} has {} local outputs", sk.index, sk.label, cap.local_outputs));
            continue;
        }
        let mut no = o.clone();
        no.extra = Extra::new();
        no.socket_id = ids::socket("local", Direction::Out, sk.index);
        if let Some(rid) = &o.source.ref_id {
            let ok = match o.source.kind {
                songbook_model::OutputSourceKind::Bus => kept_bus.contains(rid),
                songbook_model::OutputSourceKind::Channel | songbook_model::OutputSourceKind::DirectOut => out.channel(rid).is_some(),
                songbook_model::OutputSourceKind::Socket => out.socket(rid).is_some(),
                songbook_model::OutputSourceKind::Other => true,
            };
            if !ok {
                r.dropped(format!("outputs/{}", o.socket_id), format!("output {} carried \"{}\", which the target does not have", sk.index, o.source.label));
                continue;
            }
        }
        out.output_patch.push(no);
    }

    // ---- scenes and cues
    for (i, s) in source.scenes.iter().enumerate() {
        if i as u32 >= cap.scene_slots {
            r.dropped(format!("scenes/{}", s.id), format!("scene \"{}\": {model} has {} scene slots", s.label, cap.scene_slots));
            continue;
        }
        let mut ns = s.clone();
        ns.extra = Extra::new();
        ns.bank = None;
        ns.id = ids::scene(s.number.unwrap_or(i as u32 + 1));
        ns.number = Some(s.number.unwrap_or(i as u32 + 1));
        ns.label = fit_name(&s.label, 16.max(cap.name_length));
        if ns.snapshot.is_some() {
            ns.snapshot = None;
            r.adapted(format!("scenes/{}", s.id), format!("scene \"{}\" carried by name and notes; its stored mix is not converted — recall on the source, pull, and store on the target", s.label));
        }
        out.scenes.push(ns);
    }
    if !source.cues.is_empty() {
        if cap.cues {
            for c in &source.cues {
                let mut nc = c.clone();
                nc.extra = Extra::new();
                nc.steps.retain(|st| st.scene_id.as_deref().map(|id| out.scenes.iter().any(|s| s.id == id)).unwrap_or(true));
                out.cues.push(nc);
            }
        } else {
            r.dropped("cues", format!("{} cue(s): {model} has no cue list", source.cues.len()));
        }
    }

    r.info("system", format!("converted from {} {} to {} {}: {} entities carried, {} adapted, {} dropped", source.platform.label(), source.system.model, target.label(), model, r.carried, r.notes.iter().filter(|n| n.level == NoteLevel::Adapted).count(), r.notes.iter().filter(|n| n.level == NoteLevel::Dropped).count()));
    let adapted = r.notes.iter().filter(|n| n.level == NoteLevel::Adapted).count();
    let dropped = r.notes.iter().filter(|n| n.level == NoteLevel::Dropped).count();
    out.notes = r.notes.clone();
    let problems = out.validate();
    debug_assert!(problems.is_empty(), "{problems:?}");
    Conversion { show: out, notes: r.notes, carried: r.carried, adapted, dropped }
}

/// A new, empty show for a desk: the SQ skeleton for SQ, otherwise a shell
/// sized from the capability table. What `New show` and the browser tool
/// both start from.
pub fn blank_show(name: &str, platform: Platform, model: &str) -> Show {
    use songbook_model::{build, ids, Bus, BusKind, Channel, ChannelKind, Direction, SocketKind, UnitRole};
    let mut show = match platform {
        Platform::AhSq => songbook_ah::import::sq_skeleton(name, platform, model),
        _ => {
            let mut s = Show::new(name, platform);
            let cap = capabilities(platform, model);
            s.system.units.push(build::unit("local", &format!("{model} local sockets"), model, UnitRole::Console));
            s.sockets.extend(build::sockets("local", Direction::In, cap.local_inputs, SocketKind::Mic, "Local"));
            s.sockets.extend(build::sockets("local", Direction::Out, cap.local_outputs, SocketKind::Line, "Out"));
            for n in 1..=cap.input_channels {
                s.channels.push(Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Ch {n}")));
            }
            for n in 1..=cap.mains {
                let label = if n == 1 { "Main".to_string() } else { format!("Main {n}") };
                s.buses.push(Bus::new(BusKind::Main, n, &label, true));
            }
            for n in 1..=cap.auxes {
                s.buses.push(Bus::new(BusKind::Aux, n, &format!("Aux {n}"), false));
            }
            for n in 1..=cap.matrices {
                s.buses.push(Bus::new(BusKind::Matrix, n, &format!("Mtx {n}"), false));
            }
            for n in 1..=cap.fx_sends {
                s.buses.push(Bus::new(BusKind::FxSend, n, &format!("FX {n}"), false));
            }
            for n in 1..=cap.dcas {
                s.dcas.push(build::dca(n, &format!("DCA {n}")));
            }
            for n in 1..=cap.mute_groups {
                s.mute_groups.push(build::mute_group(n, &format!("Mute {n}")));
            }
            s
        }
    };
    show.system.model = model.to_string();
    show
}

#[cfg(test)]
mod tests {
    use super::*;
    use songbook_model::{Bus, Channel, Preamp, Send};

    fn sq_show() -> Show {
        let mut show = Show::new("Gala", Platform::AhSq);
        show.system.model = "SQ-5".into();
        show.system.units.push(build::unit("local", "SQ-5 local", "SQ-5", UnitRole::Console));
        show.sockets.extend(build::sockets("local", Direction::In, 16, SocketKind::Mic, "Local"));
        show.system.units.push(build::unit("slink", "SLink", "", UnitRole::Network));
        show.sockets.push(build::socket("slink", Direction::In, 3, SocketKind::SLink, "SLink 3"));
        show.buses.push(Bus::new(BusKind::Main, 1, "LR", true));
        for n in 1..=12 {
            show.buses.push(Bus::new(BusKind::Aux, n, &format!("Aux{n}"), false));
        }
        for n in 1..=3 {
            show.buses.push(Bus::new(BusKind::Matrix, n, &format!("Mtx{n}"), true));
        }
        for n in 1..=8 {
            show.dcas.push(build::dca(n, &format!("DCA{n}")));
            show.mute_groups.push(build::mute_group(n, &format!("MGrp{n}")));
        }
        for n in 1..=48u32 {
            let mut c = Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Channel number {n}"));
            c.color = Some("white".into());
            c.source = Some(if n <= 16 { ids::socket("local", Direction::In, n) } else if n == 17 { ids::socket("slink", Direction::In, 3) } else { ids::socket("local", Direction::In, 1) });
            c.strip.fader_db = Some(-6.0);
            c.strip.on = Some(true);
            for a in 1..=12 {
                c.sends.push(Send { bus_id: ids::bus(BusKind::Aux, a), level_db: Some(-10.0), on: Some(true), pre: Some(true), pan: None });
            }
            c.dca_ids.push(ids::dca(8));
            c.mute_group_ids.push(ids::mute_group(7));
            show.channels.push(c);
        }
        show.preamps.push(Preamp { socket_id: ids::socket("local", Direction::In, 1), gain_db: Some(30.0), pad: None, phantom: Some(true), extra: Extra::new() });
        show.scenes.push(build::scene(1, "Opening"));
        show
    }

    #[test]
    fn sq_to_dm3_keeps_what_fits_and_reports_the_rest() {
        let src = sq_show();
        let c = convert(&src, Platform::YamahaDm3, "DM3");
        let out = &c.show;
        assert_eq!(out.platform, Platform::YamahaDm3);
        assert_eq!(out.channels.len(), 16);
        assert_eq!(out.channels[0].label, "Channel", "cut to 8 characters and trimmed");
        assert_eq!(out.channels[0].color.as_deref(), Some("off"), "Yamaha has no white; uncoloured is the nearest");
        assert_eq!(out.channels[0].sends.len(), 6);
        assert_eq!(out.channels[0].dca_ids.len(), 0, "the DM3 has no DCAs");
        assert_eq!(out.channels[0].mute_group_ids.len(), 0, "mute group 7 of 6");
        assert_eq!(out.buses_of(BusKind::Aux).count(), 6);
        assert_eq!(out.buses_of(BusKind::Matrix).count(), 2);
        assert_eq!(out.dcas.len(), 0);
        assert_eq!(out.mute_groups.len(), 6);
        assert_eq!(out.preamps.len(), 1);
        assert!(out.validate().is_empty(), "{:?}", out.validate());
        assert!(c.dropped > 30, "{}", c.dropped);
        assert!(c.notes.iter().any(|n| n.level == NoteLevel::Dropped && n.message.contains("16 input channels")));
        assert!(c.notes.iter().any(|n| n.level == NoteLevel::Adapted && n.message.contains("cut to")));
        assert!(c.notes.iter().any(|n| n.level == NoteLevel::Dropped && n.message.contains("has 6 auxs")));
    }

    #[test]
    fn sq_to_dlive_carries_everything_and_adapts_the_stage_box() {
        let src = sq_show();
        let c = convert(&src, Platform::AhDlive, "dLive S5000");
        let out = &c.show;
        assert_eq!(out.channels.len(), 48);
        assert_eq!(out.buses_of(BusKind::Aux).count(), 12);
        assert_eq!(out.dcas.len(), 8);
        assert_eq!(out.channels[16].source.as_deref(), Some("skt:network:in:3"));
        assert!(out.validate().is_empty(), "{:?}", out.validate());
        // The surface has 8 local inputs, so channels 9–16 lose their patch.
        assert_eq!(out.channels[8].source, None);
        assert!(c.notes.iter().any(|n| n.level == NoteLevel::Dropped && n.message.contains("8 local inputs")));
        assert!(c.notes.iter().any(|n| n.level == NoteLevel::Adapted && n.message.contains("network input 3")));
        assert!(c.notes.iter().filter(|n| n.level == NoteLevel::Dropped).count() < 12);
    }

    #[test]
    fn colours_map_to_the_nearest_the_target_has() {
        let mut src = sq_show();
        src.platform = Platform::YamahaDm3;
        src.channels[0].color = Some("orange".into());
        src.channels[1].color = Some("pink".into());
        let c = convert(&src, Platform::AhSq, "SQ-6");
        assert_eq!(c.show.channels[0].color.as_deref(), Some("yellow"));
        assert_eq!(c.show.channels[1].color.as_deref(), Some("purple"));
        assert!(c.notes.iter().any(|n| n.message.contains("orange is not in the SQ-6 palette")));
    }
}

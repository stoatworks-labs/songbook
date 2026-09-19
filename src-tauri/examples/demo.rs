//! `cargo run --example demo -- ../public/demo` — write the two example shows
//! the browser demo serves. They are authored here, not captured from a desk:
//! an SQ-5 band show and a DM3 corporate show, with the values a pull would
//! fill in.

use songbook_model::{build, ids, Bus, BusKind, Channel, ChannelKind, Direction, Extra, OutputPatch, OutputSource, OutputSourceKind, Platform, Preamp, Send, Show, SocketKind, UnitRole, FADER_OFF};

fn send(bus: &str, level: f64, pre: bool) -> Send {
    Send { bus_id: bus.into(), level_db: Some(level), on: Some(true), pre: Some(pre), pan: None }
}

fn sq5() -> Show {
    let mut show = songbook_ah::import::sq_skeleton("Summer Live 2026 — FOH", Platform::AhSq, "SQ-5");
    show.id = "demo-sq5-band".into();
    show.system.firmware = "1.6.0".into();
    show.system.name = "SQ-5 FOH".into();
    show.meta.tags = vec!["festival".into(), "band".into()];
    show.meta.notes = "Main stage, Saturday. Wedges on Aux 1–6, IEM stereo on Aux 7/8 + 9/10. FX1 hall, FX2 plate, FX3 tap delay.".into();
    let names: [(&str, &str, u32, f64); 22] = [
        ("Kick In", "red", 1, -6.0),
        ("Kick Out", "red", 2, -8.0),
        ("Snare Tp", "red", 3, -4.0),
        ("Snare Bt", "red", 4, -12.0),
        ("Hat", "red", 5, -10.0),
        ("Rack", "red", 6, -9.0),
        ("Floor", "red", 7, -7.0),
        ("OH L", "red", 8, -8.0),
        ("OH R", "red", 9, -8.0),
        ("Bass DI", "purple", 10, -3.0),
        ("Bass Mic", "purple", 11, -12.0),
        ("Gtr L", "yellow", 12, -6.0),
        ("Gtr R", "yellow", 13, -6.0),
        ("Keys L", "blue", 14, -10.0),
        ("Keys R", "blue", 15, -10.0),
        ("Vox Lead", "green", 16, 0.0),
        ("Vox BV1", "green", 17, -3.0),
        ("Vox BV2", "green", 18, -3.0),
        ("Acoustic", "yellow", 19, -8.0),
        ("Talkback", "white", 20, FADER_OFF),
        ("Spare 1", "off", 21, FADER_OFF),
        ("Spare 2", "off", 22, FADER_OFF),
    ];
    show.system.units.push(build::unit("input", "Input sockets", "", UnitRole::Console));
    for n in 1..=24u32 {
        show.sockets.push(build::socket("input", Direction::In, n, SocketKind::Mic, &format!("Input socket {n}")));
    }
    for (i, (name, color, socket, fader)) in names.iter().enumerate() {
        let c = &mut show.channels[i];
        c.label = name.to_string();
        c.color = if *color == "off" { None } else { Some(color.to_string()) };
        c.source = Some(songbook_ah::sq::socket_id(*socket));
        c.strip.fader_db = Some(*fader);
        c.strip.on = Some(*fader > FADER_OFF);
        c.strip.pan = Some(match *name {
            "OH L" | "Gtr L" | "Keys L" => -0.7,
            "OH R" | "Gtr R" | "Keys R" => 0.7,
            _ => 0.0,
        });
        c.strip.main_assign = Some(*name != "Talkback");
        c.strip.hpf = Some(songbook_model::Filter { on: !name.starts_with("Kick") && !name.starts_with("Bass"), freq_hz: Some(if name.starts_with("Vox") { 120.0 } else { 80.0 }) });
        c.strip.eq = Some(songbook_model::Eq { on: true, bands: vec![] });
        c.strip.gate = Some(songbook_model::Dynamics { on: name.starts_with("Kick") || name.starts_with("Snare") || matches!(*name, "Rack" | "Floor"), ..Default::default() });
        c.strip.comp = Some(songbook_model::Dynamics { on: name.starts_with("Vox") || name.starts_with("Bass") || name.starts_with("Kick"), ..Default::default() });
        for aux in 1..=6u32 {
            let lvl = if name.starts_with("Vox") { -6.0 } else if name.starts_with("Kick") || name.starts_with("Snare") { -10.0 } else { -14.0 };
            c.sends.push(send(&ids::bus(BusKind::Aux, aux), lvl, true));
        }
        for aux in 7..=10u32 {
            let lvl = if name.starts_with("Vox") { -4.0 } else { -12.0 };
            c.sends.push(send(&ids::bus(BusKind::Aux, aux), lvl, true));
        }
        if name.starts_with("Vox") {
            c.sends.push(send(&ids::bus(BusKind::FxSend, 1), -12.0, false));
            c.sends.push(send(&ids::bus(BusKind::FxSend, 2), -18.0, false));
        }
        if name.starts_with("Snare") {
            c.sends.push(send(&ids::bus(BusKind::FxSend, 2), -10.0, false));
        }
        let dca = match *name {
            n if n.starts_with("Kick") || n.starts_with("Snare") || matches!(n, "Hat" | "Rack" | "Floor" | "OH L" | "OH R") => 1,
            n if n.starts_with("Bass") => 2,
            n if n.starts_with("Gtr") || n == "Acoustic" => 3,
            n if n.starts_with("Keys") => 4,
            n if n.starts_with("Vox") => 5,
            _ => 0,
        };
        if dca > 0 {
            c.dca_ids.push(ids::dca(dca));
        }
        if name.starts_with("Vox") {
            c.mute_group_ids.push(ids::mute_group(1));
        }
        let pre = Preamp { socket_id: songbook_ah::sq::socket_id(*socket), gain_db: Some(match *name { n if n.starts_with("Vox") => 38.0, n if n.starts_with("OH") => 30.0, n if n.contains("DI") => 18.0, _ => 26.0 }), pad: Some(false), phantom: Some(name.starts_with("OH") || name.starts_with("Vox") || *name == "Acoustic" || name.contains("DI")), extra: Extra::new() };
        show.preamps.push(pre);
    }
    for (i, n) in ["Drums", "Bass", "Gtrs", "Keys", "Vox", "FX", "Band", "All"].iter().enumerate() {
        show.dcas[i].label = n.to_string();
        show.dcas[i].fader_db = Some(0.0);
        show.dcas[i].on = Some(true);
    }
    show.mute_groups[0].label = "Vox".into();
    show.mute_groups[1].label = "Band".into();
    show.mute_groups[2].label = "FX".into();
    for (i, (n, stereo)) in [("Wedge 1", false), ("Wedge 2", false), ("Wedge 3", false), ("Wedge 4", false), ("Drum Fill", false), ("Side Fill", false), ("IEM 1 L", false), ("IEM 1 R", false), ("IEM 2 L", false), ("IEM 2 R", false), ("Sub", false), ("Rec", true)].iter().enumerate() {
        if let Some(b) = show.buses.iter_mut().find(|b| b.kind == BusKind::Aux && b.number == i as u32 + 1) {
            b.label = n.to_string();
            b.stereo = *stereo;
            b.strip.fader_db = Some(0.0);
            b.strip.on = Some(true);
            b.color = Some(if n.starts_with("IEM") { "cyan".into() } else { "yellow".into() });
        }
    }
    for b in show.buses.iter_mut() {
        if b.kind == BusKind::Main {
            b.strip.fader_db = Some(-3.0);
            b.strip.on = Some(true);
            b.color = Some("white".into());
        }
        if b.kind == BusKind::FxSend {
            b.label = ["Hall", "Plate", "Tap Dly", "Chorus"][b.number as usize - 1].into();
            b.strip.fader_db = Some(0.0);
            b.strip.on = Some(true);
        }
        if b.kind == BusKind::Matrix {
            b.label = ["Fills", "Lobby", "Rec"][b.number as usize - 1].into();
            b.strip.fader_db = Some(0.0);
            b.strip.on = Some(true);
        }
        if b.kind == BusKind::Main || (b.kind == BusKind::Aux && b.number <= 6) {
            for m in 1..=3u32 {
                b.sends.push(send(&ids::bus(BusKind::Matrix, m), if b.kind == BusKind::Main { -6.0 } else { FADER_OFF }, false));
            }
        }
    }
    for (i, (label, src)) in [("LR L", ids::bus(BusKind::Main, 1)), ("LR R", ids::bus(BusKind::Main, 1)), ("Wedge 1", ids::bus(BusKind::Aux, 1)), ("Wedge 2", ids::bus(BusKind::Aux, 2)), ("Wedge 3", ids::bus(BusKind::Aux, 3)), ("Wedge 4", ids::bus(BusKind::Aux, 4)), ("Drum Fill", ids::bus(BusKind::Aux, 5)), ("Side Fill", ids::bus(BusKind::Aux, 6))].iter().enumerate() {
        show.output_patch.push(OutputPatch { socket_id: ids::socket("local", Direction::Out, i as u32 + 1), source: OutputSource { kind: OutputSourceKind::Bus, ref_id: Some(src.clone()), label: label.to_string() }, extra: Extra::new() });
    }
    for (n, label, notes) in [(1, "Line check", "All channels open, wedges at -10"), (2, "Opening act", "Acoustic + Vox Lead only"), (3, "Main act", "Full band"), (4, "Encore", "Main act + Tap Dly on Vox"), (5, "Walk out", "Playback only")] {
        let mut s = build::scene(n, label);
        s.notes = notes.into();
        show.scenes.push(s);
    }
    for (n, label, scene, notes) in [(1, "House open", 5, "Walk-in playlist on ST1"), (2, "Opening act on", 2, ""), (3, "Changeover", 1, "Line check scene; mute Vox"), (4, "Main act on", 3, "GO on the drum count"), (5, "Encore", 4, ""), (6, "House lights", 5, "")] {
        show.cues.push(songbook_model::Cue { id: ids::cue(n), number: Some(n), label: label.into(), notes: notes.into(), steps: vec![songbook_model::CueStep { kind: songbook_model::CueStepKind::RecallScene, scene_id: Some(ids::scene(scene)), delay_ms: None, extra: Extra::new() }], extra: Extra::new() });
    }
    show.notes.push(songbook_model::Note { level: songbook_model::NoteLevel::Info, path: "system".into(), message: "example show authored for the browser demo — not captured from a desk".into() });
    assert!(show.validate().is_empty(), "{:?}", show.validate());
    show
}

fn dm3() -> Show {
    let mut show = Show::new("Product launch — Ballroom A", Platform::YamahaDm3);
    show.id = "demo-dm3-corporate".into();
    show.system.model = "DM3".into();
    show.system.firmware = "V3.00".into();
    show.system.name = "Y001-Yamaha-DM3".into();
    show.system.sample_rate = Some(48000);
    show.meta.tags = vec!["corporate".into()];
    show.meta.notes = "Two presenters on headsets, panel of four on handhelds, playback from the show laptop over USB. Mix 1/2 = stage fills, Mix 3/4 = stream feed, Mix 5/6 = presenter IEM.".into();
    show.system.units.push(build::unit("local", "DM3 inputs", "DM3", UnitRole::Console));
    show.system.units.push(build::unit("stin", "Stereo line inputs", "DM3", UnitRole::Console));
    show.system.units.push(build::unit("omni", "OMNI outputs", "DM3", UnitRole::Console));
    show.system.units.push(build::unit("usb", "USB", "", UnitRole::Internal));
    show.sockets.extend(build::sockets("local", Direction::In, 16, SocketKind::Mic, "INPUT"));
    show.sockets.extend(build::sockets("stin", Direction::In, 2, SocketKind::Line, "ST IN"));
    show.sockets.extend(build::sockets("omni", Direction::Out, 8, SocketKind::Line, "OMNI"));
    show.sockets.extend(build::sockets("usb", Direction::Out, 4, SocketKind::Usb, "USB"));
    let names: [(&str, &str, f64, f64, bool); 16] = [
        ("Lectern", "yellow", -3.0, 42.0, true),
        ("Headset1", "yellow", -2.0, 44.0, true),
        ("Headset2", "yellow", -2.0, 44.0, true),
        ("HH 1", "orange", -4.0, 30.0, false),
        ("HH 2", "orange", -4.0, 30.0, false),
        ("HH 3", "orange", -4.0, 30.0, false),
        ("HH 4", "orange", -4.0, 30.0, false),
        ("Q&A 1", "green", FADER_OFF, 36.0, false),
        ("Q&A 2", "green", FADER_OFF, 36.0, false),
        ("Laptop L", "cyan", -10.0, 0.0, false),
        ("Laptop R", "cyan", -10.0, 0.0, false),
        ("Video L", "cyan", -12.0, 0.0, false),
        ("Video R", "cyan", -12.0, 0.0, false),
        ("Walk In", "purple", FADER_OFF, 0.0, false),
        ("Stinger", "purple", FADER_OFF, 0.0, false),
        ("Spare", "off", FADER_OFF, 20.0, false),
    ];
    for (i, (name, color, fader, gain, phantom)) in names.iter().enumerate() {
        let n = i as u32 + 1;
        let mut c = Channel::new(ids::channel(n), n, ChannelKind::Input, name);
        c.color = if *color == "off" { None } else { Some(color.to_string()) };
        c.source = Some(ids::socket("local", Direction::In, n));
        c.strip.fader_db = Some(*fader);
        c.strip.on = Some(*fader > FADER_OFF);
        c.strip.pan = Some(if name.ends_with(" L") { -1.0 } else if name.ends_with(" R") { 1.0 } else { 0.0 });
        c.strip.main_assign = Some(true);
        c.strip.eq = Some(songbook_model::Eq { on: true, bands: vec![] });
        c.strip.comp = Some(songbook_model::Dynamics { on: n <= 9, ..Default::default() });
        c.strip.gate = Some(songbook_model::Dynamics { on: false, ..Default::default() });
        for mix in 1..=6u32 {
            let lvl = match (mix, n) {
                (1 | 2, _) => -6.0,
                (3 | 4, _) => 0.0,
                (5 | 6, 1..=9) => -3.0,
                _ => FADER_OFF,
            };
            c.sends.push(Send { bus_id: ids::bus(BusKind::Aux, mix), level_db: Some(lvl), on: Some(true), pre: Some(mix >= 5), pan: None });
        }
        if n <= 9 {
            c.mute_group_ids.push(ids::mute_group(1));
        } else if n <= 15 {
            c.mute_group_ids.push(ids::mute_group(2));
        }
        c.extra.insert("yamahaIcon".into(), if n <= 3 { "Headset" } else if n <= 9 { "DynamicMic" } else { "Audio" }.into());
        show.channels.push(c);
        show.preamps.push(Preamp { socket_id: ids::socket("local", Direction::In, n), gain_db: Some(*gain), pad: None, phantom: Some(*phantom), extra: Extra::new() });
    }
    for (i, (name, stereo_link)) in [("Fill L", "L"), ("Fill R", "R"), ("Stream L", "L"), ("Stream R", "R"), ("IEM Pres", ""), ("IEM Host", "")].iter().enumerate() {
        let mut b = Bus::new(BusKind::Aux, i as u32 + 1, name, false);
        b.strip.fader_db = Some(0.0);
        b.strip.on = Some(true);
        b.color = Some(if name.starts_with("IEM") { "cyan".into() } else { "yellow".into() });
        if !stereo_link.is_empty() {
            b.extra.insert("yamahaLink".into(), (*stereo_link).into());
        }
        b.extra.insert("yamahaNumber".into(), (i as u64 + 1).into());
        show.buses.push(b);
    }
    for (i, name) in ["Lobby", "Record"].iter().enumerate() {
        let mut b = Bus::new(BusKind::Matrix, i as u32 + 1, name, false);
        b.strip.fader_db = Some(-6.0);
        b.strip.on = Some(true);
        b.extra.insert("yamahaNumber".into(), (i as u64 + 1).into());
        show.buses.push(b);
    }
    let mut st = Bus::new(BusKind::Main, 1, "Stereo", true);
    st.strip.fader_db = Some(-2.0);
    st.strip.on = Some(true);
    st.color = Some("white".into());
    show.buses.push(st);
    for (i, name) in ["Rev Hall", "Delay"].iter().enumerate() {
        let mut b = Bus::new(BusKind::FxSend, i as u32 + 1, name, true);
        b.strip.fader_db = Some(0.0);
        b.strip.on = Some(true);
        show.buses.push(b);
    }
    for (n, name) in [(1, "Mics"), (2, "Playback"), (3, "Q&A"), (4, ""), (5, ""), (6, "")] {
        let label = if name.is_empty() { format!("Mute Group {n}") } else { name.to_string() };
        let mut m = build::mute_group(n, &label);
        m.on = Some(n == 3);
        show.mute_groups.push(m);
    }
    for (i, (label, src)) in [("ST L", ids::bus(BusKind::Main, 1)), ("ST R", ids::bus(BusKind::Main, 1)), ("Fill L", ids::bus(BusKind::Aux, 1)), ("Fill R", ids::bus(BusKind::Aux, 2)), ("Stream L", ids::bus(BusKind::Aux, 3)), ("Stream R", ids::bus(BusKind::Aux, 4)), ("Lobby", ids::bus(BusKind::Matrix, 1)), ("Record", ids::bus(BusKind::Matrix, 2))].iter().enumerate() {
        show.output_patch.push(OutputPatch { socket_id: ids::socket("omni", Direction::Out, i as u32 + 1), source: OutputSource { kind: OutputSourceKind::Bus, ref_id: Some(src.clone()), label: label.to_string() }, extra: Extra::new() });
    }
    for (n, bank, label, notes) in [(1, "A", "Doors", "Walk In at -12, mics muted"), (2, "A", "Welcome", "Lectern + Headset1"), (3, "A", "Keynote", "Headset2, video playback"), (4, "A", "Panel", "HH 1-4, Q&A open"), (5, "A", "Close", "Stinger, Walk In"), (1, "B", "Rehearsal", "Everything open, stream muted")] {
        let mut s = build::scene(n, label);
        s.id = ids::scene_in_bank(bank, n);
        s.bank = Some(bank.into());
        s.notes = notes.into();
        s.extra.insert("scpList".into(), format!("scene_{}", bank.to_lowercase()).into());
        show.scenes.push(s);
    }
    show.system.extra.insert("currentScene".into(), "scene_a 2".into());
    show.notes.push(songbook_model::Note { level: songbook_model::NoteLevel::Info, path: "system".into(), message: "example show authored for the browser demo — not captured from a desk".into() });
    show.notes.push(songbook_model::Note { level: songbook_model::NoteLevel::Info, path: "preamps".into(), message: "on a DM3 head-amp gain is global (not part of a scene): a scene recall does not move it".into() });
    assert!(show.validate().is_empty(), "{:?}", show.validate());
    show
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "../public/demo".into());
    std::fs::create_dir_all(&out).unwrap();
    for (name, show) in [("sq5-band", sq5()), ("dm3-corporate", dm3())] {
        let path = format!("{out}/{name}.json");
        std::fs::write(&path, serde_json::to_string_pretty(&show).unwrap()).unwrap();
        println!("{path}: {} channels, {} buses, {} scenes", show.channels.len(), show.buses.len(), show.scenes.len());
    }
}

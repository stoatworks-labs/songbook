//! A live Allen & Heath desk over MIDI/TCP.
//!
//! One blocking TCP connection (port 51325), a reader thread that parses the
//! byte stream into [`Event`]s, and request/collect helpers on top. Every
//! read is a burst of `get` messages followed by a collection window: the
//! desk answers each get with the same message it would send for a change,
//! so replies and spontaneous updates look alike and both are welcome.
//!
//! **Nothing here has been run against a desk yet.** The message shapes are
//! the published ones (SQ MIDI Protocol Issue 5; dLive MIDI over TCP/IP V2.0;
//! Avantis MIDI TCP/IP for V2.0) and the codec is tested against the worked
//! examples in those documents and against an in-process fake desk.

use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use songbook_model::{build, ids, BusKind, Channel, ChannelKind, Direction, NoteLevel, Platform, Preamp, Send as ModelSend, Show, SocketKind, UnitRole};

use crate::dlive::{self, Family, Target};
use crate::midi::{self, Event, Parser};
use crate::sq::{self, Strip};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// How the desk is addressed.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// The desk's MIDI channel (1–16) as set in its Utility / MIDI screen —
    /// the base channel N on dLive / Avantis.
    #[serde(default = "one")]
    pub midi_channel: u8,
    /// Milliseconds of silence that end a collection window.
    #[serde(default = "quiet")]
    pub quiet_ms: u64,
}

fn one() -> u8 {
    1
}
fn quiet() -> u64 {
    350
}

impl Default for Options {
    fn default() -> Options {
        Options { midi_channel: 1, quiet_ms: 350 }
    }
}

pub struct Device {
    pub platform: Platform,
    pub host: String,
    stream: TcpStream,
    rx: Receiver<Event>,
    n: u8,
    quiet: Duration,
}

impl Device {
    pub fn connect(host: &str, platform: Platform, opts: &Options) -> Result<Device> {
        let addr = if host.contains(':') { host.to_string() } else { format!("{host}:{}", midi::PORT) };
        let sa = addr.to_socket_addrs()?.next().ok_or_else(|| Error::Other(format!("cannot resolve {addr}")))?;
        let stream = TcpStream::connect_timeout(&sa, Duration::from_secs(4))?;
        stream.set_nodelay(true)?;
        let mut reader = stream.try_clone()?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut parser = Parser::new();
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        for e in parser.feed(&buf[..n]) {
                            if tx.send(e).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Ok(Device { platform, host: host.to_string(), stream, rx, n: opts.midi_channel.clamp(1, 16) - 1, quiet: Duration::from_millis(opts.quiet_ms) })
    }

    fn family(&self) -> Option<Family> {
        match self.platform {
            Platform::AhDlive => Some(Family::Dlive),
            Platform::AhAvantis => Some(Family::Avantis),
            _ => None,
        }
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.stream.write_all(bytes)?;
        Ok(())
    }

    /// Everything the desk says until it has been quiet for the window, or
    /// `max` has passed.
    pub fn collect(&self, max: Duration) -> Vec<Event> {
        let start = Instant::now();
        let mut out = vec![];
        loop {
            match self.rx.recv_timeout(self.quiet) {
                Ok(e) => out.push(e),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if start.elapsed() > max {
                break;
            }
        }
        out
    }

    /// Drop anything queued.
    fn drain(&self) {
        while self.rx.try_recv().is_ok() {}
    }

    // ------------------------------------------------------------ probe

    /// A cheap round trip: the LR mute on an SQ / Qu, input 1's name on a
    /// dLive / Avantis. Returns what came back.
    pub fn probe(&mut self) -> Result<serde_json::Value> {
        self.drain();
        match self.family() {
            None => {
                self.send(&midi::nrpn_get(self.n, sq::mute_param(Strip::Lr)))?;
                let ev = self.collect(Duration::from_secs(2));
                let mute = ev.iter().find_map(|e| match e {
                    Event::Nrpn { param, fine: Some(f), .. } if *param == sq::mute_param(Strip::Lr) => Some(*f == 1),
                    _ => None,
                });
                match mute {
                    Some(m) => Ok(serde_json::json!({"ok": true, "platform": self.platform, "lrMuted": m, "events": ev.len()})),
                    None => Err(Error::Other(format!("connected to {} but no reply to an LR mute get on MIDI channel {} in 2 s — check the desk's MIDI channel", self.host, self.n + 1))),
                }
            }
            Some(_) => {
                self.send(&midi::sysex(self.n, &[0x01, 0x00]))?;
                let ev = self.collect(Duration::from_secs(2));
                let name = ev.iter().find_map(|e| match e {
                    Event::SysEx { body } if body.len() >= 3 && body[1] == 0x02 && body[2] == 0x00 => Some(String::from_utf8_lossy(&body[3..]).trim().to_string()),
                    _ => None,
                });
                match name {
                    Some(nm) => Ok(serde_json::json!({"ok": true, "platform": self.platform, "input1": nm, "events": ev.len()})),
                    None => Err(Error::Other(format!("connected to {} but no reply to a name request on base MIDI channel {} in 2 s — check the desk's MIDI channel", self.host, self.n + 1))),
                }
            }
        }
    }

    // ------------------------------------------------------------ pull

    /// Read the whole mix into a show.
    pub fn pull(&mut self, model: &str) -> Result<Show> {
        match self.family() {
            None => self.pull_sq(model),
            Some(f) => self.pull_dlive(f, model),
        }
    }

    fn pull_sq(&mut self, model: &str) -> Result<Show> {
        let mut show = crate::import::sq_skeleton(&format!("{} pull", self.host), Platform::AhSq, model);
        show.meta.source = Some(songbook_model::SourceInfo { kind: "device".into(), origin: self.host.clone(), at: songbook_model::now(), firmware: None });
        let table = SqTable::new();
        self.drain();
        // Everything at once, in the order the tables list them; the desk answers each.
        let mut batch = Vec::with_capacity(table.by_param.len() * 9);
        for p in table.by_param.keys() {
            batch.extend_from_slice(&midi::nrpn_get(self.n, *p));
        }
        for chunk in batch.chunks(4096) {
            self.send(chunk)?;
        }
        let events = self.collect(Duration::from_secs(20));
        let mut answered = 0usize;
        for e in &events {
            let Event::Nrpn { param, coarse, fine: Some(fine), .. } = e else { continue };
            let Some(entry) = table.by_param.get(param) else { continue };
            answered += 1;
            let v14 = ((*coarse as u16) << 7) | *fine as u16;
            apply_sq(&mut show, entry, v14);
        }
        if answered == 0 {
            return Err(Error::Other(format!("the desk at {} answered none of {} parameter requests on MIDI channel {}; check the MIDI channel in Utility → General → MIDI", self.host, table.by_param.len(), self.n + 1)));
        }
        show.note(NoteLevel::Info, "channels", format!("pulled {answered} of {} parameters over MIDI/TCP (mutes, levels, pans, assignments); the SQ protocol carries no channel names or colours, so the strips keep their default labels — import the show folder or name them here", table.by_param.len()));
        show.note(NoteLevel::Info, "preamps", "the SQ MIDI protocol has no preamp messages; gain, pad and phantom are not read");
        show.note(NoteLevel::Info, "scenes", "the SQ MIDI protocol lists no scenes; recall by number from the Scenes tab");
        Ok(show)
    }

    fn pull_dlive(&mut self, family: Family, model: &str) -> Result<Show> {
        let l = family.limits();
        let mut show = Show::new(&format!("{} pull", self.host), self.platform);
        show.system.model = model.to_string();
        show.meta.source = Some(songbook_model::SourceInfo { kind: "device".into(), origin: self.host.clone(), at: songbook_model::now(), firmware: None });
        show.system.units.push(build::unit("mixrack", "MixRack sockets", "", UnitRole::MixRack));
        self.drain();

        // Names and colours of everything that can have one; a strip that
        // does not exist in the mix config never answers.
        let mut targets: Vec<Target> = vec![];
        for n in 1..=l.inputs {
            targets.push(Target::Input(n));
        }
        for n in 1..=l.mono_groups {
            targets.push(Target::MonoGroup(n));
        }
        for n in 1..=l.stereo_groups {
            targets.push(Target::StereoGroup(n));
        }
        for n in 1..=l.mono_auxes {
            targets.push(Target::MonoAux(n));
        }
        for n in 1..=l.stereo_auxes {
            targets.push(Target::StereoAux(n));
        }
        for n in 1..=l.mono_matrices {
            targets.push(Target::MonoMatrix(n));
        }
        for n in 1..=l.stereo_matrices {
            targets.push(Target::StereoMatrix(n));
        }
        for n in 1..=l.mono_fx_sends {
            targets.push(Target::MonoFxSend(n));
        }
        for n in 1..=l.stereo_fx_sends {
            targets.push(Target::StereoFxSend(n));
        }
        for n in 1..=l.fx_returns {
            targets.push(Target::FxReturn(n));
        }
        for n in 1..=l.mains {
            targets.push(Target::Main(n));
        }
        for n in 1..=l.dcas {
            targets.push(Target::Dca(n));
        }
        for n in 1..=l.mute_groups {
            targets.push(Target::MuteGroup(n));
        }
        let mut batch = vec![];
        for t in &targets {
            let (o, note) = t.address(family).unwrap();
            let ch = (self.n + o) & 0x0F;
            batch.extend_from_slice(&midi::sysex(ch, &[0x01, note])); // name
            batch.extend_from_slice(&midi::sysex(ch, &[0x04, note])); // colour
            batch.extend_from_slice(&midi::sysex(ch, &[0x05, 0x09, note])); // mute
            batch.extend_from_slice(&midi::sysex(ch, &[0x05, 0x0B, 0x17, note])); // fader
            if matches!(t, Target::Input(_)) {
                batch.extend_from_slice(&midi::sysex(ch, &[0x05, 0x0B, 0x18, note])); // main assign
            }
        }
        for chunk in batch.chunks(4096) {
            self.send(chunk)?;
        }
        let events = self.collect(Duration::from_secs(30));
        /// name, colour code, muted, fader level, main assign — as each answers.
        type Answers = (Option<String>, Option<u8>, Option<bool>, Option<u8>, Option<bool>);
        let mut named: HashMap<Target, Answers> = HashMap::new();
        for e in &events {
            match e {
                Event::SysEx { body } if body.len() >= 3 => {
                    let off = body[0].wrapping_sub(self.n) & 0x0F;
                    let opcode = body[1];
                    match opcode {
                        0x02 => {
                            if let Some(t) = Target::from_address(family, off, body[2]) {
                                named.entry(t).or_default().0 = Some(String::from_utf8_lossy(&body[3..]).trim().to_string());
                            }
                        }
                        0x05 if body.len() >= 4 => {
                            if let Some(t) = Target::from_address(family, off, body[2]) {
                                named.entry(t).or_default().1 = Some(body[3]);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Note { channel, note, velocity } => {
                    let off = channel.wrapping_sub(self.n) & 0x0F;
                    if let Some(t) = Target::from_address(family, off, *note) {
                        if *velocity != 0 {
                            named.entry(t).or_default().2 = Some(*velocity >= 0x40);
                        }
                    }
                }
                Event::Nrpn { channel, param, coarse, .. } => {
                    let off = channel.wrapping_sub(self.n) & 0x0F;
                    let (note, id) = midi::split14(*param);
                    if let Some(t) = Target::from_address(family, off, note) {
                        match id {
                            0x17 => named.entry(t).or_default().3 = Some(*coarse),
                            0x18 => named.entry(t).or_default().4 = Some(*coarse >= 0x40),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if named.is_empty() {
            return Err(Error::Other(format!("the desk at {} answered nothing on base MIDI channel {}; check Utility → Control → MIDI", self.host, self.n + 1)));
        }
        // Build the model from what answered.
        let mut order: Vec<Target> = targets.iter().copied().filter(|t| named.contains_key(t)).collect();
        order.sort_by_key(|t| t.address(family));
        for t in &order {
            let (name, color, muted, lv, main) = named.get(t).cloned().unwrap_or_default();
            let color = color.and_then(dlive::color_name).map(str::to_string);
            let fader = lv.map(dlive::level_to_db);
            let on = muted.map(|m| !m);
            match t {
                Target::Input(n) => {
                    let mut c = Channel::new(ids::channel(*n), *n, ChannelKind::Input, &name.clone().unwrap_or(format!("Ip {n}")));
                    c.color = color;
                    c.strip.fader_db = fader;
                    c.strip.on = on;
                    c.strip.main_assign = main;
                    show.channels.push(c);
                }
                Target::FxReturn(n) => {
                    let mut c = Channel::new(ids::fx_return(*n), *n, ChannelKind::FxReturn, &name.clone().unwrap_or(format!("FX Rtn {n}")));
                    c.stereo = true;
                    c.color = color;
                    c.strip.fader_db = fader;
                    c.strip.on = on;
                    show.channels.push(c);
                }
                Target::Dca(n) => {
                    let mut d = build::dca(*n, &name.clone().unwrap_or(format!("DCA {n}")));
                    d.color = color;
                    d.fader_db = fader;
                    d.on = on;
                    show.dcas.push(d);
                }
                Target::MuteGroup(n) => {
                    let mut m = build::mute_group(*n, &name.clone().unwrap_or(format!("Mute Group {n}")));
                    m.on = muted;
                    show.mute_groups.push(m);
                }
                other => {
                    let (kind, default) = match other {
                        Target::MonoGroup(n) | Target::StereoGroup(n) => (BusKind::Group, format!("Grp {n}")),
                        Target::MonoAux(n) | Target::StereoAux(n) => (BusKind::Aux, format!("Aux {n}")),
                        Target::MonoMatrix(n) | Target::StereoMatrix(n) => (BusKind::Matrix, format!("Mtx {n}")),
                        Target::MonoFxSend(n) | Target::StereoFxSend(n) => (BusKind::FxSend, format!("FX {n}")),
                        Target::Main(n) => (BusKind::Main, if *n == 1 { "Main LR".to_string() } else { format!("Main {n}") }),
                        _ => unreachable!(),
                    };
                    let desk_number = ids::number(&other.model_id()).unwrap_or(0);
                    let number = if other.is_stereo() && kind != BusKind::Main { desk_number + show.buses.iter().filter(|b| b.kind == kind && !b.stereo).count() as u32 } else { desk_number };
                    let mut b = songbook_model::Bus::new(kind, number, &name.clone().unwrap_or(default), other.is_stereo() || kind == BusKind::Main);
                    b.color = color;
                    b.strip.fader_db = fader;
                    b.strip.on = on;
                    b.extra.insert("ahTarget".into(), format!("{other:?}").into());
                    show.buses.push(b);
                }
            }
        }

        // Sends from every input into every bus that exists.
        let send_targets: Vec<Target> = order.iter().copied().filter(|t| matches!(t, Target::MonoAux(_) | Target::StereoAux(_) | Target::MonoFxSend(_) | Target::StereoFxSend(_) | Target::MonoMatrix(_) | Target::StereoMatrix(_) | Target::MonoGroup(_) | Target::StereoGroup(_))).collect();
        let inputs: Vec<u32> = order.iter().filter_map(|t| if let Target::Input(n) = t { Some(*n) } else { None }).collect();
        if !send_targets.is_empty() && !inputs.is_empty() {
            self.drain();
            let mut batch = vec![];
            for &i in &inputs {
                let (_, ch_note) = Target::Input(i).address(family).unwrap();
                for st in &send_targets {
                    let (so, sn) = st.address(family).unwrap();
                    batch.extend_from_slice(&midi::sysex(self.n, &[0x05, 0x0F, 0x0D, ch_note, (self.n + so) & 0x0F, sn]));
                }
            }
            for chunk in batch.chunks(4096) {
                self.send(chunk)?;
            }
            let events = self.collect(Duration::from_secs(60));
            let mut got = 0usize;
            for e in &events {
                let Event::SysEx { body } = e else { continue };
                if body.len() >= 6 && body[1] == 0x0D {
                    let (ch_note, snd_n, snd_ch, lv) = (body[2], body[3], body[4], body[5]);
                    let Some(Target::Input(i)) = Target::from_address(family, 0, ch_note) else { continue };
                    let Some(st) = Target::from_address(family, snd_n.wrapping_sub(self.n) & 0x0F, snd_ch) else { continue };
                    let bus_id = bus_id_for(&show, st);
                    if let (Some(bus_id), Some(c)) = (bus_id, show.channels.iter_mut().find(|c| c.kind == ChannelKind::Input && c.number == i)) {
                        let s = c.send_mut(&bus_id);
                        s.level_db = Some(dlive::level_to_db(lv));
                        got += 1;
                    }
                }
            }
            show.note(NoteLevel::Info, "channels", format!("read {got} send levels from {} inputs into {} buses", inputs.len(), send_targets.len()));
        }

        // dLive preamps on the MixRack sockets.
        if family == Family::Dlive {
            self.drain();
            let mut batch = vec![];
            for mp in 0u8..64 {
                batch.extend_from_slice(&midi::sysex(self.n, &[0x05, 0x0B, 0x19, mp]));
                batch.extend_from_slice(&midi::sysex(self.n, &[0x07, mp]));
                batch.extend_from_slice(&midi::sysex(self.n, &[0x0A, mp]));
            }
            self.send(&batch)?;
            let events = self.collect(Duration::from_secs(10));
            let mut pre: HashMap<u8, Preamp> = HashMap::new();
            for e in &events {
                match e {
                    Event::PitchBend { lsb, msb, .. } => {
                        pre.entry(*lsb).or_insert_with(|| Preamp { socket_id: String::new(), ..Default::default() }).gain_db = Some(dlive::gain_to_db(*msb));
                    }
                    Event::SysEx { body } if body.len() >= 4 && body[1] == 0x08 => {
                        pre.entry(body[2]).or_insert_with(|| Preamp { socket_id: String::new(), ..Default::default() }).pad = Some(body[3] >= 0x40);
                    }
                    Event::SysEx { body } if body.len() >= 4 && body[1] == 0x0B => {
                        pre.entry(body[2]).or_insert_with(|| Preamp { socket_id: String::new(), ..Default::default() }).phantom = Some(body[3] >= 0x40);
                    }
                    _ => {}
                }
            }
            let mut keys: Vec<u8> = pre.keys().copied().collect();
            keys.sort();
            for mp in keys {
                let (unit, idx) = dlive::preamp_socket_unit(mp);
                let sid = ids::socket(unit, Direction::In, idx);
                if show.socket(&sid).is_none() {
                    show.sockets.push(build::socket(unit, Direction::In, idx, SocketKind::Mic, &format!("MixRack {idx}")));
                }
                let mut p = pre.remove(&mp).unwrap();
                p.socket_id = sid;
                show.preamps.push(p);
            }
            if show.preamps.is_empty() {
                show.note(NoteLevel::Info, "preamps", "no preamp replies from the MixRack sockets");
            }
        } else {
            show.note(NoteLevel::Info, "preamps", "the Avantis MIDI protocol has no preamp messages; gain, pad and phantom are not read");
        }
        show.note(NoteLevel::Info, "channels", "the input patch is not in the MIDI protocol; import the show archive for the patch");
        show.note(NoteLevel::Info, "scenes", "the MIDI protocol lists no scenes; recall by number from the Scenes tab");
        Ok(show)
    }

    // ------------------------------------------------------------ writes

    pub fn recall_scene(&mut self, scene: u32) -> Result<()> {
        let (bank, prog) = midi::scene_bank_program(scene);
        self.send(&midi::scene_recall(self.n, bank, prog))
    }

    /// dLive surface cue list recall by recall id (0-based, 16 banks).
    pub fn recall_cue(&mut self, recall_id: u32) -> Result<()> {
        self.send(&midi::scene_recall(self.n, (recall_id / 128) as u8, (recall_id % 128) as u8))
    }

    /// Write what the protocol can carry from the show: mutes, faders,
    /// pans/assignments and sends (SQ); names, colours, mutes, faders, main
    /// assignments, sends and (dLive) preamps. Returns a log.
    pub fn push(&mut self, show: &Show, what: &PushWhat) -> Result<Vec<String>> {
        let mut log = vec![];
        let mut bytes = vec![];
        match self.family() {
            None => {
                let table = SqTable::new();
                let mut count = 0usize;
                for (param, entry) in &table.by_param {
                    if let Some(v) = value_for_sq(show, entry, what) {
                        bytes.extend_from_slice(&match entry.kind {
                            SqKind::Mute | SqKind::Assign => midi::nrpn_flag(self.n, *param, v != 0),
                            _ => midi::nrpn14(self.n, *param, v),
                        });
                        count += 1;
                    }
                }
                log.push(format!("{count} parameters written over NRPN"));
                if what.names {
                    log.push("names and colours cannot be written to an SQ over MIDI (the protocol has no message for them)".into());
                }
            }
            Some(family) => {
                let mut count = 0usize;
                /// target, name, colour, on, fader dB, main assign
                type Write = (Target, Option<String>, Option<String>, Option<bool>, Option<f64>, Option<bool>);
                let mut targets: Vec<Write> = vec![];
                for c in &show.channels {
                    let t = match c.kind {
                        ChannelKind::Input => Target::Input(c.number),
                        ChannelKind::FxReturn => Target::FxReturn(c.number),
                        ChannelKind::StereoInput => continue,
                    };
                    targets.push((t, Some(c.label.clone()), c.color.clone(), c.strip.on, c.strip.fader_db, c.strip.main_assign));
                }
                for b in &show.buses {
                    let Some(t) = target_for_bus(b) else { continue };
                    targets.push((t, Some(b.label.clone()), b.color.clone(), b.strip.on, b.strip.fader_db, None));
                }
                for d in &show.dcas {
                    targets.push((Target::Dca(d.number), Some(d.label.clone()), d.color.clone(), d.on, d.fader_db, None));
                }
                for (t, name, color, on, fader, main) in targets {
                    let Some((o, note)) = t.address(family) else { continue };
                    let ch = (self.n + o) & 0x0F;
                    if what.names {
                        if let Some(nm) = name {
                            let mut body = vec![0x03, note];
                            body.extend(nm.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(8).map(|c| c as u8));
                            bytes.extend_from_slice(&midi::sysex(ch, &body));
                            count += 1;
                        }
                        if let Some(code) = color.as_deref().and_then(dlive::color_code) {
                            bytes.extend_from_slice(&midi::sysex(ch, &[0x06, note, code]));
                            count += 1;
                        }
                    }
                    if what.mutes {
                        if let Some(on) = on {
                            bytes.extend_from_slice(&midi::note_pulse(ch, note, if on { 0x3F } else { 0x7F }));
                            count += 1;
                        }
                    }
                    if what.levels {
                        if let Some(db) = fader {
                            bytes.extend_from_slice(&midi::nrpn7(ch, note, 0x17, dlive::db_to_level(db)));
                            count += 1;
                        }
                        if let Some(m) = main {
                            bytes.extend_from_slice(&midi::nrpn7(ch, note, 0x18, if m { 0x7F } else { 0x3F }));
                            count += 1;
                        }
                    }
                }
                if what.sends {
                    for c in show.channels.iter().filter(|c| c.kind == ChannelKind::Input) {
                        let Some((_, ch_note)) = Target::Input(c.number).address(family) else { continue };
                        for s in &c.sends {
                            let Some(bus) = show.bus(&s.bus_id) else { continue };
                            let Some(st) = target_for_bus(bus) else { continue };
                            let Some((so, sn)) = st.address(family) else { continue };
                            if let Some(db) = s.level_db {
                                bytes.extend_from_slice(&midi::sysex(self.n, &[0x0D, ch_note, (self.n + so) & 0x0F, sn, dlive::db_to_level(db)]));
                                count += 1;
                            }
                        }
                    }
                }
                if what.preamps && family == Family::Dlive {
                    for p in &show.preamps {
                        let Some(sk) = show.socket(&p.socket_id) else { continue };
                        let unit = sk.unit_id.trim_start_matches("unit:");
                        let Some(mp) = dlive::preamp_socket(unit, sk.index) else { continue };
                        if let Some(g) = p.gain_db {
                            bytes.extend_from_slice(&midi::pitch_bend(self.n, mp, dlive::db_to_gain(g)));
                            count += 1;
                        }
                        if let Some(pad) = p.pad {
                            bytes.extend_from_slice(&midi::sysex(self.n, &[0x09, mp, if pad { 0x7F } else { 0x00 }]));
                            count += 1;
                        }
                        if let Some(ph) = p.phantom {
                            bytes.extend_from_slice(&midi::sysex(self.n, &[0x0C, mp, if ph { 0x7F } else { 0x00 }]));
                            count += 1;
                        }
                    }
                }
                log.push(format!("{count} messages written"));
            }
        }
        for chunk in bytes.chunks(4096) {
            self.send(chunk)?;
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(log)
    }
}

/// What a push writes.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PushWhat {
    pub names: bool,
    pub mutes: bool,
    pub levels: bool,
    pub sends: bool,
    pub preamps: bool,
}

fn target_for_bus(b: &songbook_model::Bus) -> Option<Target> {
    // A pulled bus remembers its desk target; an imported or converted one
    // is addressed by kind, number and stereo flag.
    if let Some(t) = b.extra.get("ahTarget").and_then(|v| v.as_str()) {
        if let Some(t) = parse_target(t) {
            return Some(t);
        }
    }
    let n = b.extra.get("ahNumber").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(b.number);
    Some(match (b.kind, b.stereo) {
        (BusKind::Main, _) => Target::Main(n),
        (BusKind::Aux, false) => Target::MonoAux(n),
        (BusKind::Aux, true) => Target::StereoAux(n),
        (BusKind::Group, false) => Target::MonoGroup(n),
        (BusKind::Group, true) => Target::StereoGroup(n),
        (BusKind::Matrix, false) => Target::MonoMatrix(n),
        (BusKind::Matrix, true) => Target::StereoMatrix(n),
        (BusKind::FxSend, false) => Target::MonoFxSend(n),
        (BusKind::FxSend, true) => Target::StereoFxSend(n),
        (BusKind::Other, _) => return None,
    })
}

fn parse_target(s: &str) -> Option<Target> {
    let (name, num) = s.trim_end_matches(')').split_once('(')?;
    let n: u32 = num.parse().ok()?;
    Some(match name {
        "Input" => Target::Input(n),
        "MonoGroup" => Target::MonoGroup(n),
        "StereoGroup" => Target::StereoGroup(n),
        "MonoAux" => Target::MonoAux(n),
        "StereoAux" => Target::StereoAux(n),
        "MonoMatrix" => Target::MonoMatrix(n),
        "StereoMatrix" => Target::StereoMatrix(n),
        "MonoFxSend" => Target::MonoFxSend(n),
        "StereoFxSend" => Target::StereoFxSend(n),
        "FxReturn" => Target::FxReturn(n),
        "Main" => Target::Main(n),
        "Dca" => Target::Dca(n),
        "MuteGroup" => Target::MuteGroup(n),
        _ => return None,
    })
}

fn bus_id_for(show: &Show, t: Target) -> Option<String> {
    let key = format!("{t:?}");
    show.buses.iter().find(|b| b.extra.get("ahTarget").and_then(|v| v.as_str()) == Some(key.as_str())).map(|b| b.id.clone())
}

// ---------------------------------------------------------------- SQ tables

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqKind {
    Mute,
    Level,
    Pan,
    Assign,
}

#[derive(Debug, Clone, Copy)]
struct SqEntry {
    kind: SqKind,
    from: Strip,
    to: Option<Strip>,
}

/// Every parameter the protocol lists, keyed by number.
struct SqTable {
    by_param: std::collections::BTreeMap<u16, SqEntry>,
}

impl SqTable {
    fn new() -> SqTable {
        let mut by_param = std::collections::BTreeMap::new();
        let mut sources: Vec<Strip> = vec![];
        for n in 1..=sq::INPUT_CHANNELS as u32 {
            sources.push(Strip::Input(n));
        }
        for n in 1..=sq::GROUPS {
            sources.push(Strip::Group(n));
        }
        for n in 1..=sq::FX_RETURNS {
            sources.push(Strip::FxReturn(n));
        }
        let mut masters: Vec<Strip> = vec![Strip::Lr];
        for n in 1..=sq::AUXES {
            masters.push(Strip::Aux(n));
        }
        for n in 1..=sq::FX_SENDS {
            masters.push(Strip::FxSend(n));
        }
        for n in 1..=sq::MATRICES {
            masters.push(Strip::Matrix(n));
        }
        for n in 1..=sq::DCAS {
            masters.push(Strip::Dca(n));
        }
        // Mutes.
        for s in sources.iter().chain(masters.iter()) {
            by_param.insert(sq::mute_param(*s), SqEntry { kind: SqKind::Mute, from: *s, to: None });
        }
        for n in 1..=sq::MUTE_GROUPS {
            by_param.insert(sq::mute_param(Strip::MuteGroup(n)), SqEntry { kind: SqKind::Mute, from: Strip::MuteGroup(n), to: None });
        }
        // Master faders and balances.
        for m in &masters {
            if let Some(p) = sq::level_param(*m, None) {
                by_param.insert(p, SqEntry { kind: SqKind::Level, from: *m, to: None });
            }
            if let Some(p) = sq::pan_param(*m, None) {
                by_param.insert(p, SqEntry { kind: SqKind::Pan, from: *m, to: None });
            }
        }
        // Sends: sources into LR / aux / FX; masters into matrices.
        let mut dests: Vec<Strip> = vec![Strip::Lr];
        for n in 1..=sq::AUXES {
            dests.push(Strip::Aux(n));
        }
        for n in 1..=sq::FX_SENDS {
            dests.push(Strip::FxSend(n));
        }
        for s in &sources {
            for d in &dests {
                if let Some(p) = sq::level_param(*s, Some(*d)) {
                    by_param.insert(p, SqEntry { kind: SqKind::Level, from: *s, to: Some(*d) });
                }
                if let Some(p) = sq::pan_param(*s, Some(*d)) {
                    by_param.insert(p, SqEntry { kind: SqKind::Pan, from: *s, to: Some(*d) });
                }
                if let Some(p) = sq::assign_param(*s, *d) {
                    by_param.insert(p, SqEntry { kind: SqKind::Assign, from: *s, to: Some(*d) });
                }
            }
        }
        let mut mtx_sources: Vec<Strip> = vec![Strip::Lr];
        for n in 1..=sq::AUXES {
            mtx_sources.push(Strip::Aux(n));
        }
        for n in 1..=sq::GROUPS {
            mtx_sources.push(Strip::Group(n));
        }
        for s in &mtx_sources {
            for m in 1..=sq::MATRICES {
                if let Some(p) = sq::level_param(*s, Some(Strip::Matrix(m))) {
                    by_param.insert(p, SqEntry { kind: SqKind::Level, from: *s, to: Some(Strip::Matrix(m)) });
                }
                if let Some(p) = sq::pan_param(*s, Some(Strip::Matrix(m))) {
                    by_param.insert(p, SqEntry { kind: SqKind::Pan, from: *s, to: Some(Strip::Matrix(m)) });
                }
                if let Some(p) = sq::assign_param(*s, Strip::Matrix(m)) {
                    by_param.insert(p, SqEntry { kind: SqKind::Assign, from: *s, to: Some(Strip::Matrix(m)) });
                }
            }
        }
        SqTable { by_param }
    }
}

fn sq_channel_mut(show: &mut Show, s: Strip) -> Option<&mut Channel> {
    match s {
        Strip::Input(n) => show.channels.iter_mut().find(|c| c.kind == ChannelKind::Input && c.number == n),
        Strip::FxReturn(n) => show.channels.iter_mut().find(|c| c.kind == ChannelKind::FxReturn && c.number == n),
        _ => None,
    }
}

fn sq_bus_mut(show: &mut Show, s: Strip) -> Option<&mut songbook_model::Bus> {
    let id = s.bus_id()?;
    show.buses.iter_mut().find(|b| b.id == id)
}

/// Put one answered SQ parameter into the show.
fn apply_sq(show: &mut Show, e: &SqEntry, v14: u16) {
    let flag = v14 & 0x7F != 0;
    match (e.kind, e.to) {
        (SqKind::Mute, None) => match e.from {
            Strip::Dca(n) => {
                if let Some(d) = show.dcas.iter_mut().find(|d| d.number == n) {
                    d.on = Some(!flag);
                }
            }
            Strip::MuteGroup(n) => {
                if let Some(m) = show.mute_groups.iter_mut().find(|m| m.number == n) {
                    m.on = Some(flag);
                }
            }
            s => {
                if let Some(c) = sq_channel_mut(show, s) {
                    c.strip.on = Some(!flag);
                } else if let Some(b) = sq_bus_mut(show, s) {
                    b.strip.on = Some(!flag);
                }
            }
        },
        (SqKind::Level, None) => {
            let db = sq::level_to_db(v14);
            if let Strip::Dca(n) = e.from {
                if let Some(d) = show.dcas.iter_mut().find(|d| d.number == n) {
                    d.fader_db = Some(db);
                }
            } else if let Some(b) = sq_bus_mut(show, e.from) {
                b.strip.fader_db = Some(db);
            }
        }
        (SqKind::Pan, None) => {
            if let Some(b) = sq_bus_mut(show, e.from) {
                b.strip.pan = Some(sq::value_to_pan(v14));
            }
        }
        (kind, Some(Strip::Lr)) if matches!(e.from, Strip::Input(_) | Strip::Group(_) | Strip::FxReturn(_)) => {
            // Into LR: the strip's own fader, pan and main assignment.
            if let Some(c) = sq_channel_mut(show, e.from) {
                match kind {
                    SqKind::Level => c.strip.fader_db = Some(sq::level_to_db(v14)),
                    SqKind::Pan => c.strip.pan = Some(sq::value_to_pan(v14)),
                    SqKind::Assign => c.strip.main_assign = Some(flag),
                    SqKind::Mute => {}
                }
            } else if let Some(b) = sq_bus_mut(show, e.from) {
                match kind {
                    SqKind::Level => b.strip.fader_db = Some(sq::level_to_db(v14)),
                    SqKind::Pan => b.strip.pan = Some(sq::value_to_pan(v14)),
                    SqKind::Assign => b.strip.main_assign = Some(flag),
                    SqKind::Mute => {}
                }
            }
        }
        (kind, Some(to)) => {
            let Some(bus_id) = to.bus_id() else { return };
            let send = if let Some(c) = sq_channel_mut(show, e.from) {
                c.send_mut(&bus_id)
            } else if let Some(b) = sq_bus_mut(show, e.from) {
                b.send_mut(&bus_id)
            } else {
                return;
            };
            match kind {
                SqKind::Level => send.level_db = Some(sq::level_to_db(v14)),
                SqKind::Pan => send.pan = Some(sq::value_to_pan(v14)),
                SqKind::Assign => send.on = Some(flag),
                SqKind::Mute => {}
            }
        }
        (SqKind::Assign, None) => {}
    }
}

/// The 14-bit value to write for one SQ parameter, from the show; `None`
/// when the show does not hold it or the push does not want it.
fn value_for_sq(show: &Show, e: &SqEntry, what: &PushWhat) -> Option<u16> {
    let strip_send = |s: &ModelSend| -> Option<u16> {
        match e.kind {
            SqKind::Level => s.level_db.map(sq::db_to_level),
            SqKind::Pan => s.pan.map(sq::pan_to_value),
            SqKind::Assign => s.on.map(u16::from),
            SqKind::Mute => None,
        }
    };
    let wanted = match e.kind {
        SqKind::Mute => what.mutes,
        SqKind::Level => what.levels,
        SqKind::Pan | SqKind::Assign => what.levels,
    };
    if !wanted {
        return None;
    }
    if e.to.is_some() && e.to != Some(Strip::Lr) && !what.sends {
        return None;
    }
    let chan = |s: Strip| -> Option<&Channel> {
        match s {
            Strip::Input(n) => show.channels.iter().find(|c| c.kind == ChannelKind::Input && c.number == n),
            Strip::FxReturn(n) => show.channels.iter().find(|c| c.kind == ChannelKind::FxReturn && c.number == n),
            _ => None,
        }
    };
    let bus = |s: Strip| -> Option<&songbook_model::Bus> { s.bus_id().and_then(|id| show.bus(&id)) };
    match (e.kind, e.to) {
        (SqKind::Mute, None) => match e.from {
            Strip::Dca(n) => show.dcas.iter().find(|d| d.number == n)?.on.map(|on| u16::from(!on)),
            Strip::MuteGroup(n) => show.mute_groups.iter().find(|m| m.number == n)?.on.map(u16::from),
            s => chan(s).and_then(|c| c.strip.on).or_else(|| bus(s).and_then(|b| b.strip.on)).map(|on| u16::from(!on)),
        },
        (SqKind::Level, None) => match e.from {
            Strip::Dca(n) => show.dcas.iter().find(|d| d.number == n)?.fader_db.map(sq::db_to_level),
            s => bus(s)?.strip.fader_db.map(sq::db_to_level),
        },
        (SqKind::Pan, None) => bus(e.from)?.strip.pan.map(sq::pan_to_value),
        (kind, Some(Strip::Lr)) if matches!(e.from, Strip::Input(_) | Strip::Group(_) | Strip::FxReturn(_)) => {
            let strip = chan(e.from).map(|c| &c.strip).or_else(|| bus(e.from).map(|b| &b.strip))?;
            match kind {
                SqKind::Level => strip.fader_db.map(sq::db_to_level),
                SqKind::Pan => strip.pan.map(sq::pan_to_value),
                SqKind::Assign => strip.main_assign.map(u16::from),
                SqKind::Mute => None,
            }
        }
        (_, Some(to)) => {
            let bus_id = to.bus_id()?;
            let sends = chan(e.from).map(|c| &c.sends).or_else(|| bus(e.from).map(|b| &b.sends))?;
            sends.iter().find(|s| s.bus_id == bus_id).and_then(strip_send)
        }
        (SqKind::Assign, None) => None,
    }
}

#[cfg(test)]
pub mod fake {
    //! An in-process stand-in for a desk: answers SQ NRPN gets and dLive
    //! SysEx gets from a state table, so the driver can be exercised without
    //! hardware. It implements the *documented* shapes and nothing more.

    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    pub struct SqState {
        pub values: HashMap<u16, u16>,
        pub recalled: Vec<(u8, u8)>,
        pub writes: Vec<(u16, u16)>,
    }

    pub fn sq_desk(state: Arc<Mutex<SqState>>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = s.try_clone().unwrap();
            let mut p = Parser::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = match r.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                for e in p.feed(&buf[..n]) {
                    match e {
                        Event::NrpnStep { channel, param, increment: true, value: 0x7F } => {
                            let st = state.lock().unwrap();
                            if let Some(v) = st.values.get(&param) {
                                let _ = s.write_all(&midi::nrpn14(channel, param, *v));
                            }
                        }
                        Event::Nrpn { param, coarse, fine: Some(f), .. } => {
                            let mut st = state.lock().unwrap();
                            let v = ((coarse as u16) << 7) | f as u16;
                            st.values.insert(param, v);
                            st.writes.push((param, v));
                        }
                        Event::BankSelect { bank, .. } => {
                            state.lock().unwrap().recalled.push((bank, 0xFF));
                        }
                        Event::ProgramChange { program, .. } => {
                            let mut st = state.lock().unwrap();
                            if let Some(last) = st.recalled.last_mut() {
                                last.1 = program;
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
        addr
    }

    #[derive(Default)]
    pub struct DliveState {
        pub names: HashMap<(u8, u8), String>,
        pub colors: HashMap<(u8, u8), u8>,
        pub mutes: HashMap<(u8, u8), bool>,
        pub faders: HashMap<(u8, u8), u8>,
        pub sends: HashMap<(u8, u8, u8), u8>,
        pub gains: HashMap<u8, u8>,
        pub writes: Vec<String>,
    }

    /// `n` is the desk's base MIDI channel (0-based).
    pub fn dlive_desk(state: Arc<Mutex<DliveState>>, n: u8) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = s.try_clone().unwrap();
            let mut p = Parser::new();
            let mut buf = [0u8; 4096];
            loop {
                let cnt = match r.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(c) => c,
                };
                for e in p.feed(&buf[..cnt]) {
                    let mut st = state.lock().unwrap();
                    match e {
                        Event::SysEx { body } => {
                            let ch = body[0];
                            let off = ch.wrapping_sub(n) & 0x0F;
                            match body[1] {
                                0x01 => {
                                    if let Some(nm) = st.names.get(&(off, body[2])) {
                                        let mut b = vec![0x02, body[2]];
                                        b.extend_from_slice(nm.as_bytes());
                                        let _ = s.write_all(&midi::sysex(ch, &b));
                                    }
                                }
                                0x04 => {
                                    if let Some(c) = st.colors.get(&(off, body[2])) {
                                        let _ = s.write_all(&midi::sysex(ch, &[0x05, body[2], *c]));
                                    }
                                }
                                0x03 => st.writes.push(format!("name {off}/{} = {}", body[2], String::from_utf8_lossy(&body[3..]))),
                                0x06 => st.writes.push(format!("colour {off}/{} = {}", body[2], body[3])),
                                0x05 if body[2] == 0x09 => {
                                    if let Some(m) = st.mutes.get(&(off, body[3])) {
                                        let _ = s.write_all(&midi::note_pulse(ch, body[3], if *m { 0x7F } else { 0x3F }));
                                    }
                                }
                                0x05 if body[2] == 0x0B && body[3] == 0x17 => {
                                    if let Some(lv) = st.faders.get(&(off, body[4])) {
                                        let _ = s.write_all(&midi::nrpn7(ch, body[4], 0x17, *lv));
                                    }
                                }
                                0x05 if body[2] == 0x0B && body[3] == 0x18 => {
                                    if st.names.contains_key(&(off, body[4])) {
                                        let _ = s.write_all(&midi::nrpn7(ch, body[4], 0x18, 0x7F));
                                    }
                                }
                                0x05 if body[2] == 0x0B && body[3] == 0x19 => {
                                    if let Some(g) = st.gains.get(&body[4]) {
                                        let _ = s.write_all(&midi::pitch_bend(ch, body[4], *g));
                                    }
                                }
                                0x05 if body[2] == 0x0F && body[3] == 0x0D => {
                                    let key = (body[4], body[5].wrapping_sub(n) & 0x0F, body[6]);
                                    if let Some(lv) = st.sends.get(&key) {
                                        let _ = s.write_all(&midi::sysex(ch, &[0x0D, body[4], body[5], body[6], *lv]));
                                    }
                                }
                                0x0D => st.writes.push(format!("send {}->{}/{} = {}", body[2], body[3].wrapping_sub(n) & 0x0F, body[4], body[5])),
                                0x07 | 0x0A => {
                                    if st.gains.contains_key(&body[2]) {
                                        let reply = if body[1] == 0x07 { 0x08 } else { 0x0B };
                                        let _ = s.write_all(&midi::sysex(ch, &[reply, body[2], 0x00]));
                                    }
                                }
                                0x09 | 0x0C => st.writes.push(format!("preamp {} {}", body[1], body[2])),
                                _ => {}
                            }
                        }
                        Event::Note { channel, note, velocity } if velocity != 0 => {
                            st.writes.push(format!("mute {}/{} = {}", channel.wrapping_sub(n) & 0x0F, note, velocity >= 0x40));
                        }
                        Event::Nrpn { channel, param, coarse, fine: None } => {
                            let (note, id) = midi::split14(param);
                            st.writes.push(format!("nrpn {}/{} {id:#x} = {coarse}", channel.wrapping_sub(n) & 0x0F, note));
                        }
                        Event::PitchBend { lsb, msb, .. } => st.writes.push(format!("gain {lsb} = {msb}")),
                        Event::BankSelect { bank, .. } => st.writes.push(format!("bank {bank}")),
                        Event::ProgramChange { program, .. } => st.writes.push(format!("program {program}")),
                        _ => {}
                    }
                }
            }
        });
        addr
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::fake::*;
    use super::*;

    #[test]
    fn pulls_and_pushes_an_sq_through_a_fake_desk() {
        let state = Arc::new(Mutex::new(SqState::default()));
        {
            let mut st = state.lock().unwrap();
            st.values.insert(sq::mute_param(Strip::Input(1)), 1);
            st.values.insert(sq::mute_param(Strip::Input(2)), 0);
            st.values.insert(sq::level_param(Strip::Input(1), Some(Strip::Lr)).unwrap(), sq::db_to_level(-6.0));
            st.values.insert(sq::pan_param(Strip::Input(1), Some(Strip::Lr)).unwrap(), sq::pan_to_value(-0.5));
            st.values.insert(sq::assign_param(Strip::Input(1), Strip::Lr).unwrap(), 1);
            st.values.insert(sq::level_param(Strip::Input(1), Some(Strip::Aux(3))).unwrap(), sq::db_to_level(-12.0));
            st.values.insert(sq::assign_param(Strip::Input(1), Strip::Aux(3)).unwrap(), 1);
            st.values.insert(sq::level_param(Strip::Aux(3), None).unwrap(), sq::db_to_level(0.0));
            st.values.insert(sq::level_param(Strip::Aux(3), Some(Strip::Matrix(1))).unwrap(), sq::db_to_level(-3.0));
            st.values.insert(sq::level_param(Strip::Dca(2), None).unwrap(), sq::db_to_level(2.0));
            st.values.insert(sq::mute_param(Strip::MuteGroup(4)), 1);
            st.values.insert(sq::mute_param(Strip::Lr), 0);
        }
        let addr = sq_desk(state.clone());
        let opts = Options { midi_channel: 1, quiet_ms: 150 };
        let mut d = Device::connect(&addr, Platform::AhSq, &opts).unwrap();
        let probe = d.probe().unwrap();
        assert_eq!(probe["lrMuted"], false);
        let show = d.pull("SQ-6").unwrap();
        assert_eq!(show.system.model, "SQ-6");
        let ip1 = &show.channels[0];
        assert_eq!(ip1.strip.on, Some(false));
        assert_eq!(ip1.strip.fader_db, Some(-6.0));
        assert!((ip1.strip.pan.unwrap() + 0.5).abs() < 0.01);
        assert_eq!(ip1.strip.main_assign, Some(true));
        let s = ip1.sends.iter().find(|s| s.bus_id == "bus:aux:3").unwrap();
        assert_eq!(s.level_db, Some(-12.0));
        assert_eq!(s.on, Some(true));
        assert_eq!(show.channels[1].strip.on, Some(true));
        assert_eq!(show.channels[2].strip.on, None, "unanswered parameters stay unknown");
        let aux3 = show.bus("bus:aux:3").unwrap();
        assert_eq!(aux3.strip.fader_db, Some(0.0));
        assert_eq!(aux3.sends[0].bus_id, "bus:mtx:1");
        assert_eq!(aux3.sends[0].level_db, Some(-3.0));
        assert_eq!(show.dcas[1].fader_db, Some(2.0));
        assert_eq!(show.mute_groups[3].on, Some(true));
        assert!(show.validate().is_empty(), "{:?}", show.validate());

        // Push it back with a change and recall a scene.
        let mut show = show;
        show.channels[0].strip.fader_db = Some(-20.0);
        let log = d.push(&show, &PushWhat { mutes: true, levels: true, sends: true, names: false, preamps: false }).unwrap();
        assert!(log[0].contains("parameters written"));
        d.recall_scene(156).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let st = state.lock().unwrap();
        assert_eq!(st.values[&sq::level_param(Strip::Input(1), Some(Strip::Lr)).unwrap()], sq::db_to_level(-20.0));
        assert!(st.writes.iter().any(|(p, v)| *p == sq::mute_param(Strip::Input(1)) && *v == 1));
        assert_eq!(st.recalled, vec![(1, 0x1B)]);
    }

    #[test]
    fn pulls_and_pushes_a_dlive_through_a_fake_desk() {
        let n = 11u8; // base MIDI channel 12
        let state = Arc::new(Mutex::new(DliveState::default()));
        {
            let mut st = state.lock().unwrap();
            st.names.insert((0, 0), "Kick".into());
            st.colors.insert((0, 0), 1);
            st.mutes.insert((0, 0), false);
            st.faders.insert((0, 0), dlive::db_to_level(-5.0));
            st.names.insert((0, 1), "Snare".into());
            st.mutes.insert((0, 1), true);
            st.names.insert((2, 0), "Wedge 1".into()); // mono aux 1
            st.colors.insert((2, 0), 6);
            st.faders.insert((2, 0), dlive::db_to_level(0.0));
            st.names.insert((2, 0x40), "IEM 1".into()); // stereo aux 1
            st.names.insert((4, 0x30), "LR".into()); // main 1
            st.names.insert((4, 0x36), "Band".into()); // DCA 1
            st.sends.insert((0, 2, 0), dlive::db_to_level(-10.0));
            st.sends.insert((0, 2, 0x40), dlive::db_to_level(-20.0));
            st.gains.insert(0, dlive::db_to_gain(30.0));
        }
        let addr = dlive_desk(state.clone(), n);
        let opts = Options { midi_channel: 12, quiet_ms: 150 };
        let mut d = Device::connect(&addr, Platform::AhDlive, &opts).unwrap();
        assert_eq!(d.probe().unwrap()["input1"], "Kick");
        let show = d.pull("dLive S5000").unwrap();
        assert_eq!(show.channels.len(), 2);
        assert_eq!(show.channels[0].label, "Kick");
        assert_eq!(show.channels[0].color.as_deref(), Some("red"));
        assert_eq!(show.channels[0].strip.on, Some(true));
        assert!((show.channels[0].strip.fader_db.unwrap() + 5.0).abs() < 0.3, "7-bit fader law");
        assert_eq!(show.channels[0].strip.main_assign, Some(true));
        assert_eq!(show.channels[1].strip.on, Some(false));
        let auxes: Vec<_> = show.buses_of(BusKind::Aux).collect();
        assert_eq!(auxes.len(), 2);
        assert_eq!(auxes[0].label, "Wedge 1");
        assert!(!auxes[0].stereo);
        assert_eq!(auxes[1].label, "IEM 1");
        assert!(auxes[1].stereo);
        assert_eq!(auxes[1].number, 2, "stereo aux 1 follows the one mono aux");
        assert_eq!(show.buses_of(BusKind::Main).next().unwrap().label, "LR");
        assert_eq!(show.dcas[0].label, "Band");
        let sends = &show.channels[0].sends;
        assert!((sends.iter().find(|s| s.bus_id == auxes[0].id).unwrap().level_db.unwrap() + 10.0).abs() < 0.3);
        assert!((sends.iter().find(|s| s.bus_id == auxes[1].id).unwrap().level_db.unwrap() + 20.0).abs() < 0.3);
        assert_eq!(show.preamps.len(), 1);
        assert_eq!(show.preamps[0].socket_id, "skt:mixrack:in:1");
        assert!((show.preamps[0].gain_db.unwrap() - 30.0).abs() < 0.3);
        assert_eq!(show.preamps[0].phantom, Some(false));
        assert!(show.validate().is_empty(), "{:?}", show.validate());

        let mut show = show;
        show.channels[0].label = "Kick In".into();
        show.channels[0].color = Some("blue".into());
        let log = d.push(&show, &PushWhat { names: true, mutes: true, levels: true, sends: true, preamps: true }).unwrap();
        assert!(log[0].contains("messages written"));
        std::thread::sleep(Duration::from_millis(300));
        let st = state.lock().unwrap();
        assert!(st.writes.iter().any(|w| w == "name 0/0 = Kick In"), "{:?}", st.writes);
        assert!(st.writes.iter().any(|w| w == "colour 0/0 = 4"));
        assert!(st.writes.iter().any(|w| w == "mute 0/1 = true"));
        assert!(st.writes.iter().any(|w| w.starts_with("nrpn 0/0 0x17")));
        assert!(st.writes.iter().any(|w| w.starts_with("send 0->2/64 =")), "{:?}", st.writes);
        assert!(st.writes.iter().any(|w| w.starts_with("gain 0 =")));
    }
}

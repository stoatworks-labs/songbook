//! Yamaha SCP — the "Remote Control Protocol" every current Yamaha desk
//! speaks on TCP 49280: newline-delimited ASCII, one command per line.
//!
//! ```text
//! get MIXER:Current/InCh/Fader/Level 0 0      -> OK get MIXER:Current/InCh/Fader/Level 0 0 -300
//! set MIXER:Current/InCh/Label/Name 0 0 "Kick" -> OK set MIXER:Current/InCh/Label/Name 0 0 "Kick"
//! devinfo productname                           -> OK devinfo productname "DM3"
//! prmnum / prminfo <i>                          -> the desk's own parameter dictionary
//! ssnum_ex scene_a / ssinfo_ex scene_a 3        -> scene slots and their titles
//! ssrecall_ex scene_a 3                         -> OK, then NOTIFY sscurrent_ex scene_a 3
//! ERROR get UnknownAddress
//! ```
//!
//! Indices are 0-based; levels are centi-dB with `-32768` for −∞; the desk
//! pushes `NOTIFY set …` for every change to every client. All of that was
//! observed on a real DM3 (firmware V3.00) on 2026-09-15 in this fleet's
//! Dante-BabelBox work, including a scene store and recall; the CL/QL, TF,
//! DM7 and RIVAGE PM grammars follow the same document and the
//! `yamaha-rcp` Companion module's use of them, and have not been run
//! against those desks from here.
//!
//! The parameter address space differs per family (`MIXER:Current/InCh/Port/
//! HA/Gain` on a CL, `IO:Current/InCh/HAGain` on a DM3), so the driver reads
//! the desk's own dictionary with `prmnum`/`prminfo` and falls back to the
//! embedded copies in `dict/` (each the verbatim reply of one desk, captured
//! by that module) when a desk does not answer them.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use songbook_model::{build, ids, Bus, BusKind, Channel, ChannelKind, Direction, Extra, NoteLevel, Platform, Preamp, Send, Show, SocketKind, UnitRole, FADER_OFF};

pub const PORT: u16 = 49280;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
    #[error("the desk answered ERROR {command} {reason}")]
    Desk { command: String, reason: String },
    #[error("no reply to `{0}` in time")]
    Timeout(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Which grammar a desk speaks for scenes and where its head amps live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    ClQl,
    Tf,
    Dm3,
    Dm7,
    Rivage,
}

impl Family {
    pub fn from_platform(p: Platform) -> Option<Family> {
        Some(match p {
            Platform::YamahaClQl => Family::ClQl,
            Platform::YamahaTf => Family::Tf,
            Platform::YamahaDm3 => Family::Dm3,
            Platform::YamahaDm7 => Family::Dm7,
            Platform::YamahaRivage => Family::Rivage,
            _ => return None,
        })
    }

    pub fn platform(self) -> Platform {
        match self {
            Family::ClQl => Platform::YamahaClQl,
            Family::Tf => Platform::YamahaTf,
            Family::Dm3 => Platform::YamahaDm3,
            Family::Dm7 => Platform::YamahaDm7,
            Family::Rivage => Platform::YamahaRivage,
        }
    }

    /// From `devinfo productname`.
    pub fn from_product(name: &str) -> Option<Family> {
        let n = name.to_uppercase();
        if n.starts_with("CL") || n.starts_with("QL") {
            Some(Family::ClQl)
        } else if n.starts_with("TF") {
            Some(Family::Tf)
        } else if n.starts_with("DM3") {
            Some(Family::Dm3)
        } else if n.starts_with("DM7") {
            Some(Family::Dm7)
        } else if n.contains("PM") || n.contains("RIVAGE") || n.starts_with("CS-R") || n.starts_with("DSP-R") {
            Some(Family::Rivage)
        } else {
            None
        }
    }

    /// The embedded dictionary: one desk's verbatim `prminfo` replies.
    pub fn embedded_dictionary(self) -> &'static str {
        match self {
            Family::ClQl => include_str!("../dict/clql.txt"),
            Family::Tf => include_str!("../dict/tf.txt"),
            Family::Dm3 => include_str!("../dict/dm3.txt"),
            Family::Dm7 => include_str!("../dict/dm7.txt"),
            Family::Rivage => include_str!("../dict/rivage.txt"),
        }
    }

    /// Scene banks: TF, DM3 and DM7 keep `scene_a` / `scene_b`; CL/QL and
    /// RIVAGE PM one list at `MIXER:Lib/Scene`.
    pub fn scene_lists(self) -> &'static [&'static str] {
        match self {
            Family::Tf | Family::Dm3 | Family::Dm7 => &["scene_a", "scene_b"],
            Family::ClQl | Family::Rivage => &["MIXER:Lib/Scene"],
        }
    }

    /// Whether scene numbers are text (`"1.00"`, DM7 and RIVAGE) and the
    /// `…t_ex` verbs apply.
    pub fn text_scenes(self) -> bool {
        matches!(self, Family::Dm7 | Family::Rivage)
    }

    pub fn label(self) -> &'static str {
        match self {
            Family::ClQl => "CL/QL",
            Family::Tf => "TF",
            Family::Dm3 => "DM3",
            Family::Dm7 => "DM7",
            Family::Rivage => "RIVAGE PM",
        }
    }
}

// ---------------------------------------------------------------- the line grammar

/// Split a reply into tokens, keeping quoted strings whole (quotes stripped).
pub fn tokenize(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut had_quote = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                had_quote = true;
            }
            '\\' if quoted => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ' ' | '\t' if !quoted => {
                if !cur.is_empty() || had_quote {
                    out.push(std::mem::take(&mut cur));
                    had_quote = false;
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() || had_quote {
        out.push(cur);
    }
    out
}

pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// One line from the desk.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// `OK` or `OKm` (a clamped/modified set).
    Ok { tokens: Vec<String>, modified: bool },
    Error { command: String, reason: String },
    Notify { tokens: Vec<String> },
    /// A `--` line: the embedded dictionaries mark parameters a desk's
    /// firmware did not support this way.
    Unsupported { tokens: Vec<String> },
    Other(String),
}

pub fn parse_reply(line: &str) -> Reply {
    let t = tokenize(line.trim());
    match t.first().map(String::as_str) {
        Some("OK") => Reply::Ok { tokens: t[1..].to_vec(), modified: false },
        Some("OKm") => Reply::Ok { tokens: t[1..].to_vec(), modified: true },
        Some("ERROR") => Reply::Error { command: t.get(1).cloned().unwrap_or_default(), reason: t.get(2).cloned().unwrap_or_default() },
        Some("NOTIFY") => Reply::Notify { tokens: t[1..].to_vec() },
        Some("--") => Reply::Unsupported { tokens: t[1..].to_vec() },
        // The DM3 capture lists unsupported parameters with no status at all.
        Some("prminfo") => Reply::Unsupported { tokens: t },
        _ => Reply::Other(line.to_string()),
    }
}

// ---------------------------------------------------------------- the dictionary

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub address: String,
    pub x_count: u32,
    pub y_count: u32,
    pub min: i64,
    pub max: i64,
    pub default: String,
    pub unit: String,
    /// `integer`, `string`, `binary`, `list`, `none`, `mtr`.
    pub kind: String,
    /// `rw`, `r`, `w`, `--`.
    pub access: String,
    pub step: i64,
    /// Whether the desk that produced the dictionary supported it (`OK` vs `--`).
    pub supported: bool,
}

impl Param {
    pub fn readable(&self) -> bool {
        self.supported && self.access.contains('r')
    }
    pub fn writable(&self) -> bool {
        self.access.contains('w')
    }
    pub fn is_level(&self) -> bool {
        self.unit == "dB" && self.min <= -32768
    }
    pub fn is_string(&self) -> bool {
        self.kind == "string" || self.kind == "binary" || self.kind == "any"
    }
}

/// Parse one `prminfo` reply (with or without the `OK prminfo N` lead).
pub fn parse_prminfo(line: &str) -> Option<Param> {
    let t = tokenize(line.trim());
    let supported = t.first().map(|s| s == "OK" || s == "OKm").unwrap_or(false);
    let start = t.iter().position(|s| s == "prminfo").map(|i| i + 2)?;
    let f = &t[start..];
    if f.len() < 10 {
        return None;
    }
    Some(Param {
        address: f[0].clone(),
        x_count: f[1].parse().ok()?,
        y_count: f[2].parse().ok()?,
        min: f[3].parse().unwrap_or(0),
        max: f[4].parse().unwrap_or(0),
        default: f[5].clone(),
        unit: f[6].clone(),
        kind: f[7].clone(),
        access: f.get(9).cloned().unwrap_or_default(),
        step: f.get(10).and_then(|s| s.parse().ok()).unwrap_or(1),
        supported,
    })
}

/// A parsed dictionary, by address.
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    pub params: BTreeMap<String, Param>,
    /// Scene commands seen (`MIXER:Lib/Scene/Recall` …) with their max.
    pub scene_slots: Option<u32>,
}

impl Dictionary {
    pub fn parse(text: &str) -> Dictionary {
        let mut d = Dictionary::default();
        for line in text.lines() {
            if let Some(p) = parse_prminfo(line) {
                d.params.insert(p.address.clone(), p);
            } else if line.contains("scninfo") && line.contains("Scene/Recall") {
                let t = tokenize(line);
                if let Some(i) = t.iter().position(|s| s.contains("Scene/Recall")) {
                    d.scene_slots = t.get(i + 4).and_then(|s| s.parse().ok());
                }
            }
        }
        d
    }

    pub fn embedded(family: Family) -> Dictionary {
        Dictionary::parse(family.embedded_dictionary())
    }

    pub fn get(&self, address: &str) -> Option<&Param> {
        self.params.get(address)
    }

    /// The first of several spellings a family may use.
    pub fn first(&self, addresses: &[&str]) -> Option<&Param> {
        addresses.iter().find_map(|a| self.params.get(*a))
    }

    /// How many strips a table has (`InCh` → 16 on a DM3, 72 on a CL5).
    pub fn strips(&self, table: &str) -> Option<u32> {
        self.first(&[&format!("MIXER:Current/{table}/Fader/Level"), &format!("MIXER:Current/{table}/Label/Name"), &format!("MIXER:Current/{table}/Fader/On")]).map(|p| p.x_count)
    }
}

// ---------------------------------------------------------------- values

pub fn level_db(v: i64) -> f64 {
    if v <= -32768 {
        FADER_OFF
    } else {
        v as f64 / 100.0
    }
}

pub fn db_level(db: f64) -> i64 {
    if db <= FADER_OFF + 0.5 {
        -32768
    } else {
        (db.clamp(-138.0, 10.0) * 100.0).round() as i64
    }
}

pub fn pan(v: i64) -> f64 {
    (v as f64 / 63.0).clamp(-1.0, 1.0)
}

pub fn pan_value(p: f64) -> i64 {
    (p.clamp(-1.0, 1.0) * 63.0).round() as i64
}

/// The desk's colour word for a model colour, per family.
pub fn color_word(family: Family, color: &str) -> Option<&'static str> {
    Some(match (family, color) {
        (_, "red") => "Red",
        (_, "green") => "Green",
        (_, "yellow") => "Yellow",
        (_, "blue") => "Blue",
        (_, "purple") => "Purple",
        (_, "orange") => "Orange",
        (Family::ClQl | Family::Rivage, "cyan") => "Cyan",
        (Family::ClQl | Family::Rivage, "pink") => "Magenta",
        (_, "cyan") => "SkyBlue",
        (_, "pink") => "Pink",
        (_, "off") => "Off",
        _ => return None,
    })
}

// ---------------------------------------------------------------- the client

pub struct Client {
    stream: TcpStream,
    rx: Receiver<String>,
    pub host: String,
    /// Unsolicited lines seen while waiting for something else.
    pub notifies: Vec<Vec<String>>,
    timeout: Duration,
}

impl Client {
    pub fn connect(host: &str) -> Result<Client> {
        let addr = if host.contains(':') { host.to_string() } else { format!("{host}:{PORT}") };
        let sa = addr.to_socket_addrs()?.next().ok_or_else(|| Error::Other(format!("cannot resolve {addr}")))?;
        let stream = TcpStream::connect_timeout(&sa, Duration::from_secs(4))?;
        stream.set_nodelay(true)?;
        let reader = stream.try_clone()?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match r.read_line(&mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {
                        if tx.send(line.trim_end_matches(['\r', '\n']).to_string()).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        let mut c = Client { stream, rx, host: host.to_string(), notifies: vec![], timeout: Duration::from_secs(3) };
        // Ask the desk to keep the session open; best effort.
        let _ = c.send_line("scpmode keepalive 60000");
        let _ = c.collect_replies(1, Duration::from_millis(600));
        Ok(c)
    }

    pub fn send_line(&mut self, line: &str) -> Result<()> {
        self.stream.write_all(format!("{line}\n").as_bytes())?;
        Ok(())
    }

    /// The next non-NOTIFY reply, keeping notifies aside.
    fn next_reply(&mut self, deadline: Instant) -> Option<Reply> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            match self.rx.recv_timeout(deadline - now) {
                Ok(line) => match parse_reply(&line) {
                    Reply::Notify { tokens } => self.notifies.push(tokens),
                    Reply::Other(_) => continue,
                    r => return Some(r),
                },
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// Up to `n` replies, or until `max` passes.
    fn collect_replies(&mut self, n: usize, max: Duration) -> Vec<Reply> {
        let deadline = Instant::now() + max;
        let mut out = vec![];
        while out.len() < n {
            match self.next_reply(deadline) {
                Some(r) => out.push(r),
                None => break,
            }
        }
        out
    }

    /// Send one command and wait for its reply.
    pub fn request(&mut self, line: &str) -> Result<Vec<String>> {
        self.send_line(line)?;
        match self.next_reply(Instant::now() + self.timeout) {
            Some(Reply::Ok { tokens, .. }) => Ok(tokens),
            Some(Reply::Error { command, reason }) => Err(Error::Desk { command, reason }),
            Some(Reply::Unsupported { .. }) => Err(Error::Desk { command: line.split(' ').next().unwrap_or_default().to_string(), reason: "Unsupported".into() }),
            _ => Err(Error::Timeout(line.to_string())),
        }
    }

    pub fn devinfo(&mut self, key: &str) -> Result<String> {
        let t = self.request(&format!("devinfo {key}"))?;
        Ok(t.get(2).cloned().unwrap_or_default())
    }

    /// `get <addr> x y` → the value token(s) after the address and indices.
    pub fn get(&mut self, address: &str, x: u32, y: u32) -> Result<Vec<String>> {
        let t = self.request(&format!("get {address} {x} {y}"))?;
        Ok(t.get(4..).map(|s| s.to_vec()).unwrap_or_default())
    }

    pub fn set(&mut self, address: &str, x: u32, y: u32, value: &str) -> Result<()> {
        self.request(&format!("set {address} {x} {y} {value}")).map(|_| ())
    }

    /// Many gets, pipelined in batches; replies are matched by address and
    /// indices so an unanswered one is simply missing.
    pub fn get_many(&mut self, requests: &[(String, u32, u32)]) -> Result<HashMap<(String, u32, u32), Vec<String>>> {
        let mut out = HashMap::new();
        for chunk in requests.chunks(64) {
            let mut line = String::new();
            for (a, x, y) in chunk {
                line.push_str(&format!("get {a} {x} {y}\n"));
            }
            self.stream.write_all(line.as_bytes())?;
            let replies = self.collect_replies(chunk.len(), self.timeout + Duration::from_millis(30 * chunk.len() as u64));
            for r in replies {
                if let Reply::Ok { tokens, .. } = r {
                    if tokens.len() >= 4 && tokens[0] == "get" {
                        let (Ok(x), Ok(y)) = (tokens[2].parse::<u32>(), tokens[3].parse::<u32>()) else { continue };
                        out.insert((tokens[1].clone(), x, y), tokens[4..].to_vec());
                    }
                }
            }
        }
        Ok(out)
    }

    /// The desk's own dictionary, or `None` when it does not answer `prmnum`.
    pub fn dictionary(&mut self) -> Option<Dictionary> {
        let n: u32 = self.request("prmnum").ok()?.get(1)?.parse().ok()?;
        let mut text = String::new();
        let mut line = String::new();
        for i in 0..n {
            line.push_str(&format!("prminfo {i}\n"));
            if i % 64 == 63 || i + 1 == n {
                self.stream.write_all(line.as_bytes()).ok()?;
                line.clear();
                for r in self.collect_replies(64.min(i as usize % 64 + 1), self.timeout) {
                    let (lead, tokens) = match r {
                        Reply::Ok { tokens, modified } => (if modified { "OKm " } else { "OK " }, tokens),
                        Reply::Unsupported { tokens } => ("-- ", tokens),
                        _ => continue,
                    };
                    text.push_str(lead);
                    text.push_str(&tokens.iter().map(|t| if t.contains(' ') || t.is_empty() { quote(t) } else { t.clone() }).collect::<Vec<_>>().join(" "));
                    text.push('\n');
                }
            }
        }
        let d = Dictionary::parse(&text);
        if d.params.is_empty() {
            None
        } else {
            Some(d)
        }
    }

    // ------------------------------------------------------------ scenes

    /// `(list, slot, title, comment, populated)` for every slot the desk lists.
    pub fn scenes(&mut self, family: Family) -> Result<Vec<SceneSlot>> {
        let mut out = vec![];
        for list in family.scene_lists() {
            let verb = if family.text_scenes() { "ssnumt_ex" } else { "ssnum_ex" };
            let n: u32 = match self.request(&format!("{verb} {list}")) {
                Ok(t) => t.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                Err(_) => 0,
            };
            if n == 0 {
                continue;
            }
            let info = if family.text_scenes() { "ssinfot_ex" } else { "ssinfo_ex" };
            for slot in 1..=n {
                let arg = if family.text_scenes() { quote(&format!("{slot}.00")) } else { slot.to_string() };
                let Ok(t) = self.request(&format!("{info} {list} {arg}")) else { continue };
                // OK ssinfo_ex scene_a 30 "30" "ins on" "comment" user
                let populated = t.iter().any(|s| s == "user" || s == "preset") || t.last().map(|s| s != "empty").unwrap_or(false);
                if !populated {
                    continue;
                }
                out.push(SceneSlot { list: list.to_string(), slot, title: t.get(4).cloned().unwrap_or_default(), comment: t.get(5).cloned().unwrap_or_default() });
            }
        }
        Ok(out)
    }

    pub fn recall_scene(&mut self, family: Family, list: &str, slot: u32) -> Result<()> {
        let line = if family.text_scenes() { format!("ssrecallt_ex {list} {}", quote(&format!("{slot}.00"))) } else { format!("ssrecall_ex {list} {slot}") };
        self.request(&line).map(|_| ())
    }

    pub fn store_scene(&mut self, family: Family, list: &str, slot: u32) -> Result<()> {
        let line = if family.text_scenes() { format!("ssupdatet_ex {list} {}", quote(&format!("{slot}.00"))) } else { format!("ssupdate_ex {list} {slot}") };
        self.request(&line).map(|_| ())
    }

    pub fn current_scene(&mut self, family: Family) -> Option<(String, String)> {
        for list in family.scene_lists() {
            let verb = if family.text_scenes() { "sscurrentt_ex" } else { "sscurrent_ex" };
            if let Ok(t) = self.request(&format!("{verb} {list}")) {
                return Some((list.to_string(), t.get(2).cloned().unwrap_or_default()));
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneSlot {
    pub list: String,
    pub slot: u32,
    pub title: String,
    pub comment: String,
}

// ---------------------------------------------------------------- pull

/// Strip tables and what they become; the first spelling found wins.
const CHANNEL_TABLES: &[(&str, ChannelKind)] = &[("InCh", ChannelKind::Input), ("StInCh", ChannelKind::StereoInput), ("FxRtnCh", ChannelKind::FxReturn)];
const BUS_TABLES: &[(&str, BusKind)] = &[("Mix", BusKind::Aux), ("Mtrx", BusKind::Matrix), ("St", BusKind::Main), ("Mono", BusKind::Other), ("Fx", BusKind::FxSend)];

fn first_value(v: Option<&Vec<String>>) -> Option<String> {
    v.and_then(|t| t.first().cloned())
}

fn int_value(v: Option<&Vec<String>>) -> Option<i64> {
    first_value(v).and_then(|s| s.parse().ok())
}

/// Read the whole mix. `deep_scenes` recalls every scene to read it, which
/// changes the desk's live state and is only done when asked.
pub fn pull(client: &mut Client, family: Family, model: &str, firmware: &str) -> Result<Show> {
    let (dict, dict_source) = match client.dictionary() {
        Some(d) => (d, "the desk's own prminfo"),
        None => (Dictionary::embedded(family), "the embedded dictionary"),
    };
    let mut show = Show::new(&format!("{model} pull"), family.platform());
    show.system.model = model.to_string();
    show.system.firmware = firmware.to_string();
    show.meta.source = Some(songbook_model::SourceInfo { kind: "device".into(), origin: client.host.clone(), at: songbook_model::now(), firmware: Some(firmware.to_string()) });
    show.system.units.push(build::unit("local", &format!("{model} inputs"), model, UnitRole::Console));

    // Plan every get from the dictionary, then run them pipelined.
    let mut reqs: Vec<(String, u32, u32)> = vec![];
    let mut want = |dict: &Dictionary, addrs: &[&str], x_max: Option<u32>| -> Option<String> {
        let p = dict.first(addrs)?;
        if !p.readable() && !p.supported {
            return None;
        }
        let xs = x_max.unwrap_or(p.x_count).min(p.x_count.max(1));
        let ys = p.y_count.max(1);
        for x in 0..xs {
            for y in 0..ys {
                reqs.push((p.address.clone(), x, y));
            }
        }
        Some(p.address.clone())
    };
    let mut plan: HashMap<(String, &str), String> = HashMap::new();
    let tables: Vec<&str> = CHANNEL_TABLES.iter().map(|(t, _)| *t).chain(BUS_TABLES.iter().map(|(t, _)| *t)).collect();
    for table in tables {
        for field in ["Fader/Level", "Fader/On", "Label/Name", "Label/Color", "Label/Icon", "ToSt/Pan", "ToSt/On", "Out/Balance", "ToMix/Level", "ToMix/On", "ToMix/Pan", "ToMix/PrePost", "ToMtrx/Level", "ToMtrx/On", "ToMtrx/Pan", "ToFx/Level", "ToFx/On", "DCA/Assign", "Port/HA/Gain", "Patch", "Role", "BusType", "Insert/On", "PEQ/On", "HPF/On", "Gate/On", "Comp/On", "Dyna1/On", "Dyna2/On", "Delay/On"] {
            if let Some(a) = want(&dict, &[&format!("MIXER:Current/{table}/{field}")], None) {
                plan.insert((table.to_string(), field), a);
            }
        }
    }
    for (table, fields) in [("DCA", &["Fader/Level", "Fader/On", "Label/Name", "Label/Color"][..]), ("DcaCh", &["Fader/Level", "Fader/On", "Label/Name", "Label/Color"][..]), ("MuteMaster", &["On", "Label/Name"][..]), ("MuteGrpCtrl", &["On", "Label/Name"][..])] {
        for field in fields {
            if let Some(a) = want(&dict, &[&format!("MIXER:Current/{table}/{field}")], None) {
                plan.insert((table.to_string(), field), a);
            }
        }
    }
    // Head amps outside the MIXER tree (DM3 / DM7 / TF).
    for (key, addrs) in [("IO/HAGain", &["IO:Current/InCh/HAGain"][..]), ("IO/48VOn", &["IO:Current/InCh/48VOn"][..]), ("IO/Phantom", &["MIXER:Current/InCh/Port/HA/48VOn", "MIXER:Current/InCh/Port/HA/Phantom"][..])] {
        if let Some(a) = want(&dict, addrs, None) {
            plan.insert(("IO".into(), key), a);
        }
    }
    let values = client.get_many(&reqs)?;
    if values.is_empty() {
        return Err(Error::Other(format!("the desk at {} answered none of {} parameter requests", client.host, reqs.len())));
    }
    let v = |table: &str, field: &str, x: u32, y: u32| -> Option<&Vec<String>> {
        let a = plan.get(&(table.to_string(), field))?;
        values.get(&(a.clone(), x, y))
    };

    // Buses first, so sends can point at them.
    let mut bus_ids: HashMap<(BusKind, u32), String> = HashMap::new();
    for (table, kind) in BUS_TABLES {
        let Some(n) = dict.strips(table) else { continue };
        let mut number = 0u32;
        for x in 0..n {
            // Stereo L/R members of a bus arrive as separate strips on some
            // desks (St 0/1); keep every strip the desk lists.
            let name = first_value(v(table, "Label/Name", x, 0)).unwrap_or_default();
            let default = format!("{} {}", match kind {
                BusKind::Aux => "MIX",
                BusKind::Matrix => "MTX",
                BusKind::Main => "ST",
                BusKind::Other => "MONO",
                BusKind::FxSend => "FX",
                BusKind::Group => "GRP",
            }, x + 1);
            let stereo = matches!(kind, BusKind::Main) || first_value(v(table, "Role", x, 0)).map(|r| r.to_lowercase().contains("stereo")).unwrap_or(false);
            number += 1;
            let label = if name.trim().is_empty() { default } else { name.trim().to_string() };
            let mut b = Bus::new(*kind, number, &label, stereo);
            b.color = first_value(v(table, "Label/Color", x, 0)).and_then(|c| crate::scene::color_name(&c)).map(str::to_string);
            b.strip.fader_db = int_value(v(table, "Fader/Level", x, 0)).map(level_db);
            b.strip.on = int_value(v(table, "Fader/On", x, 0)).map(|i| i != 0);
            b.strip.pan = int_value(v(table, "Out/Balance", x, 0)).or_else(|| int_value(v(table, "ToSt/Pan", x, 0))).map(pan);
            b.strip.main_assign = int_value(v(table, "ToSt/On", x, 0)).map(|i| i != 0);
            b.strip.eq = int_value(v(table, "PEQ/On", x, 0)).map(|i| songbook_model::Eq { on: i != 0, bands: vec![] });
            b.strip.hpf = int_value(v(table, "HPF/On", x, 0)).map(|i| songbook_model::Filter { on: i != 0, freq_hz: None });
            b.strip.comp = int_value(v(table, "Comp/On", x, 0)).or_else(|| int_value(v(table, "Dyna1/On", x, 0))).map(|i| songbook_model::Dynamics { on: i != 0, ..Default::default() });
            if let Some(bt) = first_value(v(table, "BusType", x, 0)) {
                b.extra.insert("yamahaBusType".into(), bt.into());
            }
            b.extra.insert("scpTable".into(), (*table).into());
            b.extra.insert("scpIndex".into(), x.into());
            bus_ids.insert((*kind, x), b.id.clone());
            show.buses.push(b);
        }
    }
    let mut matrix_sends = vec![];
    for b in &mut show.buses {
        let (Some(table), Some(x)) = (b.extra.get("scpTable").and_then(|t| t.as_str()).map(str::to_string), b.extra.get("scpIndex").and_then(|t| t.as_u64())) else { continue };
        let x = x as u32;
        if let Some(p) = plan.get(&(table.clone(), "ToMtrx/Level")).and_then(|a| dict.get(a)) {
            for y in 0..p.y_count {
                if let Some(l) = int_value(values.get(&(p.address.clone(), x, y))) {
                    if let Some(id) = bus_ids.get(&(BusKind::Matrix, y)) {
                        let on = plan.get(&(table.clone(), "ToMtrx/On")).and_then(|a| int_value(values.get(&(a.clone(), x, y)))).map(|i| i != 0);
                        matrix_sends.push((b.id.clone(), id.clone(), level_db(l), on));
                    }
                }
            }
        }
    }
    for (from, to, level, on) in matrix_sends {
        if let Some(b) = show.buses.iter_mut().find(|b| b.id == from) {
            let s = b.send_mut(&to);
            s.level_db = Some(level);
            s.on = on;
        }
    }

    // DCAs and mute groups.
    for table in ["DCA", "DcaCh"] {
        let Some(n) = dict.strips(table) else { continue };
        if !show.dcas.is_empty() {
            break;
        }
        for x in 0..n {
            let name = first_value(v(table, "Label/Name", x, 0)).unwrap_or_default();
            let label = if name.trim().is_empty() { format!("DCA {}", x + 1) } else { name.trim().to_string() };
            let mut d = build::dca(x + 1, &label);
            d.color = first_value(v(table, "Label/Color", x, 0)).and_then(|c| crate::scene::color_name(&c)).map(str::to_string);
            d.fader_db = int_value(v(table, "Fader/Level", x, 0)).map(level_db);
            d.on = int_value(v(table, "Fader/On", x, 0)).map(|i| i != 0);
            show.dcas.push(d);
        }
    }
    for table in ["MuteMaster", "MuteGrpCtrl"] {
        let Some(p) = plan.get(&(table.to_string(), "On")).and_then(|a| dict.get(a)) else { continue };
        if !show.mute_groups.is_empty() {
            break;
        }
        for x in 0..p.x_count {
            let name = first_value(v(table, "Label/Name", x, 0)).unwrap_or_default();
            let label = if name.trim().is_empty() { format!("MUTE {}", x + 1) } else { name.trim().to_string() };
            let mut m = build::mute_group(x + 1, &label);
            m.on = int_value(v(table, "On", x, 0)).map(|i| i != 0);
            show.mute_groups.push(m);
        }
    }

    // Channels.
    for (table, kind) in CHANNEL_TABLES {
        let Some(n) = dict.strips(table) else { continue };
        for x in 0..n {
            let number = x + 1;
            let id = match kind {
                ChannelKind::Input => ids::channel(number),
                ChannelKind::StereoInput => ids::stereo_input(number),
                ChannelKind::FxReturn => ids::fx_return(number),
            };
            let name = first_value(v(table, "Label/Name", x, 0)).unwrap_or_default();
            let default = match kind {
                ChannelKind::Input => format!("ch {number}"),
                ChannelKind::StereoInput => format!("ST IN {number}"),
                ChannelKind::FxReturn => format!("FX RTN {number}"),
            };
            let label = if name.trim().is_empty() { default } else { name.trim().to_string() };
            let mut c = Channel::new(id, number, *kind, &label);
            c.stereo = *kind != ChannelKind::Input;
            c.color = first_value(v(table, "Label/Color", x, 0)).and_then(|s| crate::scene::color_name(&s)).map(str::to_string);
            if let Some(icon) = first_value(v(table, "Label/Icon", x, 0)) {
                c.extra.insert("yamahaIcon".into(), icon.into());
            }
            c.strip.fader_db = int_value(v(table, "Fader/Level", x, 0)).map(level_db);
            c.strip.on = int_value(v(table, "Fader/On", x, 0)).map(|i| i != 0);
            c.strip.pan = int_value(v(table, "ToSt/Pan", x, 0)).map(pan);
            c.strip.main_assign = int_value(v(table, "ToSt/On", x, 0)).map(|i| i != 0);
            c.strip.eq = int_value(v(table, "PEQ/On", x, 0)).map(|i| songbook_model::Eq { on: i != 0, bands: vec![] });
            c.strip.hpf = int_value(v(table, "HPF/On", x, 0)).map(|i| songbook_model::Filter { on: i != 0, freq_hz: None });
            c.strip.gate = int_value(v(table, "Gate/On", x, 0)).or_else(|| int_value(v(table, "Dyna1/On", x, 0))).map(|i| songbook_model::Dynamics { on: i != 0, ..Default::default() });
            c.strip.comp = int_value(v(table, "Comp/On", x, 0)).or_else(|| int_value(v(table, "Dyna2/On", x, 0))).map(|i| songbook_model::Dynamics { on: i != 0, ..Default::default() });
            c.strip.insert_on = int_value(v(table, "Insert/On", x, 0)).map(|i| i != 0);
            if let Some(role) = first_value(v(table, "Role", x, 0)) {
                c.extra.insert("yamahaRole".into(), role.into());
            }
            // Sends.
            for (field, on_field, pan_field, pre_field, bkind) in [("ToMix/Level", "ToMix/On", "ToMix/Pan", "ToMix/PrePost", BusKind::Aux), ("ToMtrx/Level", "ToMtrx/On", "ToMtrx/Pan", "", BusKind::Matrix), ("ToFx/Level", "ToFx/On", "", "", BusKind::FxSend)] {
                let Some(p) = plan.get(&(table.to_string(), field)).and_then(|a| dict.get(a)) else { continue };
                for y in 0..p.y_count {
                    let Some(l) = int_value(values.get(&(p.address.clone(), x, y))) else { continue };
                    let Some(bus_id) = bus_ids.get(&(bkind, y)) else { continue };
                    c.sends.push(Send {
                        bus_id: bus_id.clone(),
                        level_db: Some(level_db(l)),
                        on: plan.get(&(table.to_string(), on_field)).and_then(|a| int_value(values.get(&(a.clone(), x, y)))).map(|i| i != 0),
                        pre: if pre_field.is_empty() { None } else { plan.get(&(table.to_string(), pre_field)).and_then(|a| int_value(values.get(&(a.clone(), x, y)))).map(|i| i != 0) },
                        pan: if pan_field.is_empty() { None } else { plan.get(&(table.to_string(), pan_field)).and_then(|a| int_value(values.get(&(a.clone(), x, y)))).map(pan) },
                    });
                }
            }
            if let Some(p) = plan.get(&(table.to_string(), "DCA/Assign")).and_then(|a| dict.get(a)) {
                for y in 0..p.y_count {
                    if int_value(values.get(&(p.address.clone(), x, y))) == Some(1) {
                        c.dca_ids.push(ids::dca(y + 1));
                    }
                }
            }
            // Patch, as the desk names it (CL/QL, RIVAGE): "DANTE1", "INPUT4".
            if let Some(src) = first_value(v(table, "Patch", x, 0)) {
                let s = src.trim().to_uppercase();
                let socket = if let Some(n) = s.strip_prefix("DANTE").and_then(|n| n.trim().parse::<u32>().ok()) {
                    Some(("dante", SocketKind::Dante, n, "DANTE"))
                } else if let Some(n) = s.strip_prefix("INPUT").and_then(|n| n.trim().parse::<u32>().ok()) {
                    Some(("local", SocketKind::Mic, n, "INPUT"))
                } else { s.strip_prefix("OMNI").and_then(|n| n.trim().parse::<u32>().ok()).map(|n| ("local", SocketKind::Mic, n, "OMNI")) };
                match socket {
                    Some((unit, sk, n, label)) => {
                        if show.unit(&ids::unit(unit)).is_none() {
                            show.system.units.push(build::unit(unit, if unit == "dante" { "Dante" } else { "Local inputs" }, "", if unit == "dante" { UnitRole::Network } else { UnitRole::Console }));
                        }
                        let sid = ids::socket(unit, Direction::In, n);
                        if show.socket(&sid).is_none() {
                            show.sockets.push(build::socket(unit, Direction::In, n, sk, &format!("{label} {n}")));
                        }
                        c.source = Some(sid);
                    }
                    None => {
                        if !s.is_empty() && s != "NONE" {
                            c.extra.insert("yamahaPatch".into(), src.into());
                        }
                    }
                }
            }
            // Head amp: the CL/QL keep it on the channel's port; the DM3 / DM7 / TF on IO:Current/InCh.
            if *kind == ChannelKind::Input {
                let gain = int_value(v(table, "Port/HA/Gain", x, 0)).map(|g| g as f64 / 100.0).or_else(|| int_value(v("IO", "IO/HAGain", x, 0)).map(|g| g as f64));
                let phantom = int_value(v("IO", "IO/48VOn", x, 0)).or_else(|| int_value(v("IO", "IO/Phantom", x, 0))).map(|i| i != 0);
                if gain.is_some() || phantom.is_some() {
                    let sid = c.source.clone().unwrap_or_else(|| {
                        let sid = ids::socket("local", Direction::In, number);
                        if show.socket(&sid).is_none() {
                            show.sockets.push(build::socket("local", Direction::In, number, SocketKind::Mic, &format!("INPUT {number}")));
                        }
                        sid
                    });
                    if c.source.is_none() {
                        c.source = Some(sid.clone());
                    }
                    if show.preamp(&sid).is_none() {
                        show.preamps.push(Preamp { socket_id: sid, gain_db: gain, pad: None, phantom, extra: Extra::new() });
                    }
                }
            }
            c.extra.insert("scpTable".into(), (*table).into());
            c.extra.insert("scpIndex".into(), x.into());
            show.channels.push(c);
        }
    }
    show.sockets.sort_by_key(|a| (a.unit_id.clone(), a.index));

    // Scenes.
    match client.scenes(family) {
        Ok(slots) => {
            for s in slots {
                let bank = s.list.strip_prefix("scene_").map(|b| b.to_uppercase());
                let label = if s.title.trim().is_empty() { format!("Scene {}", s.slot) } else { s.title.trim().to_string() };
                let mut sc = build::scene(s.slot, &label);
                if let Some(b) = &bank {
                    sc.id = ids::scene_in_bank(b, s.slot);
                    sc.bank = Some(b.clone());
                }
                sc.notes = s.comment;
                sc.extra.insert("scpList".into(), s.list.into());
                show.scenes.push(sc);
            }
        }
        Err(e) => show.note(NoteLevel::Info, "scenes", format!("scene list not read: {e}")),
    }
    if let Some((list, cur)) = client.current_scene(family) {
        show.system.extra.insert("currentScene".into(), format!("{list} {cur}").into());
    }
    show.note(NoteLevel::Info, "channels", format!("pulled {} of {} parameters over SCP using {dict_source}; EQ bands, dynamics settings and output patches are on the desk but not read", values.len(), reqs.len()));
    if plan.contains_key(&("IO".into(), "IO/HAGain")) {
        show.note(NoteLevel::Info, "preamps", "on this desk head-amp gain is global (not part of a scene): a scene recall does not move it, and neither does pushing a scene");
    }
    Ok(show)
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

/// Write the show's values with `set`. Everything the dictionary says is
/// writable and the show holds; nothing else.
pub fn push(client: &mut Client, family: Family, show: &Show, what: &PushWhat) -> Result<Vec<String>> {
    let dict = client.dictionary().unwrap_or_else(|| Dictionary::embedded(family));
    let mut lines: Vec<String> = vec![];
    let mut skipped = 0usize;
    let mut set = |dict: &Dictionary, addr: &str, x: u32, y: u32, value: String| {
        match dict.get(addr) {
            Some(p) if p.writable() && x < p.x_count.max(1) && y < p.y_count.max(1) => lines.push(format!("set {addr} {x} {y} {value}")),
            _ => skipped += 1,
        }
    };
    let bus_index = |b: &Bus| -> Option<(String, u32)> {
        let t = b.extra.get("scpTable").and_then(|v| v.as_str()).map(str::to_string).or_else(|| {
            Some(match b.kind {
                BusKind::Aux => "Mix".into(),
                BusKind::Matrix => "Mtrx".into(),
                BusKind::Main => "St".into(),
                BusKind::FxSend => "Fx".into(),
                BusKind::Other => "Mono".into(),
                BusKind::Group => return None,
            })
        })?;
        let x = b.extra.get("scpIndex").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(b.number.saturating_sub(1));
        Some((t, x))
    };
    let matrix_index: HashMap<String, u32> = show.buses.iter().filter(|b| b.kind == BusKind::Matrix).filter_map(|b| bus_index(b).map(|(_, x)| (b.id.clone(), x))).collect();
    let aux_index: HashMap<String, u32> = show.buses.iter().filter(|b| b.kind == BusKind::Aux).filter_map(|b| bus_index(b).map(|(_, x)| (b.id.clone(), x))).collect();
    let fx_index: HashMap<String, u32> = show.buses.iter().filter(|b| b.kind == BusKind::FxSend).filter_map(|b| bus_index(b).map(|(_, x)| (b.id.clone(), x))).collect();

    for c in &show.channels {
        let table = c.extra.get("scpTable").and_then(|v| v.as_str()).unwrap_or(match c.kind {
            ChannelKind::Input => "InCh",
            ChannelKind::StereoInput => "StInCh",
            ChannelKind::FxReturn => "FxRtnCh",
        });
        let x = c.extra.get("scpIndex").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(c.number.saturating_sub(1));
        if what.names {
            set(&dict, &format!("MIXER:Current/{table}/Label/Name"), x, 0, quote(&c.label));
            if let Some(w) = c.color.as_deref().and_then(|col| color_word(family, col)) {
                set(&dict, &format!("MIXER:Current/{table}/Label/Color"), x, 0, quote(w));
            }
        }
        if what.mutes {
            if let Some(on) = c.strip.on {
                set(&dict, &format!("MIXER:Current/{table}/Fader/On"), x, 0, u8::from(on).to_string());
            }
        }
        if what.levels {
            if let Some(db) = c.strip.fader_db {
                set(&dict, &format!("MIXER:Current/{table}/Fader/Level"), x, 0, db_level(db).to_string());
            }
            if let Some(p) = c.strip.pan {
                set(&dict, &format!("MIXER:Current/{table}/ToSt/Pan"), x, 0, pan_value(p).to_string());
            }
            if let Some(m) = c.strip.main_assign {
                set(&dict, &format!("MIXER:Current/{table}/ToSt/On"), x, 0, u8::from(m).to_string());
            }
        }
        if what.sends {
            for s in &c.sends {
                let (field, y) = if let Some(y) = aux_index.get(&s.bus_id) {
                    ("ToMix", *y)
                } else if let Some(y) = matrix_index.get(&s.bus_id) {
                    ("ToMtrx", *y)
                } else if let Some(y) = fx_index.get(&s.bus_id) {
                    ("ToFx", *y)
                } else {
                    continue;
                };
                if let Some(db) = s.level_db {
                    set(&dict, &format!("MIXER:Current/{table}/{field}/Level"), x, y, db_level(db).to_string());
                }
                if let Some(on) = s.on {
                    set(&dict, &format!("MIXER:Current/{table}/{field}/On"), x, y, u8::from(on).to_string());
                }
                if let Some(p) = s.pan {
                    set(&dict, &format!("MIXER:Current/{table}/{field}/Pan"), x, y, pan_value(p).to_string());
                }
                if let Some(pre) = s.pre {
                    set(&dict, &format!("MIXER:Current/{table}/{field}/PrePost"), x, y, u8::from(pre).to_string());
                }
            }
        }
        if what.preamps && c.kind == ChannelKind::Input {
            if let Some(p) = c.source.as_deref().and_then(|s| show.preamp(s)) {
                if let Some(g) = p.gain_db {
                    if dict.get("IO:Current/InCh/HAGain").is_some() {
                        set(&dict, "IO:Current/InCh/HAGain", x, 0, (g.round() as i64).to_string());
                    } else {
                        set(&dict, &format!("MIXER:Current/{table}/Port/HA/Gain"), x, 0, ((g * 100.0).round() as i64).to_string());
                    }
                }
                if let Some(ph) = p.phantom {
                    if dict.get("IO:Current/InCh/48VOn").is_some() {
                        set(&dict, "IO:Current/InCh/48VOn", x, 0, u8::from(ph).to_string());
                    } else {
                        set(&dict, &format!("MIXER:Current/{table}/Port/HA/48VOn"), x, 0, u8::from(ph).to_string());
                    }
                }
            }
        }
    }
    for b in &show.buses {
        let Some((table, x)) = bus_index(b) else { continue };
        if what.names {
            set(&dict, &format!("MIXER:Current/{table}/Label/Name"), x, 0, quote(&b.label));
            if let Some(w) = b.color.as_deref().and_then(|col| color_word(family, col)) {
                set(&dict, &format!("MIXER:Current/{table}/Label/Color"), x, 0, quote(w));
            }
        }
        if what.mutes {
            if let Some(on) = b.strip.on {
                set(&dict, &format!("MIXER:Current/{table}/Fader/On"), x, 0, u8::from(on).to_string());
            }
        }
        if what.levels {
            if let Some(db) = b.strip.fader_db {
                set(&dict, &format!("MIXER:Current/{table}/Fader/Level"), x, 0, db_level(db).to_string());
            }
        }
        if what.sends {
            for s in &b.sends {
                let Some(y) = matrix_index.get(&s.bus_id) else { continue };
                if let Some(db) = s.level_db {
                    set(&dict, &format!("MIXER:Current/{table}/ToMtrx/Level"), x, *y, db_level(db).to_string());
                }
            }
        }
    }
    for d in &show.dcas {
        let x = d.number.saturating_sub(1);
        let table = if dict.strips("DCA").is_some() { "DCA" } else { "DcaCh" };
        if what.names {
            set(&dict, &format!("MIXER:Current/{table}/Label/Name"), x, 0, quote(&d.label));
        }
        if what.mutes {
            if let Some(on) = d.on {
                set(&dict, &format!("MIXER:Current/{table}/Fader/On"), x, 0, u8::from(on).to_string());
            }
        }
        if what.levels {
            if let Some(db) = d.fader_db {
                set(&dict, &format!("MIXER:Current/{table}/Fader/Level"), x, 0, db_level(db).to_string());
            }
        }
    }

    let mut ok = 0usize;
    let mut errors: Vec<String> = vec![];
    for line in &lines {
        match client.request(line) {
            Ok(_) => ok += 1,
            Err(e) => {
                if errors.len() < 20 {
                    errors.push(format!("{line}: {e}"));
                }
            }
        }
    }
    let mut log = vec![format!("{ok} of {} sets accepted; {skipped} values the dictionary has no writable address for", lines.len())];
    log.extend(errors);
    Ok(log)
}

// ---------------------------------------------------------------- a fake desk

#[cfg(test)]
pub mod fake {
    //! A stand-in SCP server: serves an embedded dictionary, answers get/set
    //! from a value table, and speaks the scene verbs the DM3 was seen to.
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    pub struct State {
        pub product: String,
        pub values: HashMap<(String, u32, u32), String>,
        pub scenes: Vec<(String, u32, String, String)>,
        pub recalled: Vec<(String, u32)>,
        pub stored: Vec<(String, u32)>,
        pub log: Vec<String>,
    }

    pub fn desk(state: Arc<Mutex<State>>, family: Family, serve_dictionary: bool) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_nodelay(true).unwrap();
            let r = BufReader::new(s.try_clone().unwrap());
            let dict_lines: Vec<String> = family.embedded_dictionary().lines().filter(|l| l.contains("prminfo")).map(str::to_string).collect();
            for line in r.lines() {
                let Ok(line) = line else { return };
                let t = tokenize(&line);
                let mut st = state.lock().unwrap();
                st.log.push(line.clone());
                let reply = match t.first().map(String::as_str) {
                    Some("scpmode") => format!("OK {line}"),
                    Some("devinfo") => match t.get(1).map(String::as_str) {
                        Some("productname") => format!("OK devinfo productname {}", quote(&st.product)),
                        Some("version") => "OK devinfo version \"V3.00\"".into(),
                        Some("devicename") => "OK devinfo devicename \"Y001-Yamaha-DM3\"".into(),
                        _ => format!("ERROR devinfo InvalidArgument"),
                    },
                    Some("prmnum") if serve_dictionary => format!("OK prmnum {}", dict_lines.len()),
                    Some("prminfo") if serve_dictionary => {
                        let i: usize = t.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                        dict_lines.get(i).cloned().unwrap_or_else(|| "ERROR prminfo InvalidArgument".into())
                    }
                    Some("get") => {
                        let key = (t[1].clone(), t[2].parse().unwrap_or(0), t[3].parse().unwrap_or(0));
                        match st.values.get(&key) {
                            Some(v) => format!("OK get {} {} {} {}", key.0, key.1, key.2, v),
                            None => "ERROR get UnknownAddress".into(),
                        }
                    }
                    Some("set") => {
                        let key = (t[1].clone(), t[2].parse().unwrap_or(0), t[3].parse().unwrap_or(0));
                        let v = t.get(4).cloned().unwrap_or_default();
                        let quoted = line.contains('"');
                        st.values.insert(key.clone(), if quoted { quote(&v) } else { v.clone() });
                        format!("OK set {} {} {} {}", key.0, key.1, key.2, if quoted { quote(&v) } else { v })
                    }
                    Some("ssnum_ex") | Some("ssnumt_ex") => format!("OK {} {} 100", t[0], t[1]),
                    Some("ssinfo_ex") | Some("ssinfot_ex") => {
                        let list = t[1].clone();
                        let slot: u32 = t[2].trim_end_matches(".00").parse().unwrap_or(0);
                        match st.scenes.iter().find(|s| s.0 == list && s.1 == slot) {
                            Some((_, _, title, comment)) => format!("OK {} {list} {slot} \"{slot}\" {} {} user", t[0], quote(title), quote(comment)),
                            None => format!("OK {} {list} {slot} \"{slot}\" \"\" \"\" empty", t[0]),
                        }
                    }
                    Some("ssrecall_ex") | Some("ssrecallt_ex") => {
                        let slot: u32 = t[2].trim_end_matches(".00").parse().unwrap_or(0);
                        st.recalled.push((t[1].clone(), slot));
                        let _ = s.write_all(format!("NOTIFY sscurrent_ex {} {slot}\n", t[1]).as_bytes());
                        "OK ssrecall_ex".into()
                    }
                    Some("ssupdate_ex") | Some("ssupdatet_ex") => {
                        let slot: u32 = t[2].trim_end_matches(".00").parse().unwrap_or(0);
                        st.stored.push((t[1].clone(), slot));
                        "OK ssupdate_ex".into()
                    }
                    Some("sscurrent_ex") | Some("sscurrentt_ex") => format!("OK {} {} 19 modified", t[0], t[1]),
                    _ => "ERROR unknown UnknownCommand".into(),
                };
                drop(st);
                let _ = s.write_all(format!("{reply}\n").as_bytes());
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
    fn line_grammar() {
        assert_eq!(tokenize(r#"OK get MIXER:Current/InCh/Label/Name 0 0 "Kick Drum""#), vec!["OK", "get", "MIXER:Current/InCh/Label/Name", "0", "0", "Kick Drum"]);
        assert_eq!(tokenize(r#"OK devinfo productname "DM3""#)[3], "DM3");
        assert_eq!(tokenize("OK ssinfo_ex scene_a 30 \"30\" \"ins on\" \"\" user"), vec!["OK", "ssinfo_ex", "scene_a", "30", "30", "ins on", "", "user"]);
        assert_eq!(parse_reply("ERROR get UnknownAddress"), Reply::Error { command: "get".into(), reason: "UnknownAddress".into() });
        assert_eq!(parse_reply("OKm set IO:Current/InCh/HAGain 8 0 64 \"+64\""), Reply::Ok { tokens: vec!["set".into(), "IO:Current/InCh/HAGain".into(), "8".into(), "0".into(), "64".into(), "+64".into()], modified: true });
        assert!(matches!(parse_reply("NOTIFY set MIXER:Current/St/Fader/On 0 0 0 \"OFF\""), Reply::Notify { .. }));
        assert_eq!(quote("a \"b\""), "\"a \\\"b\\\"\"");
        assert_eq!(level_db(-32768), FADER_OFF);
        assert_eq!(level_db(-300), -3.0);
        assert_eq!(db_level(0.0), 0);
        assert_eq!(db_level(FADER_OFF), -32768);
    }

    #[test]
    fn dictionaries_parse_for_every_family() {
        for f in [Family::ClQl, Family::Tf, Family::Dm3, Family::Dm7, Family::Rivage] {
            let d = Dictionary::embedded(f);
            assert!(d.params.len() > 100, "{f:?}: {}", d.params.len());
            assert!(d.strips("InCh").unwrap() >= 16, "{f:?}");
            let p = d.get("MIXER:Current/InCh/Fader/Level").unwrap();
            assert!(p.is_level());
            assert!(p.readable());
        }
        assert_eq!(Dictionary::embedded(Family::Dm3).strips("InCh"), Some(16));
        assert_eq!(Dictionary::embedded(Family::ClQl).strips("InCh"), Some(72));
        assert_eq!(Dictionary::embedded(Family::Dm3).get("MIXER:Current/InCh/ToMix/Level").unwrap().y_count, 6);
        assert_eq!(Dictionary::embedded(Family::ClQl).get("MIXER:Current/InCh/Port/HA/Gain").unwrap().unit, "dB");
        assert_eq!(Family::from_product("DM3"), Some(Family::Dm3));
        assert_eq!(Family::from_product("QL5"), Some(Family::ClQl));
        assert_eq!(Family::from_product("CS-R10"), Some(Family::Rivage));
    }

    #[test]
    fn pulls_pushes_and_recalls_through_a_fake_dm3() {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut st = state.lock().unwrap();
            st.product = "DM3".into();
            let mut set = |a: &str, x: u32, y: u32, v: &str| {
                st.values.insert((a.into(), x, y), v.into());
            };
            for x in 0..16u32 {
                set("MIXER:Current/InCh/Fader/Level", x, 0, if x == 0 { "-300" } else { "-32768" });
                set("MIXER:Current/InCh/Fader/On", x, 0, if x == 1 { "0" } else { "1" });
                set("MIXER:Current/InCh/Label/Name", x, 0, &quote(if x == 0 { "Kick" } else { "" }));
                set("MIXER:Current/InCh/Label/Color", x, 0, &quote(if x == 0 { "SkyBlue" } else { "Blue" }));
                set("MIXER:Current/InCh/ToSt/Pan", x, 0, if x == 0 { "-32" } else { "0" });
                set("MIXER:Current/InCh/ToSt/On", x, 0, "1");
                for y in 0..6u32 {
                    set("MIXER:Current/InCh/ToMix/Level", x, y, if x == 0 && y == 2 { "-1000" } else { "-32768" });
                    set("MIXER:Current/InCh/ToMix/On", x, y, "1");
                    set("MIXER:Current/InCh/ToMix/PrePost", x, y, "1");
                }
                set("IO:Current/InCh/HAGain", x, 0, if x == 0 { "23" } else { "0" });
                set("IO:Current/InCh/48VOn", x, 0, if x == 0 { "1" } else { "0" });
            }
            for x in 0..6u32 {
                set("MIXER:Current/Mix/Label/Name", x, 0, &quote(if x == 2 { "Wedge" } else { "" }));
                set("MIXER:Current/Mix/Fader/Level", x, 0, "0");
                set("MIXER:Current/Mix/Fader/On", x, 0, "1");
                for y in 0..2u32 {
                    set("MIXER:Current/Mix/ToMtrx/Level", x, y, if x == 2 && y == 0 { "-600" } else { "-32768" });
                }
            }
            for x in 0..2u32 {
                set("MIXER:Current/Mtrx/Label/Name", x, 0, &quote(""));
                set("MIXER:Current/Mtrx/Fader/Level", x, 0, "0");
                set("MIXER:Current/St/Label/Name", x, 0, &quote("Stereo"));
                set("MIXER:Current/St/Fader/Level", x, 0, "-100");
                set("MIXER:Current/St/Fader/On", x, 0, "1");
            }
            for x in 0..6u32 {
                set("MIXER:Current/MuteGrpCtrl/On", x, 0, "0");
                set("MIXER:Current/MuteGrpCtrl/Label/Name", x, 0, &quote(if x == 0 { "Vox" } else { "" }));
            }
            st.scenes.push(("scene_a".into(), 3, "Opening".into(), "Band on".into()));
            st.scenes.push(("scene_b".into(), 1, "Interval".into(), "".into()));
        }
        let addr = desk(state.clone(), Family::Dm3, true);
        let mut c = Client::connect(&addr).unwrap();
        assert_eq!(c.devinfo("productname").unwrap(), "DM3");
        let d = c.dictionary().unwrap();
        assert_eq!(d.strips("InCh"), Some(16));
        let show = pull(&mut c, Family::Dm3, "DM3", "V3.00").unwrap();
        assert_eq!(show.platform, Platform::YamahaDm3);
        assert_eq!(show.channels.len(), 22, "every strip the dictionary sizes: 16 InCh + 2 StInCh + 4 FxRtnCh");
        let kick = &show.channels[0];
        assert_eq!(kick.label, "Kick");
        assert_eq!(kick.color.as_deref(), Some("cyan"));
        assert_eq!(kick.strip.fader_db, Some(-3.0));
        assert!((kick.strip.pan.unwrap() + 32.0 / 63.0).abs() < 0.001);
        assert_eq!(kick.strip.main_assign, Some(true));
        assert_eq!(show.channels[1].strip.on, Some(false));
        assert_eq!(show.channels[1].label, "ch 2");
        let s = kick.sends.iter().find(|s| s.bus_id == "bus:aux:3").unwrap();
        assert_eq!(s.level_db, Some(-10.0));
        assert_eq!(s.pre, Some(true));
        assert_eq!(kick.source.as_deref(), Some("skt:local:in:1"));
        let pre = show.preamp("skt:local:in:1").unwrap();
        assert_eq!(pre.gain_db, Some(23.0));
        assert_eq!(pre.phantom, Some(true));
        let auxes: Vec<_> = show.buses_of(BusKind::Aux).collect();
        assert_eq!(auxes.len(), 6);
        assert_eq!(auxes[2].label, "Wedge");
        assert_eq!(auxes[2].sends[0].bus_id, "bus:mtx:1");
        assert_eq!(auxes[2].sends[0].level_db, Some(-6.0));
        assert_eq!(show.buses_of(BusKind::Main).count(), 2);
        assert_eq!(show.mute_groups.len(), 6);
        assert_eq!(show.mute_groups[0].label, "Vox");
        assert_eq!(show.scenes.len(), 2);
        assert_eq!(show.scenes[0].id, "scene:a:3");
        assert_eq!(show.scenes[0].label, "Opening");
        assert_eq!(show.scenes[1].bank.as_deref(), Some("B"));
        assert_eq!(show.system.extra["currentScene"], "scene_a 19");
        assert!(show.validate().is_empty(), "{:?}", show.validate());

        // Push a change back.
        let mut show = show;
        show.channels[0].label = "Kick In".into();
        show.channels[0].strip.fader_db = Some(-6.0);
        show.channels[0].color = Some("pink".into());
        let log = push(&mut c, Family::Dm3, &show, &PushWhat { names: true, mutes: true, levels: true, sends: true, preamps: true }).unwrap();
        assert!(log[0].contains("sets accepted"), "{log:?}");
        c.recall_scene(Family::Dm3, "scene_a", 3).unwrap();
        c.store_scene(Family::Dm3, "scene_b", 30).unwrap();
        let st = state.lock().unwrap();
        assert_eq!(st.values[&("MIXER:Current/InCh/Label/Name".to_string(), 0, 0)], "\"Kick In\"");
        assert_eq!(st.values[&("MIXER:Current/InCh/Label/Color".to_string(), 0, 0)], "\"Pink\"");
        assert_eq!(st.values[&("MIXER:Current/InCh/Fader/Level".to_string(), 0, 0)], "-600");
        assert_eq!(st.values[&("IO:Current/InCh/HAGain".to_string(), 0, 0)], "23");
        assert_eq!(st.recalled, vec![("scene_a".to_string(), 3)]);
        assert_eq!(st.stored, vec![("scene_b".to_string(), 30)]);
        assert!(c.notifies.iter().any(|n| n.first().map(String::as_str) == Some("sscurrent_ex")));
    }

    #[test]
    fn falls_back_to_the_embedded_dictionary() {
        let state = Arc::new(Mutex::new(State::default()));
        {
            let mut st = state.lock().unwrap();
            st.product = "QL5".into();
            st.values.insert(("MIXER:Current/InCh/Label/Name".into(), 3, 0), quote("Vox"));
            st.values.insert(("MIXER:Current/InCh/Patch".into(), 3, 0), quote("DANTE1"));
            st.values.insert(("MIXER:Current/InCh/Port/HA/Gain".into(), 3, 0), "2500".into());
        }
        let addr = desk(state, Family::ClQl, false);
        let mut c = Client::connect(&addr).unwrap();
        assert!(c.dictionary().is_none());
        let show = pull(&mut c, Family::ClQl, "QL5", "5.8").unwrap();
        assert_eq!(show.channels.len(), 88, "the embedded CL/QL dictionary sizes the tables: 72 InCh + 16 StInCh");
        assert_eq!(show.channels[3].label, "Vox");
        assert_eq!(show.channels[3].source.as_deref(), Some("skt:dante:in:1"));
        assert_eq!(show.preamp("skt:dante:in:1").unwrap().gain_db, Some(25.0));
    }
}

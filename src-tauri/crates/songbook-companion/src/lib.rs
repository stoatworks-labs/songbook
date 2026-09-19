//! Bitfocus Companion pages from a show, and back.
//!
//! The export is Companion's own `.companionconfig` JSON (`version: 12`, the
//! format Companion 5.0 writes; older Companions upgrade it on import): one
//! `button-layered` control per channel mute, DCA mute, mute group and scene
//! on 8×4 pages, each carrying one action for the desk's Companion module:
//!
//! | platform | module | action | options |
//! |---|---|---|---|
//! | SQ / Qu | `allenheath-sq` | `mute_input` / `mute_dca` / `mute_mutegroup` | `strip` (0-based), `mute` `0` toggle |
//! | SQ / Qu | `allenheath-sq` | `scene_recall` | `scene` (1-based) |
//! | dLive | `allenheath-dlive` | `mute` | `channelType` `input`/`dca`/`mute_group`, `input`/`dca`/`muteGroup` (0-based), `mute` bool — two steps, on then off |
//! | dLive | `allenheath-dlive` | `recallScene` | `scene` (0-based) |
//! | Avantis | `allenheath-avantis` | `mute_input` / `mute_dca` / `mute_group` | `channel` (1-based), `mute` bool — two steps |
//! | Avantis | `allenheath-avantis` | `scene_recall` | `sceneNumber` (1-based) |
//! | Yamaha | `yamaha-rcp` | `MIXER_Current/InCh/Fader/On` … | `X` (1-based), `Y` `1`, `Val` `Toggle` |
//! | Yamaha | `yamaha-rcp` | `MIXER_Lib/Scene/Recall` (CL/QL, PM) or `MIXER_Lib/Bank/Scene/Recall` (TF/DM3/DM7, `Y` bank) | `Val` slot |
//!
//! Option ids were read from the module bundles installed with Companion
//! 5.0.5 (allenheath-sq 3.1.0, allenheath-dlive 1.0.1, allenheath-avantis
//! 1.0.0, yamaha-rcp 3.5.12). The control shape is the one Companion 5.0.5
//! saves itself. A generated page has been validated against those shapes
//! but has not yet been imported into a running Companion from this code.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use songbook_model::{ChannelKind, Platform, Show};

pub const FILE_VERSION: u64 = 12;
pub const COLUMNS: u32 = 8;
pub const ROWS: u32 = 4;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Connection label in Companion, e.g. `sq5` or `dm3`.
    pub connection_label: String,
    /// Desk address written into the connection config.
    pub host: String,
    pub include_channels: bool,
    pub include_dcas: bool,
    pub include_mute_groups: bool,
    pub include_scenes: bool,
    pub page_name: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { connection_label: "desk".into(), host: "192.168.1.60".into(), include_channels: true, include_dcas: true, include_mute_groups: true, include_scenes: true, page_name: String::new() }
    }
}

fn module_for(platform: Platform) -> Option<(&'static str, &'static str)> {
    match platform {
        Platform::AhSq | Platform::AhQu => Some(("allenheath-sq", "3.1.0")),
        Platform::AhDlive => Some(("allenheath-dlive", "1.0.1")),
        Platform::AhAvantis => Some(("allenheath-avantis", "1.0.0")),
        p if p.is_yamaha() => Some(("yamaha-rcp", "3.5.12")),
        _ => None,
    }
}

fn nanoid(seed: &str) -> String {
    use sha2::Digest;
    let h = sha2::Sha256::digest(seed.as_bytes());
    let alphabet: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    h.iter().take(21).map(|b| alphabet[(*b as usize) % alphabet.len()] as char).collect()
}

fn v(value: Value) -> Value {
    json!({"value": value, "isExpression": false})
}

fn text_layer(text: &str) -> Value {
    json!({
        "id": "text0", "name": "", "usage": "auto", "type": "text",
        "enabled": v(json!(true)), "opacity": v(json!(100)),
        "x": v(json!(0)), "y": v(json!(0)), "width": v(json!(100)), "height": v(json!(100)), "rotation": v(json!(0)),
        "text": v(json!(text)), "color": v(json!(16777215)),
        "halign": v(json!("center")), "valign": v(json!("center")),
        "fontsize": v(json!("auto")), "fontsizeAllowShrink": v(json!(true)), "font": v(json!("companion-sans")),
        "outlineColor": v(json!(4278190080u64))
    })
}

fn action(connection_id: &str, definition: &str, options: &Map<String, Value>, seed: &str) -> Value {
    let opts: Map<String, Value> = options.iter().map(|(k, val)| (k.clone(), v(val.clone()))).collect();
    json!({"id": nanoid(seed), "definitionId": definition, "connectionId": connection_id, "options": opts, "upgradeIndex": -1, "type": "action"})
}

/// A button with one or two steps (a second step makes an on/off pair on
/// modules whose mute action is not a toggle).
fn button(text: &str, bg: u32, connection_id: &str, steps: &[(&str, Map<String, Value>)], seed: &str) -> Value {
    let mut step_map = Map::new();
    for (i, (definition, options)) in steps.iter().enumerate() {
        step_map.insert(
            i.to_string(),
            json!({"action_sets": {"down": [action(connection_id, definition, options, &format!("{seed}-{i}"))], "up": []}, "options": {"runWhileHeld": []}}),
        );
    }
    json!({
        "type": "button-layered",
        "style": {"layers": [
            {"id": "canvas", "name": "Canvas", "usage": "auto", "type": "canvas", "decoration": v(json!("default")), "showStatusIcons": v(json!("default"))},
            {"id": "box0", "name": "Background", "usage": "auto", "type": "box", "enabled": v(json!(true)), "opacity": v(json!(100)),
             "x": v(json!(0)), "y": v(json!(0)), "width": v(json!(100)), "height": v(json!(100)), "rotation": v(json!(0)),
             "color": v(json!(bg)), "borderWidth": v(json!(0)), "borderColor": v(json!(0)), "borderPosition": v(json!("inside"))},
            text_layer(text)
        ]},
        "options": {"stepProgression": "auto", "stepExpression": "", "rotaryActions": false, "canModifyStyleInApis": false, "notes": ""},
        "feedbacks": [],
        "steps": step_map,
        "localVariables": []
    })
}

const BG_MUTE: u32 = 0x5f1f1f;
const BG_DCA: u32 = 0x4b2a6f;
const BG_MUTE_GROUP: u32 = 0x6f4b1f;
const BG_SCENE: u32 = 0x1f3a5f;

fn color_bg(color: Option<&str>, default: u32) -> u32 {
    match color {
        Some("red") => 0x7a1f1f,
        Some("green") => 0x1f5f2a,
        Some("yellow") => 0x6f6a1f,
        Some("blue") => 0x1f3a7a,
        Some("purple") => 0x4b2a6f,
        Some("cyan") => 0x1f5f6f,
        Some("orange") => 0x7a4a1f,
        Some("pink") => 0x7a2a5f,
        Some("white") => 0x4a4a4a,
        _ => default,
    }
}

struct Plan {
    text: String,
    bg: u32,
    steps: Vec<(&'static str, Map<String, Value>)>,
    seed: String,
}

fn map(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn yamaha_family(platform: Platform) -> &'static str {
    match platform {
        Platform::YamahaClQl => "CL/QL",
        Platform::YamahaTf => "TF",
        Platform::YamahaDm3 => "DM3",
        Platform::YamahaDm7 => "DM7",
        Platform::YamahaRivage => "PM",
        _ => "",
    }
}

fn plan(show: &Show, opts: &ExportOptions) -> Vec<Plan> {
    let mut out = vec![];
    let p = show.platform;
    if opts.include_channels {
        for c in show.channels.iter().filter(|c| c.kind == ChannelKind::Input) {
            let n = c.number;
            let steps: Vec<(&'static str, Map<String, Value>)> = match p {
                Platform::AhSq | Platform::AhQu => vec![("mute_input", map(&[("strip", json!(n - 1)), ("mute", json!(0))]))],
                Platform::AhDlive => vec![
                    ("mute", map(&[("channelType", json!("input")), ("input", json!(n - 1)), ("mute", json!(true))])),
                    ("mute", map(&[("channelType", json!("input")), ("input", json!(n - 1)), ("mute", json!(false))])),
                ],
                Platform::AhAvantis => vec![("mute_input", map(&[("channel", json!(n)), ("mute", json!(true))])), ("mute_input", map(&[("channel", json!(n)), ("mute", json!(false))]))],
                _ if p.is_yamaha() => vec![("MIXER_Current/InCh/Fader/On", map(&[("X", json!(n)), ("Y", json!(1)), ("Val", json!("Toggle"))]))],
                _ => continue,
            };
            out.push(Plan { text: format!("{}\n{}", n, c.label), bg: color_bg(c.color.as_deref(), BG_MUTE), steps, seed: format!("mute-{}", c.id) });
        }
    }
    if opts.include_dcas {
        for d in &show.dcas {
            let n = d.number;
            let steps: Vec<(&'static str, Map<String, Value>)> = match p {
                Platform::AhSq | Platform::AhQu => vec![("mute_dca", map(&[("strip", json!(n - 1)), ("mute", json!(0))]))],
                Platform::AhDlive => vec![
                    ("mute", map(&[("channelType", json!("dca")), ("dca", json!(n - 1)), ("mute", json!(true))])),
                    ("mute", map(&[("channelType", json!("dca")), ("dca", json!(n - 1)), ("mute", json!(false))])),
                ],
                Platform::AhAvantis => vec![("mute_dca", map(&[("channel", json!(n)), ("mute", json!(true))])), ("mute_dca", map(&[("channel", json!(n)), ("mute", json!(false))]))],
                _ if p.is_yamaha() => vec![("MIXER_Current/DCA/Fader/On", map(&[("X", json!(n)), ("Y", json!(1)), ("Val", json!("Toggle"))]))],
                _ => continue,
            };
            out.push(Plan { text: format!("DCA {}\n{}", n, d.label), bg: color_bg(d.color.as_deref(), BG_DCA), steps, seed: format!("dca-{}", d.id) });
        }
    }
    if opts.include_mute_groups {
        for m in &show.mute_groups {
            let n = m.number;
            let steps: Vec<(&'static str, Map<String, Value>)> = match p {
                Platform::AhSq | Platform::AhQu => vec![("mute_mutegroup", map(&[("strip", json!(n - 1)), ("mute", json!(0))]))],
                Platform::AhDlive => vec![
                    ("mute", map(&[("channelType", json!("mute_group")), ("muteGroup", json!(n - 1)), ("mute", json!(true))])),
                    ("mute", map(&[("channelType", json!("mute_group")), ("muteGroup", json!(n - 1)), ("mute", json!(false))])),
                ],
                Platform::AhAvantis => vec![("mute_group", map(&[("channel", json!(n)), ("mute", json!(true))])), ("mute_group", map(&[("channel", json!(n)), ("mute", json!(false))]))],
                Platform::YamahaClQl | Platform::YamahaRivage => vec![("MIXER_Current/MuteMaster/On", map(&[("X", json!(n)), ("Y", json!(1)), ("Val", json!("Toggle"))]))],
                _ if p.is_yamaha() => vec![("MIXER_Current/MuteGrpCtrl/On", map(&[("X", json!(n)), ("Y", json!(1)), ("Val", json!("Toggle"))]))],
                _ => continue,
            };
            out.push(Plan { text: format!("MUTE {}\n{}", n, m.label), bg: BG_MUTE_GROUP, steps, seed: format!("mg-{}", m.id) });
        }
    }
    if opts.include_scenes {
        for s in &show.scenes {
            let Some(n) = s.number else { continue };
            let steps: Vec<(&'static str, Map<String, Value>)> = match p {
                Platform::AhSq | Platform::AhQu => vec![("scene_recall", map(&[("scene", json!(n))]))],
                Platform::AhDlive => vec![("recallScene", map(&[("scene", json!(n - 1))]))],
                Platform::AhAvantis => vec![("scene_recall", map(&[("sceneNumber", json!(n))]))],
                Platform::YamahaClQl => vec![("MIXER_Lib/Scene/Recall", map(&[("X", json!(1)), ("Y", json!(1)), ("Val", json!(n))]))],
                Platform::YamahaRivage => vec![("MIXER_Lib/Scene/Recall", map(&[("X", json!(1)), ("Y", json!(1)), ("Val", json!(format!("{n}.00")))]))],
                Platform::YamahaDm7 => vec![("MIXER_Lib/Bank/Scene/Recall", map(&[("X", json!(1)), ("Y", json!(if s.bank.as_deref() == Some("B") { 2 } else { 1 })), ("Val", json!(format!("{n}.00")))]))],
                _ if p.is_yamaha() => vec![("MIXER_Lib/Bank/Scene/Recall", map(&[("X", json!(1)), ("Y", json!(if s.bank.as_deref() == Some("B") { 2 } else { 1 })), ("Val", json!(n))]))],
                _ => continue,
            };
            let bank = s.bank.as_deref().map(|b| b.to_string()).unwrap_or_default();
            out.push(Plan { text: format!("SCENE {bank}{}\n{}", n, s.label), bg: BG_SCENE, steps, seed: format!("scene-{}", s.id) });
        }
    }
    out
}

/// Build a Companion export. One page → `type: "page"`; more → `type: "full"`.
pub fn export(show: &Show, opts: &ExportOptions) -> Result<Value, String> {
    let (module, module_version) = module_for(show.platform).ok_or_else(|| format!("no Companion module mapping for {}", show.platform.label()))?;
    let connection_id = nanoid(&format!("conn-{}-{}", module, opts.connection_label));
    let plans = plan(show, opts);
    let per_page = (COLUMNS * ROWS) as usize;
    let page_count = plans.len().div_ceil(per_page).max(1);
    let mut pages: Map<String, Value> = Map::new();
    for page in 0..page_count {
        let mut controls: Map<String, Value> = Map::new();
        for (i, p) in plans.iter().skip(page * per_page).take(per_page).enumerate() {
            let row = (i as u32) / COLUMNS;
            let col = (i as u32) % COLUMNS;
            let entry = controls.entry(row.to_string()).or_insert_with(|| json!({}));
            entry[col.to_string()] = button(&p.text, p.bg, &connection_id, &p.steps, &format!("{}-{}", show.id, p.seed));
        }
        let name = if opts.page_name.is_empty() { show.meta.name.clone() } else { opts.page_name.clone() };
        let name = if page_count > 1 { format!("{name} {}", page + 1) } else { name };
        pages.insert((page + 1).to_string(), json!({"id": nanoid(&format!("page-{}-{}", show.id, page)), "name": name, "controls": controls, "gridSize": {"minColumn": 0, "maxColumn": COLUMNS - 1, "minRow": 0, "maxRow": ROWS - 1}}));
    }
    let config = match module {
        "yamaha-rcp" => json!({"host": opts.host, "model": yamaha_family(show.platform)}),
        "allenheath-sq" => json!({"host": opts.host, "model": show.system.model.replace('-', ""), "midich": 1}),
        _ => json!({"host": opts.host}),
    };
    let instances = json!({connection_id: {"label": opts.connection_label, "moduleId": module, "moduleVersionId": module_version, "updatePolicy": "stable", "sortOrder": 0, "isFirstInit": false, "lastUpgradeIndex": -1, "enabled": true, "config": config, "secrets": {}}});
    let build = format!("songbook {}", env!("CARGO_PKG_VERSION"));
    if page_count == 1 {
        let page = pages.remove("1").unwrap();
        Ok(json!({"version": FILE_VERSION, "type": "page", "companionBuild": build, "page": page, "instances": instances, "connectionCollections": [], "oldPageNumber": 1}))
    } else {
        Ok(json!({"version": FILE_VERSION, "type": "full", "companionBuild": build, "pages": pages, "instances": instances, "connectionCollections": []}))
    }
}

// ---------------------------------------------------------------- import

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedButton {
    pub page: u32,
    pub row: u32,
    pub column: u32,
    pub text: String,
    pub module: String,
    pub definition: String,
    pub options: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub file_version: u64,
    pub kind: String,
    pub pages: Vec<String>,
    pub connections: Vec<String>,
    pub buttons: Vec<ImportedButton>,
    pub unmatched: usize,
}

/// Read a `.companionconfig` (JSON, or gzip of it) and match its buttons
/// against the show.
pub fn import(bytes: &[u8], show: &Show) -> Result<ImportReport, String> {
    let text = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut dec = flate2::read::GzDecoder::new(bytes);
        let mut s = String::new();
        std::io::Read::read_to_string(&mut dec, &mut s).map_err(|e| e.to_string())?;
        s
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("not JSON ({e}); Companion also writes YAML, which Songbook does not read yet"))?;
    let kind = doc.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    let mut report = ImportReport { file_version: doc.get("version").and_then(Value::as_u64).unwrap_or(0), kind: kind.clone(), ..Default::default() };
    let instances: Map<String, Value> = doc.get("instances").and_then(Value::as_object).cloned().unwrap_or_default();
    let module_of = |conn: &str| instances.get(conn).and_then(|i| i.get("moduleId")).and_then(Value::as_str).unwrap_or("").to_string();
    report.connections = instances.values().filter_map(|i| i.get("label").and_then(Value::as_str).map(str::to_string)).collect();
    let mut pages: Vec<(u32, Value)> = vec![];
    match kind.as_str() {
        "page" => pages.push((doc.get("oldPageNumber").and_then(Value::as_u64).unwrap_or(1) as u32, doc.get("page").cloned().unwrap_or(Value::Null))),
        "full" => {
            if let Some(ps) = doc.get("pages").and_then(Value::as_object) {
                let mut keys: Vec<(u32, &String)> = ps.keys().map(|k| (k.parse().unwrap_or(0), k)).collect();
                keys.sort();
                for (n, k) in keys {
                    pages.push((n, ps[k].clone()));
                }
            }
        }
        other => return Err(format!("unsupported export type {other:?}")),
    }
    for (pn, page) in pages {
        report.pages.push(page.get("name").and_then(Value::as_str).unwrap_or("").to_string());
        let Some(rows) = page.get("controls").and_then(Value::as_object) else { continue };
        for (r, cols) in rows {
            let Some(cols) = cols.as_object() else { continue };
            for (c, control) in cols {
                let text = button_text(control);
                // Only the first step's actions: the second of a mute pair says the same thing.
                let actions = control.pointer("/steps/0/action_sets/down").and_then(Value::as_array).cloned().unwrap_or_default();
                for a in actions {
                    let definition = a.get("definitionId").and_then(Value::as_str).unwrap_or("").to_string();
                    let conn = a.get("connectionId").and_then(Value::as_str).unwrap_or("");
                    let module = module_of(conn);
                    let options: Map<String, Value> = a.get("options").and_then(Value::as_object).map(|o| o.iter().map(|(k, x)| (k.clone(), x.get("value").cloned().unwrap_or(x.clone()))).collect()).unwrap_or_default();
                    let (matched, problem) = match_action(show, &module, &definition, &options);
                    if problem.is_some() {
                        report.unmatched += 1;
                    }
                    report.buttons.push(ImportedButton { page: pn, row: r.parse().unwrap_or(0), column: c.parse().unwrap_or(0), text: text.clone(), module, definition, options, matched, problem });
                }
            }
        }
    }
    Ok(report)
}

fn button_text(control: &Value) -> String {
    if let Some(layers) = control.pointer("/style/layers").and_then(Value::as_array) {
        for l in layers {
            if l.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(t) = l.pointer("/text/value").and_then(Value::as_str) {
                    return t.to_string();
                }
            }
        }
    }
    control.pointer("/style/text").and_then(Value::as_str).unwrap_or("").to_string()
}

fn num(options: &Map<String, Value>, key: &str) -> Option<u32> {
    match options.get(key)? {
        Value::Number(n) => n.as_u64().map(|v| v as u32),
        Value::String(s) => s.trim_end_matches(".00").parse().ok(),
        _ => None,
    }
}

fn match_action(show: &Show, module: &str, definition: &str, options: &Map<String, Value>) -> (Option<String>, Option<String>) {
    let channel = |n: u32| match show.channels.iter().find(|c| c.kind == ChannelKind::Input && c.number == n) {
        Some(c) => (Some(c.id.clone()), None),
        None => (None, Some(format!("input channel {n} is not in the show"))),
    };
    let dca = |n: u32| match show.dcas.iter().find(|d| d.number == n) {
        Some(d) => (Some(d.id.clone()), None),
        None => (None, Some(format!("DCA {n} is not in the show"))),
    };
    let mg = |n: u32| match show.mute_groups.iter().find(|m| m.number == n) {
        Some(m) => (Some(m.id.clone()), None),
        None => (None, Some(format!("mute group {n} is not in the show"))),
    };
    let scene = |n: u32, bank: Option<&str>| match show.scenes.iter().find(|s| s.number == Some(n) && (bank.is_none() || s.bank.as_deref() == bank)) {
        Some(s) => (Some(s.id.clone()), None),
        None => (None, Some(format!("scene {n} is not in the show"))),
    };
    match (module, definition) {
        ("allenheath-sq", "mute_input") => num(options, "strip").map(|s| channel(s + 1)).unwrap_or((None, Some("no strip".into()))),
        ("allenheath-sq", "mute_dca") => num(options, "strip").map(|s| dca(s + 1)).unwrap_or((None, Some("no strip".into()))),
        ("allenheath-sq", "mute_mutegroup") => num(options, "strip").map(|s| mg(s + 1)).unwrap_or((None, Some("no strip".into()))),
        ("allenheath-sq", "scene_recall") => num(options, "scene").map(|s| scene(s, None)).unwrap_or((None, Some("no scene".into()))),
        ("allenheath-dlive", "mute") => match options.get("channelType").and_then(Value::as_str) {
            Some("input") => num(options, "input").map(|s| channel(s + 1)).unwrap_or((None, Some("no input".into()))),
            Some("dca") => num(options, "dca").map(|s| dca(s + 1)).unwrap_or((None, Some("no dca".into()))),
            Some("mute_group") => num(options, "muteGroup").map(|s| mg(s + 1)).unwrap_or((None, Some("no mute group".into()))),
            _ => (None, None),
        },
        ("allenheath-dlive", "recallScene") => num(options, "scene").map(|s| scene(s + 1, None)).unwrap_or((None, Some("no scene".into()))),
        ("allenheath-avantis", "mute_input") => num(options, "channel").map(channel).unwrap_or((None, Some("no channel".into()))),
        ("allenheath-avantis", "mute_dca") => num(options, "channel").map(dca).unwrap_or((None, Some("no channel".into()))),
        ("allenheath-avantis", "mute_group") => num(options, "channel").map(mg).unwrap_or((None, Some("no channel".into()))),
        ("allenheath-avantis", "scene_recall") => num(options, "sceneNumber").map(|s| scene(s, None)).unwrap_or((None, Some("no scene".into()))),
        ("yamaha-rcp", "MIXER_Current/InCh/Fader/On") => num(options, "X").map(channel).unwrap_or((None, Some("no X".into()))),
        ("yamaha-rcp", "MIXER_Current/DCA/Fader/On") => num(options, "X").map(dca).unwrap_or((None, Some("no X".into()))),
        ("yamaha-rcp", "MIXER_Current/MuteMaster/On") | ("yamaha-rcp", "MIXER_Current/MuteGrpCtrl/On") => num(options, "X").map(mg).unwrap_or((None, Some("no X".into()))),
        ("yamaha-rcp", "MIXER_Lib/Scene/Recall") => num(options, "Val").map(|s| scene(s, None)).unwrap_or((None, Some("no Val".into()))),
        ("yamaha-rcp", "MIXER_Lib/Bank/Scene/Recall") => {
            let bank = match num(options, "Y") {
                Some(2) => Some("B"),
                _ => Some("A"),
            };
            num(options, "Val").map(|s| scene(s, bank)).unwrap_or((None, Some("no Val".into())))
        }
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use songbook_model::*;

    fn show(platform: Platform) -> Show {
        let mut s = Show::new("Gala", platform);
        s.system.model = "SQ-5".into();
        for n in 1..=3u32 {
            let mut c = Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Ch {n}"));
            c.color = Some("red".into());
            s.channels.push(c);
        }
        s.dcas.push(build::dca(1, "Band"));
        s.mute_groups.push(build::mute_group(2, "Vox"));
        let mut sc = build::scene(7, "Opening");
        if platform.is_yamaha() {
            sc.id = ids::scene_in_bank("A", 7);
            sc.bank = Some("A".into());
        }
        s.scenes.push(sc);
        s
    }

    #[test]
    fn sq_page_round_trips() {
        let s = show(Platform::AhSq);
        let doc = export(&s, &ExportOptions::default()).unwrap();
        assert_eq!(doc["type"], "page");
        let row0 = doc["page"]["controls"]["0"].as_object().unwrap();
        // 3 channels + 1 DCA + 1 mute group + 1 scene = 6 buttons.
        assert_eq!(row0.len(), 6);
        let a = &row0["0"]["steps"]["0"]["action_sets"]["down"][0];
        assert_eq!(a["definitionId"], "mute_input");
        assert_eq!(a["options"]["strip"]["value"], 0);
        assert_eq!(row0["5"]["steps"]["0"]["action_sets"]["down"][0]["options"]["scene"]["value"], 7);
        let rep = import(&serde_json::to_vec(&doc).unwrap(), &s).unwrap();
        assert_eq!(rep.buttons.len(), 6);
        assert_eq!(rep.unmatched, 0);
        assert_eq!(rep.buttons[0].matched.as_deref(), Some("ch:1"));
        assert_eq!(rep.buttons[3].matched.as_deref(), Some("dca:1"));
        assert_eq!(rep.buttons[5].matched.as_deref(), Some("scene:7"));
    }

    #[test]
    fn dlive_mutes_are_two_step_and_scenes_are_zero_based() {
        let s = show(Platform::AhDlive);
        let doc = export(&s, &ExportOptions::default()).unwrap();
        let b = &doc["page"]["controls"]["0"]["0"];
        assert_eq!(b["steps"].as_object().unwrap().len(), 2);
        assert_eq!(b["steps"]["0"]["action_sets"]["down"][0]["options"]["mute"]["value"], true);
        assert_eq!(b["steps"]["1"]["action_sets"]["down"][0]["options"]["mute"]["value"], false);
        assert_eq!(doc["page"]["controls"]["0"]["5"]["steps"]["0"]["action_sets"]["down"][0]["options"]["scene"]["value"], 6);
        let rep = import(&serde_json::to_vec(&doc).unwrap(), &s).unwrap();
        assert_eq!(rep.unmatched, 0);
        assert_eq!(rep.buttons[5].matched.as_deref(), Some("scene:7"));
    }

    #[test]
    fn yamaha_uses_rcp_addresses_and_banks() {
        let s = show(Platform::YamahaDm3);
        let doc = export(&s, &ExportOptions::default()).unwrap();
        let a = &doc["page"]["controls"]["0"]["0"]["steps"]["0"]["action_sets"]["down"][0];
        assert_eq!(a["definitionId"], "MIXER_Current/InCh/Fader/On");
        assert_eq!(a["options"]["X"]["value"], 1);
        assert_eq!(a["options"]["Val"]["value"], "Toggle");
        let sc = &doc["page"]["controls"]["0"]["5"]["steps"]["0"]["action_sets"]["down"][0];
        assert_eq!(sc["definitionId"], "MIXER_Lib/Bank/Scene/Recall");
        assert_eq!(sc["options"]["Y"]["value"], 1);
        assert_eq!(sc["options"]["Val"]["value"], 7);
        let inst = doc["instances"].as_object().unwrap().values().next().unwrap();
        assert_eq!(inst["config"]["model"], "DM3");
        let rep = import(&serde_json::to_vec(&doc).unwrap(), &s).unwrap();
        assert_eq!(rep.unmatched, 0);
        assert_eq!(rep.buttons[5].matched.as_deref(), Some("scene:a:7"));
        // A different show reports the mismatch.
        let mut other = show(Platform::YamahaDm3);
        other.scenes.clear();
        let rep = import(&serde_json::to_vec(&doc).unwrap(), &other).unwrap();
        assert_eq!(rep.unmatched, 1);
    }

    #[test]
    fn many_buttons_make_a_full_export() {
        let mut s = show(Platform::AhSq);
        for n in 4..=40u32 {
            s.channels.push(Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Ch {n}")));
        }
        let doc = export(&s, &ExportOptions::default()).unwrap();
        assert_eq!(doc["type"], "full");
        assert_eq!(doc["pages"].as_object().unwrap().len(), 2);
    }
}

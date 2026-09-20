//! Songbook — the Tauri layer.
//!
//! Thin on purpose: every decision about a show lives in the workspace crates
//! (`songbook-model`, `-ah`, `-yamaha`, `-library`, `-convert`, `-companion`)
//! and this file only wires them to the window. Commands that touch the
//! network or the disk for more than an instant run on the blocking pool.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use songbook_library::{Commit, Entry, Library};
use songbook_model::summary::Summary;
use songbook_model::{Platform, Show};
use tauri::{AppHandle, Manager, State};

mod settings;

use settings::Settings;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub config_dir: PathBuf,
    pub pending_auth: Mutex<Option<songbook_library::oauth::PendingAuth>>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn library(state: &AppState) -> CmdResult<Library> {
    let path = state.settings.lock().map_err(err)?.library_path.clone();
    Library::open(Path::new(&path)).map_err(err)
}

fn author(state: &AppState) -> Option<String> {
    state.settings.lock().ok().and_then(|s| if s.author.trim().is_empty() { None } else { Some(s.author.clone()) })
}

// ---------------------------------------------------------------- settings

#[tauri::command]
fn settings_get(state: State<AppState>) -> CmdResult<Settings> {
    Ok(state.settings.lock().map_err(err)?.clone())
}

#[tauri::command]
fn settings_set(state: State<AppState>, settings: Settings) -> CmdResult<Settings> {
    settings.save(&state.config_dir).map_err(err)?;
    *state.settings.lock().map_err(err)? = settings.clone();
    Ok(settings)
}

#[tauri::command]
fn app_info(state: State<AppState>) -> CmdResult<Value> {
    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "configDir": state.config_dir.display().to_string(),
        "platforms": Platform::all().iter().map(|p| serde_json::json!({"id": p, "label": p.label(), "vendor": p.vendor(), "models": songbook_convert::models(*p)})).collect::<Vec<_>>(),
    }))
}

// ---------------------------------------------------------------- library

#[tauri::command]
fn library_list(state: State<AppState>) -> CmdResult<Vec<Entry>> {
    library(&state)?.list().map_err(err)
}

#[tauri::command]
fn show_load(state: State<AppState>, id: String) -> CmdResult<Show> {
    library(&state)?.load(&id).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    show: Show,
    commit: Option<Commit>,
}

#[tauri::command]
fn show_save(state: State<AppState>, mut show: Show, message: String) -> CmdResult<SaveResult> {
    let lib = library(&state)?;
    let commit = lib.save(&mut show, &message, author(&state).as_deref()).map_err(err)?;
    Ok(SaveResult { show, commit })
}

#[tauri::command]
fn show_new(state: State<AppState>, name: String, platform: Platform, model: String) -> CmdResult<Show> {
    let lib = library(&state)?;
    let mut show = match platform {
        Platform::AhSq => songbook_ah::import::sq_skeleton(&name, platform, &model),
        _ => {
            let mut s = Show::new(&name, platform);
            s.system.model = model.clone();
            let cap = songbook_convert::capabilities(platform, &model);
            s.system.units.push(songbook_model::build::unit("local", &format!("{model} local sockets"), &model, songbook_model::UnitRole::Console));
            s.sockets.extend(songbook_model::build::sockets("local", songbook_model::Direction::In, cap.local_inputs, songbook_model::SocketKind::Mic, "Local"));
            s.sockets.extend(songbook_model::build::sockets("local", songbook_model::Direction::Out, cap.local_outputs, songbook_model::SocketKind::Line, "Out"));
            for n in 1..=cap.input_channels {
                s.channels.push(songbook_model::Channel::new(songbook_model::ids::channel(n), n, songbook_model::ChannelKind::Input, &format!("Ch {n}")));
            }
            for n in 1..=cap.mains {
                let label = if n == 1 { "Main".to_string() } else { format!("Main {n}") };
                s.buses.push(songbook_model::Bus::new(songbook_model::BusKind::Main, n, &label, true));
            }
            for n in 1..=cap.auxes {
                s.buses.push(songbook_model::Bus::new(songbook_model::BusKind::Aux, n, &format!("Aux {n}"), false));
            }
            for n in 1..=cap.matrices {
                s.buses.push(songbook_model::Bus::new(songbook_model::BusKind::Matrix, n, &format!("Mtx {n}"), false));
            }
            for n in 1..=cap.fx_sends {
                s.buses.push(songbook_model::Bus::new(songbook_model::BusKind::FxSend, n, &format!("FX {n}"), false));
            }
            for n in 1..=cap.dcas {
                s.dcas.push(songbook_model::build::dca(n, &format!("DCA {n}")));
            }
            for n in 1..=cap.mute_groups {
                s.mute_groups.push(songbook_model::build::mute_group(n, &format!("Mute {n}")));
            }
            s
        }
    };
    show.system.model = model;
    lib.save(&mut show, "Created", author(&state).as_deref()).map_err(err)?;
    Ok(show)
}

#[tauri::command]
fn show_history(state: State<AppState>, id: String) -> CmdResult<Vec<Commit>> {
    library(&state)?.history(&id).map_err(err)
}

#[tauri::command]
fn show_snapshot(state: State<AppState>, id: String, commit: String) -> CmdResult<Show> {
    library(&state)?.snapshot(&id, &commit).map_err(err)
}

#[tauri::command]
fn show_diff(state: State<AppState>, id: String, from: String, to: String) -> CmdResult<Vec<songbook_model::diff::Change>> {
    library(&state)?.diff(&id, &from, &to).map_err(err)
}

#[tauri::command]
fn show_restore(state: State<AppState>, id: String, commit: String) -> CmdResult<Commit> {
    library(&state)?.restore(&id, &commit, author(&state).as_deref()).map_err(err)
}

#[tauri::command]
fn show_delete(state: State<AppState>, id: String) -> CmdResult<()> {
    library(&state)?.delete(&id).map_err(err)
}

#[tauri::command]
fn show_duplicate(state: State<AppState>, id: String, name: String) -> CmdResult<Show> {
    library(&state)?.duplicate(&id, &name, author(&state).as_deref()).map_err(err)
}

#[tauri::command]
fn show_validate(show: Show) -> CmdResult<Vec<String>> {
    Ok(show.validate())
}

// ---------------------------------------------------------------- import / export of files

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    show: Show,
    summary: Summary,
    /// What the file turned out to be.
    kind: String,
}

fn import_any(lib: &Library, path: &Path, platform: Option<Platform>, author: Option<&str>) -> CmdResult<ImportResult> {
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_lowercase();
    let bytes = if path.is_dir() { vec![] } else { std::fs::read(path).map_err(err)? };

    // A Songbook JSON round-trips as a new show.
    if lower.ends_with(".json") {
        let mut s: Show = serde_json::from_slice(&bytes).map_err(|e| format!("{name}: not a Songbook show ({e})"))?;
        if !s.schema.starts_with("songbook/") {
            return Err(format!("{name}: not a Songbook show (schema {})", s.schema));
        }
        s.id = uuid::Uuid::new_v4().to_string();
        lib.save(&mut s, &format!("Imported {name}"), author).map_err(err)?;
        let summary = Summary::of(&s);
        return Ok(ImportResult { show: s, summary, kind: "songbook".into() });
    }

    /// The vendor file to keep: its name, bytes and kind.
    type Vendor<'a> = Option<(String, Vec<u8>, &'a str)>;
    let (mut show, kind, vendor): (Show, String, Vendor) = if !path.is_dir() && songbook_yamaha::looks_like(&bytes) {
        let r = songbook_yamaha::import_bytes(&name, &bytes).map_err(|e| format!("{name}: {e}"))?;
        (r.show, r.kind.to_string(), Some((name.clone(), bytes.clone(), r.kind)))
    } else if path.is_dir() || lower.ends_with(".dat") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".zip") || bytes.starts_with(&[0x1F, 0x8B]) {
        let r = songbook_ah::import::import_path(path, platform).map_err(err)?;
        let vendor = r.vendor.map(|(n, b)| (n, b, r.kind));
        (r.show, r.kind.to_string(), vendor)
    } else {
        return Err(format!("{name}: not a show file Songbook knows (an SQ show folder or NVDATA.DAT, a dLive / Avantis show .tar.gz, a Yamaha .CLF, .dm3s / .tfs / .dm7s, or a Songbook .json)"));
    };
    lib.save(&mut show, "Imported", author).map_err(err)?;
    if let Some((vname, vbytes, vkind)) = vendor {
        let platform = show.platform;
        lib.add_vendor(&mut show, platform, vkind, &vname, &vbytes, "imported file").map_err(err)?;
    }
    lib.save(&mut show, &format!("Imported {name}"), author).map_err(err)?;
    let summary = Summary::of(&show);
    Ok(ImportResult { show, summary, kind })
}

/// The import the desktop app does, for scripts and the `seed` example.
pub fn import_for_cli(lib: &Library, path: &Path, platform: Option<Platform>) -> CmdResult<Show> {
    import_any(lib, path, platform, None).map(|r| r.show)
}

#[tauri::command]
async fn import_path(app: AppHandle, path: String, platform: Option<Platform>) -> CmdResult<ImportResult> {
    let state = app.state::<AppState>();
    let lib = library(&state)?;
    let a = author(&state);
    tauri::async_runtime::spawn_blocking(move || import_any(&lib, Path::new(&path), platform, a.as_deref())).await.map_err(err)?
}


/// What `vendor_write` did.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VendorWriteResult {
    /// Where the file or folder went.
    path: String,
    /// `sq-show` or `clf`.
    kind: String,
    /// What was written and what was not, per driver.
    report: Value,
}

/// Write the show as it is on screen into a copy of one of its kept vendor
/// files: an SQ show (input patch into `NVDATA.DAT`, scene names into the
/// `SCENEnnn.DAT` images — as a folder when `path` has no `.zip` suffix) or a
/// CL/QL `.CLF` (input patch and channel names). Each image's checksum is
/// recomputed. The kept file is never modified.
#[tauri::command]
async fn vendor_write(app: AppHandle, show: Show, sha256: String, path: String) -> CmdResult<VendorWriteResult> {
    let state = app.state::<AppState>();
    let lib = library(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let blob = show.vendor.iter().find(|b| b.sha256 == sha256).ok_or("no such vendor file")?.clone();
        let bytes = lib.vendor_bytes(&show.id, &blob).map_err(err)?;
        let dest = Path::new(&path);
        let report = match blob.kind.as_str() {
            "sq-show" => {
                if path.to_lowercase().ends_with(".zip") {
                    let (out, report) = songbook_ah::write::write_zip(&bytes, &show).map_err(err)?;
                    std::fs::write(dest, out).map_err(err)?;
                    serde_json::to_value(report).map_err(err)?
                } else {
                    let report = songbook_ah::write::write_folder(&bytes, &show, dest).map_err(err)?;
                    serde_json::to_value(report).map_err(err)?
                }
            }
            "clf" => {
                let (out, report) = songbook_yamaha::clf::write(&bytes, &show).map_err(err)?;
                std::fs::write(dest, out).map_err(err)?;
                serde_json::to_value(report).map_err(err)?
            }
            other => return Err(format!("Songbook cannot write a {other} file: only SQ shows and CL/QL .CLF files have solved checksums")),
        };
        Ok(VendorWriteResult { path, kind: blob.kind, report })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn export_show_json(state: State<AppState>, id: String, path: String) -> CmdResult<()> {
    let show = library(&state)?.load(&id).map_err(err)?;
    std::fs::write(&path, serde_json::to_string_pretty(&show).map_err(err)?).map_err(err)
}

#[tauri::command]
fn vendor_export(state: State<AppState>, id: String, sha256: String, path: String) -> CmdResult<()> {
    let lib = library(&state)?;
    let show = lib.load(&id).map_err(err)?;
    let blob = show.vendor.iter().find(|b| b.sha256 == sha256).ok_or("no such vendor file")?;
    let bytes = lib.vendor_bytes(&id, blob).map_err(err)?;
    std::fs::write(&path, bytes).map_err(err)
}

/// Write bytes the webview produced (a PDF, a PNG) to a path it chose.
#[tauri::command]
fn write_file(path: String, base64: String) -> CmdResult<()> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(base64.as_bytes()).map_err(err)?;
    std::fs::write(&path, bytes).map_err(err)
}

#[tauri::command]
fn write_text(path: String, text: String) -> CmdResult<()> {
    std::fs::write(&path, text).map_err(err)
}

#[tauri::command]
fn read_text(path: String) -> CmdResult<String> {
    std::fs::read_to_string(&path).map_err(err)
}

// ---------------------------------------------------------------- devices

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceRef {
    platform: Platform,
    host: String,
    /// The desk's MIDI channel (Allen & Heath), 1–16.
    #[serde(default)]
    midi_channel: Option<u8>,
    /// Desk model, for the shape of a pulled show.
    #[serde(default)]
    model: Option<String>,
}

fn ah_opts(dev: &DeviceRef) -> songbook_ah::live::Options {
    songbook_ah::live::Options { midi_channel: dev.midi_channel.unwrap_or(1), ..Default::default() }
}

#[tauri::command]
async fn device_probe(dev: DeviceRef) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        if dev.platform.is_ah() {
            let mut d = songbook_ah::live::Device::connect(&dev.host, dev.platform, &ah_opts(&dev)).map_err(err)?;
            d.probe().map_err(err)
        } else if dev.platform.is_yamaha() {
            let mut c = songbook_yamaha::scp::Client::connect(&dev.host).map_err(err)?;
            let product = c.devinfo("productname").map_err(err)?;
            let version = c.devinfo("version").unwrap_or_default();
            let device = c.devinfo("devicename").unwrap_or_default();
            let family = songbook_yamaha::scp::Family::from_product(&product);
            let current = family.and_then(|f| c.current_scene(f));
            Ok(serde_json::json!({"ok": true, "platform": dev.platform, "product": product, "firmware": version, "deviceName": device, "family": family.map(|f| f.label()), "currentScene": current}))
        } else {
            Err(format!("no live driver for {} yet", dev.platform.label()))
        }
    })
    .await
    .map_err(err)?
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PullOptions {
    /// Existing show to update instead of creating a new one.
    #[serde(default)]
    into_show: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[tauri::command]
async fn device_pull(app: AppHandle, dev: DeviceRef, opts: PullOptions) -> CmdResult<ImportResult> {
    let state = app.state::<AppState>();
    let lib = library(&state)?;
    let a = author(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let model = dev.model.clone().unwrap_or_else(|| songbook_convert::models(dev.platform).first().map(|s| s.to_string()).unwrap_or_default());
        let mut show = if dev.platform.is_ah() {
            let mut d = songbook_ah::live::Device::connect(&dev.host, dev.platform, &ah_opts(&dev)).map_err(err)?;
            d.pull(&model).map_err(err)?
        } else if dev.platform.is_yamaha() {
            let mut c = songbook_yamaha::scp::Client::connect(&dev.host).map_err(err)?;
            let product = c.devinfo("productname").map_err(err)?;
            let version = c.devinfo("version").unwrap_or_default();
            let family = songbook_yamaha::scp::Family::from_product(&product).or_else(|| songbook_yamaha::scp::Family::from_platform(dev.platform)).ok_or_else(|| format!("{product}: not a desk family Songbook knows"))?;
            let model = if product.is_empty() { model } else { product };
            songbook_yamaha::scp::pull(&mut c, family, &model, &version).map_err(err)?
        } else {
            return Err(format!("no live driver for {} yet", dev.platform.label()));
        };
        if let Some(existing) = &opts.into_show {
            if let Ok(prev) = lib.load(existing) {
                show.id = prev.id;
                show.meta.name = prev.meta.name;
                show.meta.created = prev.meta.created;
                show.meta.notes = prev.meta.notes;
                show.meta.tags = prev.meta.tags;
                show.vendor = prev.vendor;
                // Names the desk cannot say (an SQ over MIDI) come from the show it updates.
                for c in &mut show.channels {
                    if let Some(p) = prev.channels.iter().find(|p| p.id == c.id) {
                        if c.label.starts_with("Ip") || c.label.starts_with("ch ") || c.label.starts_with("Ch ") {
                            c.label = p.label.clone();
                            c.color = c.color.clone().or_else(|| p.color.clone());
                        }
                    }
                }
            }
        }
        if let Some(n) = &opts.name {
            if !n.trim().is_empty() {
                show.meta.name = n.trim().to_string();
            }
        }
        lib.save(&mut show, &format!("Pulled from {}", dev.host), a.as_deref()).map_err(err)?;
        let summary = Summary::of(&show);
        Ok(ImportResult { show, summary, kind: "device".into() })
    })
    .await
    .map_err(err)?
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct PushOptions {
    #[serde(default)]
    names: bool,
    #[serde(default)]
    mutes: bool,
    #[serde(default)]
    levels: bool,
    #[serde(default)]
    sends: bool,
    #[serde(default)]
    preamps: bool,
}

#[tauri::command]
async fn device_push(app: AppHandle, dev: DeviceRef, id: String, opts: PushOptions) -> CmdResult<Value> {
    let state = app.state::<AppState>();
    let lib = library(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let show = lib.load(&id).map_err(err)?;
        let log = if dev.platform.is_ah() {
            let mut d = songbook_ah::live::Device::connect(&dev.host, dev.platform, &ah_opts(&dev)).map_err(err)?;
            d.push(&show, &songbook_ah::live::PushWhat { names: opts.names, mutes: opts.mutes, levels: opts.levels, sends: opts.sends, preamps: opts.preamps }).map_err(err)?
        } else if dev.platform.is_yamaha() {
            let mut c = songbook_yamaha::scp::Client::connect(&dev.host).map_err(err)?;
            let product = c.devinfo("productname").unwrap_or_default();
            let family = songbook_yamaha::scp::Family::from_product(&product).or_else(|| songbook_yamaha::scp::Family::from_platform(dev.platform)).ok_or("unknown desk family")?;
            songbook_yamaha::scp::push(&mut c, family, &show, &songbook_yamaha::scp::PushWhat { names: opts.names, mutes: opts.mutes, levels: opts.levels, sends: opts.sends, preamps: opts.preamps }).map_err(err)?
        } else {
            return Err(format!("no live driver for {} yet", dev.platform.label()));
        };
        Ok(serde_json::json!({"ok": true, "log": log}))
    })
    .await
    .map_err(err)?
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SceneRequest {
    /// `recall` or `store`.
    action: String,
    /// Scene number as the desk counts it.
    number: u32,
    /// Yamaha scene list: `scene_a`, `scene_b` or `MIXER:Lib/Scene`.
    #[serde(default)]
    list: Option<String>,
}

#[tauri::command]
async fn device_scene(dev: DeviceRef, req: SceneRequest) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        if dev.platform.is_ah() {
            if req.action != "recall" {
                return Err("Allen & Heath desks store scenes from their own screen; the MIDI protocol only recalls".into());
            }
            let mut d = songbook_ah::live::Device::connect(&dev.host, dev.platform, &ah_opts(&dev)).map_err(err)?;
            d.recall_scene(req.number).map_err(err)?;
            Ok(serde_json::json!({"ok": true}))
        } else if dev.platform.is_yamaha() {
            let mut c = songbook_yamaha::scp::Client::connect(&dev.host).map_err(err)?;
            let product = c.devinfo("productname").unwrap_or_default();
            let family = songbook_yamaha::scp::Family::from_product(&product).or_else(|| songbook_yamaha::scp::Family::from_platform(dev.platform)).ok_or("unknown desk family")?;
            let list = req.list.clone().unwrap_or_else(|| family.scene_lists()[0].to_string());
            match req.action.as_str() {
                "store" => c.store_scene(family, &list, req.number).map_err(err)?,
                _ => c.recall_scene(family, &list, req.number).map_err(err)?,
            }
            Ok(serde_json::json!({"ok": true, "current": c.current_scene(family)}))
        } else {
            Err(format!("no live driver for {} yet", dev.platform.label()))
        }
    })
    .await
    .map_err(err)?
}

// ---------------------------------------------------------------- conversion

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertResult {
    show: Show,
    notes: Vec<songbook_model::Note>,
    carried: usize,
    adapted: usize,
    dropped: usize,
    saved: bool,
}

#[tauri::command]
fn convert_show(state: State<AppState>, id: String, target: Platform, model: String, save: bool) -> CmdResult<ConvertResult> {
    let lib = library(&state)?;
    let show = lib.load(&id).map_err(err)?;
    let c = songbook_convert::convert(&show, target, &model);
    let mut out = c.show;
    if save {
        lib.save(&mut out, &format!("Converted from {} ({})", show.meta.name, show.system.model), author(&state).as_deref()).map_err(err)?;
    }
    Ok(ConvertResult { show: out, notes: c.notes, carried: c.carried, adapted: c.adapted, dropped: c.dropped, saved: save })
}

#[tauri::command]
fn capabilities(platform: Platform, model: String) -> CmdResult<songbook_convert::Capabilities> {
    Ok(songbook_convert::capabilities(platform, &model))
}

// ---------------------------------------------------------------- companion

#[tauri::command]
fn companion_export(state: State<AppState>, id: String, opts: songbook_companion::ExportOptions, path: String) -> CmdResult<Value> {
    let show = library(&state)?.load(&id).map_err(err)?;
    let doc = songbook_companion::export(&show, &opts)?;
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(err)?).map_err(err)?;
    Ok(serde_json::json!({"path": path, "type": doc["type"], "pages": doc.get("pages").map(|p| p.as_object().map(|o| o.len()).unwrap_or(1)).unwrap_or(1)}))
}

#[tauri::command]
fn companion_import(state: State<AppState>, id: String, path: String) -> CmdResult<songbook_companion::ImportReport> {
    let show = library(&state)?.load(&id).map_err(err)?;
    let bytes = std::fs::read(&path).map_err(err)?;
    songbook_companion::import(&bytes, &show)
}

// ---------------------------------------------------------------- sync

#[tauri::command]
async fn sync_run(app: AppHandle, direction: String) -> CmdResult<songbook_library::sync::SyncReport> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().map_err(err)?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use songbook_library::sync::*;
        let dir = match direction.as_str() {
            "push" => Direction::Push,
            "pull" => Direction::Pull,
            _ => Direction::Both,
        };
        let s = &settings.sync;
        let mut provider: Box<dyn SyncProvider> = match s.provider.as_str() {
            "folder" => Box::new(FolderProvider { root: PathBuf::from(&s.root) }),
            "dropbox" => Box::new(DropboxProvider { token: s.tokens.as_ref().map(|t| t.access_token.clone()).ok_or("not connected to Dropbox")?, root: if s.root.is_empty() { "/Songbook".into() } else { s.root.clone() } }),
            "google" => Box::new(GoogleDriveProvider::new(s.tokens.as_ref().map(|t| t.access_token.clone()).ok_or("not connected to Google Drive")?, if s.root.is_empty() { "Songbook".into() } else { s.root.clone() })),
            "onedrive" => Box::new(OneDriveProvider { token: s.tokens.as_ref().map(|t| t.access_token.clone()).ok_or("not connected to OneDrive")?, root: if s.root.is_empty() { "Songbook".into() } else { s.root.clone() } }),
            other => return Err(format!("unknown sync provider {other:?}")),
        };
        sync(Path::new(&settings.library_path), provider.as_mut(), dir).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn oauth_begin(state: State<AppState>, provider: String, client_id: String, client_secret: Option<String>) -> CmdResult<String> {
    let config = songbook_library::sync::oauth_config(&provider, &client_id, client_secret.as_deref()).ok_or("unknown provider")?;
    let pending = songbook_library::oauth::begin(config).map_err(err)?;
    let url = pending.url.clone();
    *state.pending_auth.lock().map_err(err)? = Some(pending);
    Ok(url)
}

#[tauri::command]
async fn oauth_finish(app: AppHandle) -> CmdResult<songbook_library::oauth::Tokens> {
    let state = app.state::<AppState>();
    let pending = state.pending_auth.lock().map_err(err)?.take().ok_or("no sign-in in progress")?;
    let tokens = tauri::async_runtime::spawn_blocking(move || pending.wait(std::time::Duration::from_secs(300)).map_err(err)).await.map_err(err)??;
    let mut settings = state.settings.lock().map_err(err)?;
    settings.sync.tokens = Some(tokens.clone());
    settings.save(&state.config_dir).map_err(err)?;
    Ok(tokens)
}

#[tauri::command]
fn oauth_refresh(state: State<AppState>) -> CmdResult<songbook_library::oauth::Tokens> {
    let mut settings = state.settings.lock().map_err(err)?;
    let s = settings.sync.clone();
    let config = songbook_library::sync::oauth_config(&s.provider, &s.client_id, s.client_secret.as_deref()).ok_or("unknown provider")?;
    let rt = s.tokens.and_then(|t| t.refresh_token).ok_or("no refresh token")?;
    let tokens = songbook_library::oauth::refresh(&config, &rt).map_err(err)?;
    settings.sync.tokens = Some(tokens.clone());
    settings.save(&state.config_dir).map_err(err)?;
    Ok(tokens)
}

// ---------------------------------------------------------------- run

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&config_dir).ok();
            let default_library = app.path().document_dir().map(|d| d.join("Songbook")).unwrap_or_else(|_| config_dir.join("library"));
            let settings = Settings::load(&config_dir, &default_library);
            app.manage(AppState { settings: Mutex::new(settings), config_dir, pending_auth: Mutex::new(None) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_set,
            app_info,
            library_list,
            show_load,
            show_save,
            show_new,
            show_history,
            show_snapshot,
            show_diff,
            show_restore,
            show_delete,
            show_duplicate,
            show_validate,
            import_path,
            export_show_json,
            vendor_export,
            vendor_write,
            write_file,
            write_text,
            read_text,
            device_probe,
            device_pull,
            device_push,
            device_scene,
            convert_show,
            capabilities,
            companion_export,
            companion_import,
            sync_run,
            oauth_begin,
            oauth_finish,
            oauth_refresh,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Songbook");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lib() -> (PathBuf, Library) {
        let root = std::env::temp_dir().join(format!("songbook-app-{}", uuid::Uuid::new_v4()));
        let lib = Library::open(&root).unwrap();
        (root, lib)
    }

    #[test]
    fn imports_every_shape_the_library_accepts() {
        let (root, lib) = temp_lib();
        // A synthetic DM3 scene.
        let dm3 = root.join("opening.dm3s");
        std::fs::write(&dm3, songbook_yamaha::scene::synthetic_dm3("Opening")).unwrap();
        let r = import_any(&lib, &dm3, None, Some("test")).unwrap();
        assert_eq!(r.kind, "mbdf-scene");
        assert_eq!(r.summary.channels, 2);
        assert_eq!(r.show.vendor.len(), 1);
        // A synthetic CLF.
        let clf = root.join("gig.CLF");
        std::fs::write(&clf, songbook_yamaha::clf::synthetic("QL [OSX, 5.8.1.27]", &[(1, "Kick")], &[])).unwrap();
        let r = import_any(&lib, &clf, None, None).unwrap();
        assert_eq!(r.kind, "clf");
        assert_eq!(r.summary.channels, 64);
        // A Songbook JSON round-trips as a new show.
        let out = root.join("copy.json");
        std::fs::write(&out, serde_json::to_string(&r.show).unwrap()).unwrap();
        let again = import_any(&lib, &out, None, None).unwrap();
        assert_eq!(again.kind, "songbook");
        assert_ne!(again.show.id, r.show.id);
        assert_eq!(lib.list().unwrap().len(), 3);
        // Something else is refused with the list of what works.
        let junk = root.join("notes.txt");
        std::fs::write(&junk, b"hello").unwrap();
        let e = import_any(&lib, &junk, None, None).unwrap_err();
        assert!(e.contains("not a show file"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

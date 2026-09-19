//! Show files → `Show`.
//!
//! - **SQ**: a folder holding `NVDATA.DAT` and `SCENEnnn.DAT` (a USB show is
//!   `AHSQ/Shows/<name>/`), the `NVDATA.DAT` itself (its siblings are read
//!   too), or a zip of the folder.
//! - **dLive / Avantis**: the show `.tar.gz` Director or the console exports.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use songbook_model::{build, ids, BusKind, Channel, ChannelKind, Direction, NoteLevel, Platform, Show, SocketKind, Unit, UnitRole};

use crate::{dlive, sq};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Archive(#[from] dlive::ArchiveError),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The files of an SQ show, keyed by upper-case file name.
pub type SqFiles = BTreeMap<String, Vec<u8>>;

/// What was imported, and the bytes to keep as the vendor blob.
pub struct Imported {
    pub show: Show,
    /// `sq-show` or `ah-show`.
    pub kind: &'static str,
    /// The vendor file name and bytes: the archive as given, or a zip built
    /// from the SQ folder.
    pub vendor: Option<(String, Vec<u8>)>,
}

/// Import whatever is at `path`: an SQ folder or `.DAT`, a dLive/Avantis
/// `.tar.gz`, or a zip of an SQ show. `platform` says which A&H desk when the
/// file cannot (a show archive never does); `None` guesses Avantis.
pub fn import_path(path: &Path, platform: Option<Platform>) -> Result<Imported> {
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
        let files = read_sq_dir(path)?;
        let show_name = name.clone();
        return import_sq_files(&show_name, files, platform);
    }
    let bytes = std::fs::read(path)?;
    let lower = name.to_lowercase();
    if lower.ends_with(".dat") {
        // The one file, plus its siblings.
        let dir = path.parent().unwrap_or(Path::new("."));
        let files = read_sq_dir(dir)?;
        let show_name = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or(name.clone());
        return import_sq_files(&show_name, files, platform);
    }
    import_bytes(&name, &bytes, platform)
}

/// Import from bytes: a `.tar.gz` show archive, a zip of an SQ show folder,
/// or a lone `NVDATA.DAT`.
pub fn import_bytes(name: &str, bytes: &[u8], platform: Option<Platform>) -> Result<Imported> {
    let lower = name.to_lowercase();
    if bytes.starts_with(&[0x1F, 0x8B]) || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return import_ah_archive(name, bytes, platform);
    }
    if bytes.starts_with(b"PK\x03\x04") || lower.ends_with(".zip") {
        let mut files = SqFiles::new();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        for i in 0..z.len() {
            let mut f = z.by_index(i)?;
            if !f.is_file() {
                continue;
            }
            let fname = Path::new(f.name()).file_name().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
            if fname.ends_with(".DAT") {
                let mut data = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut data)?;
                files.insert(fname, data);
            }
        }
        let stem = name.trim_end_matches(".zip").trim_end_matches(".ZIP");
        return import_sq_files(stem, files, platform);
    }
    if sq::image_kind(bytes) != sq::ImageKind::Unknown {
        let mut files = SqFiles::new();
        files.insert(name.to_uppercase(), bytes.to_vec());
        return import_sq_files(name.trim_end_matches(".DAT").trim_end_matches(".dat"), files, platform);
    }
    Err(Error::Other(format!("{name}: not an Allen & Heath show Songbook knows (an SQ show folder or NVDATA.DAT, or a dLive / Avantis show .tar.gz)")))
}

fn read_sq_dir(dir: &Path) -> Result<SqFiles> {
    let mut files = SqFiles::new();
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        let fname = p.file_name().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
        if p.is_file() && fname.ends_with(".DAT") {
            files.insert(fname, std::fs::read(&p)?);
        }
    }
    Ok(files)
}

// ---------------------------------------------------------------- SQ

pub fn import_sq_files(name: &str, files: SqFiles, platform: Option<Platform>) -> Result<Imported> {
    let Some((nv_name, nv)) = files.iter().find(|(n, d)| n.as_str() == "NVDATA.DAT" && sq::image_kind(d) == sq::ImageKind::Nvdata).or_else(|| files.iter().find(|(_, d)| sq::image_kind(d) == sq::ImageKind::Nvdata)) else {
        return Err(Error::Other("no NVDATA.DAT image among the files — an SQ show needs one".into()));
    };
    let _ = nv_name;
    let platform = match platform {
        Some(p) if p == Platform::AhSq => p,
        _ => Platform::AhSq,
    };
    let mut show = sq_skeleton(name, platform, "SQ");
    let patch = sq::nvdata_patch(nv);
    if patch.is_empty() {
        show.note(NoteLevel::Info, "channels", "no input channel records were found in NVDATA.DAT, so the input patch is unknown");
    }
    let max_socket = patch.iter().filter_map(|p| p.socket).max().unwrap_or(0).max(1);
    ensure_input_sockets(&mut show, max_socket);
    for p in &patch {
        if let Some(ch) = show.channels.iter_mut().find(|c| c.kind == ChannelKind::Input && c.number == p.channel) {
            ch.source = p.socket.map(sq::socket_id);
        }
    }
    show.note(
        NoteLevel::Info,
        "channels",
        format!(
            "input patch read for Ip1–Ip{} from NVDATA.DAT; the patch byte is a socket number whose class (Local / SLink / USB / I/O port) is not encoded where it was found, and only a Local patch has been observed, so the sockets are labelled as input sockets rather than Local",
            patch.len()
        ),
    );
    show.note(NoteLevel::Info, "channels", "channel names, colours, preamps, processing and the mix are not decoded from SQ show files yet; pull the show from the desk over MIDI/TCP to read mutes, levels, pans and assignments");

    // Scenes.
    let mut scene_files: Vec<(u32, &Vec<u8>)> = files
        .iter()
        .filter_map(|(n, d)| {
            let num: u32 = n.strip_prefix("SCENE")?.strip_suffix(".DAT")?.parse().ok()?;
            (sq::image_kind(d) == sq::ImageKind::Scene).then_some((num, d))
        })
        .collect();
    scene_files.sort_by_key(|(n, _)| *n);
    for (n, d) in &scene_files {
        let label = sq::scene_name(d);
        let label = if label.is_empty() { format!("Scene {n}") } else { label };
        let mut sc = build::scene(*n, &label);
        sc.notes = "contents not decoded: an SQ scene image holds the whole mix, but only its name is read".into();
        show.scenes.push(sc);
    }

    // The vendor blob: one zip of every image, so the show can go back to a
    // USB stick as the folder it came from.
    let mut zbuf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut zbuf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, d) in &files {
            zw.start_file(n.as_str(), opts)?;
            std::io::Write::write_all(&mut zw, d)?;
        }
        zw.finish()?;
    }
    show.meta.source = Some(songbook_model::SourceInfo { kind: "file".into(), origin: name.to_string(), at: songbook_model::now(), firmware: None });
    Ok(Imported { show, kind: "sq-show", vendor: Some((format!("{name}.sq-show.zip"), zbuf.into_inner())) })
}

/// The fixed shape of every SQ: 48 inputs, 3 stereo inputs and USB, 8 FX
/// returns, LR, 12 mixes, 4 FX sends, 3 matrices, 8 DCAs, 8 mute groups.
pub fn sq_skeleton(name: &str, platform: Platform, model: &str) -> Show {
    let mut show = Show::new(name, platform);
    let m = sq::model(model);
    show.system.model = m.name.to_string();
    show.system.units.push(build::unit("local", &format!("{} local sockets", m.name), m.name, UnitRole::Console));
    show.sockets.extend(build::sockets("local", Direction::In, m.local_inputs, SocketKind::Mic, "Local"));
    show.sockets.extend(build::sockets("local", Direction::Out, m.local_outputs, SocketKind::Line, "Out"));
    show.system.units.push(build::unit("slink", "SLink", "SLink", UnitRole::Network));
    show.system.units.push(build::unit("usb", "USB-B audio", "USB", UnitRole::Internal));
    for n in 1..=sq::INPUT_CHANNELS as u32 {
        show.channels.push(Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Ip{n}")));
    }
    for n in 1..=3u32 {
        let mut c = Channel::new(ids::stereo_input(n), n, ChannelKind::StereoInput, &format!("ST{n}"));
        c.stereo = true;
        show.channels.push(c);
    }
    let mut usb = Channel::new(ids::stereo_input(4), 4, ChannelKind::StereoInput, "USB");
    usb.stereo = true;
    show.channels.push(usb);
    for n in 1..=sq::FX_RETURNS {
        let mut c = Channel::new(ids::fx_return(n), n, ChannelKind::FxReturn, &format!("FX{n}Rtn"));
        c.stereo = true;
        show.channels.push(c);
    }
    show.buses.push(songbook_model::Bus::new(BusKind::Main, 1, "LR", true));
    for n in 1..=sq::AUXES {
        show.buses.push(songbook_model::Bus::new(BusKind::Aux, n, &format!("Aux{n}"), false));
    }
    for n in 1..=sq::FX_SENDS {
        show.buses.push(songbook_model::Bus::new(BusKind::FxSend, n, &format!("FX{n}Snd"), false));
    }
    for n in 1..=sq::MATRICES {
        show.buses.push(songbook_model::Bus::new(BusKind::Matrix, n, &format!("Mtx{n}"), true));
    }
    for n in 1..=sq::DCAS {
        show.dcas.push(build::dca(n, &format!("DCA{n}")));
    }
    for n in 1..=sq::MUTE_GROUPS {
        show.mute_groups.push(build::mute_group(n, &format!("MGrp{n}")));
    }
    show
}

fn ensure_input_sockets(show: &mut Show, up_to: u32) {
    if show.unit("unit:input").is_none() {
        show.system.units.push(Unit {
            id: ids::unit("input"),
            label: "Input sockets (class not decoded)".into(),
            model: String::new(),
            role: UnitRole::Console,
            address: None,
            extra: Default::default(),
        });
    }
    for n in 1..=up_to {
        let id = sq::socket_id(n);
        if show.socket(&id).is_none() {
            show.sockets.push(build::socket("input", Direction::In, n, SocketKind::Mic, &format!("Input socket {n}")));
        }
    }
}

// ---------------------------------------------------------------- dLive / Avantis

/// The desk names in the show file's name blocks, and the model entities they become.
fn class_kind(class: &str) -> Option<(BusKind, bool)> {
    Some(match class {
        "Mono Group" => (BusKind::Group, false),
        "Stereo Group" => (BusKind::Group, true),
        "Mono Aux" => (BusKind::Aux, false),
        "Stereo Aux" => (BusKind::Aux, true),
        "Mono Matrix" => (BusKind::Matrix, false),
        "Stereo Matrix" => (BusKind::Matrix, true),
        "Mono FX Send" => (BusKind::FxSend, false),
        "Stereo FX Send" => (BusKind::FxSend, true),
        "Main" => (BusKind::Main, true),
        _ => return None,
    })
}

pub fn import_ah_archive(name: &str, bytes: &[u8], platform: Option<Platform>) -> Result<Imported> {
    let entries = dlive::open(bytes)?;
    if !entries.iter().any(|e| e.path.contains("Show/")) {
        return Err(dlive::ArchiveError::NotAShow.into());
    }
    let platform = match platform {
        Some(p @ (Platform::AhDlive | Platform::AhAvantis)) => p,
        _ => Platform::AhAvantis,
    };
    let family = if platform == Platform::AhDlive { dlive::Family::Dlive } else { dlive::Family::Avantis };
    let show_name = name.trim_end_matches(".gz").trim_end_matches(".tar").trim_end_matches(".tgz");
    let mut show = Show::new(show_name, platform);
    show.system.model = platform.label().trim_start_matches("Allen & Heath ").to_string();
    show.note(NoteLevel::Info, "system", "the show archive does not say which console model it came from; Avantis and dLive shows are structurally the same, so the platform is the one chosen at import");

    let version = entries.iter().find(|e| e.path.ends_with("Version.dat")).map(|e| String::from_utf8_lossy(&e.data).trim().to_string());
    if let Some(v) = version {
        show.system.extra.insert("showVersion".into(), v.into());
    }

    // Units the mapper can name.
    show.system.units.push(build::unit("local", "Local sockets", "", UnitRole::Surface));
    show.system.units.push(build::unit("slink", "SLink", "", UnitRole::Network));

    let live = entries.iter().find(|e| e.path.contains("StageBoxScene65535")).ok_or(dlive::ArchiveError::NoLiveScene)?;
    let inner = dlive::open(&live.data)?;
    let blob = inner.first().map(|e| e.data.clone()).unwrap_or_default();

    let blocks = dlive::name_blocks(&blob);
    let inputs_block = blocks.iter().find(|b| b.class == "Input");
    let input_count = inputs_block.map(|b| b.names.len() as u32).unwrap_or_else(|| dlive::count_objects(&blob, "Input Channel")).max(1);
    for n in 1..=input_count {
        let mut ch = Channel::new(ids::channel(n), n, ChannelKind::Input, &format!("Ip {n}"));
        if let Some(b) = inputs_block {
            if let Some(nm) = b.names.get(n as usize - 1) {
                if !nm.is_empty() {
                    ch.label = nm.clone();
                }
            }
            if let Some(&c) = b.colors.get(n as usize - 1) {
                ch.color = dlive::color_name(c).map(str::to_string);
            }
        }
        show.channels.push(ch);
    }
    for b in &blocks {
        match b.class.as_str() {
            "Input" => {}
            "FX Return" => {
                for (i, nm) in b.names.iter().enumerate() {
                    let n = i as u32 + 1;
                    let label = if nm.is_empty() { format!("FX Rtn {n}") } else { nm.clone() };
                    let mut ch = Channel::new(ids::fx_return(n), n, ChannelKind::FxReturn, &label);
                    ch.stereo = true;
                    ch.color = b.colors.get(i).and_then(|&c| dlive::color_name(c)).map(str::to_string);
                    show.channels.push(ch);
                }
            }
            "DCA" => {
                for (i, nm) in b.names.iter().enumerate() {
                    let n = i as u32 + 1;
                    let label = if nm.is_empty() { format!("DCA {n}") } else { nm.clone() };
                    let mut d = build::dca(n, &label);
                    d.color = b.colors.get(i).and_then(|&c| dlive::color_name(c)).map(str::to_string);
                    show.dcas.push(d);
                }
            }
            other => {
                if let Some((kind, stereo)) = class_kind(other) {
                    for (i, nm) in b.names.iter().enumerate() {
                        let n = i as u32 + 1;
                        // Mono and stereo buses of a kind share one model numbering; a
                        // stereo bus keeps its desk numbering in `extra`.
                        let number = if stereo && kind != BusKind::Main { n + stereo_offset(&show, kind) } else { n };
                        let label = if nm.is_empty() { format!("{other} {n}") } else { nm.clone() };
                        let mut bus = songbook_model::Bus::new(kind, number, &label, stereo);
                        bus.extra.insert("ahClass".into(), other.into());
                        bus.extra.insert("ahNumber".into(), n.into());
                        bus.color = b.colors.get(i).and_then(|&c| dlive::color_name(c)).map(str::to_string);
                        show.buses.push(bus);
                    }
                }
            }
        }
    }
    if show.buses.is_empty() {
        show.note(NoteLevel::Info, "buses", "no mix name blocks were found in the live scene; the bus inventory is unknown");
    }
    for n in 1..=family.limits().mute_groups {
        show.mute_groups.push(build::mute_group(n, &format!("Mute Group {n}")));
    }

    // Input patch.
    match dlive::channel_mapper(&blob, input_count as usize) {
        Some(map) => {
            let mut unknown: BTreeMap<u8, usize> = BTreeMap::new();
            for (i, (t, idx)) in map.iter().enumerate() {
                let unit = match t {
                    0x00 => "local",
                    0x03 => "slink",
                    _ => {
                        *unknown.entry(*t).or_default() += 1;
                        continue;
                    }
                };
                let sid = ids::socket(unit, Direction::In, *idx as u32 + 1);
                if show.socket(&sid).is_none() {
                    show.sockets.push(build::socket(unit, Direction::In, *idx as u32 + 1, if unit == "local" { SocketKind::Mic } else { SocketKind::SLink }, &format!("{} {}", if unit == "local" { "Local" } else { "SLink" }, idx + 1)));
                }
                show.channels[i].source = Some(sid);
            }
            if !unknown.is_empty() {
                let detail = unknown.iter().map(|(t, n)| format!("0x{t:02x} on {n}")).collect::<Vec<_>>().join(", ");
                show.note(NoteLevel::Info, "channels", format!("some channels carry a source type outside the two confirmed codes (0x00 local, 0x03 SLink): {detail}; I/O Port, USB and unpatched have not been identified, so those sources are left blank"));
            }
        }
        None => show.note(NoteLevel::Info, "channels", "no Channel Mapper block in the live scene; the input patch is unknown"),
    }
    show.sockets.sort_by_key(|a| (a.unit_id.clone(), a.index));

    // Numbered scenes: name and description only.
    let mut scenes: Vec<(u32, String, String)> = vec![];
    for e in &entries {
        let Some(file) = e.path.rsplit('/').next() else { continue };
        let Some(num) = file.strip_prefix("StageBoxScene").and_then(|s| s.strip_suffix(".tar.gz")).and_then(|s| s.parse::<u32>().ok()) else { continue };
        if num == 65535 {
            continue;
        }
        let Ok(inner) = dlive::open(&e.data) else { continue };
        let Some(first) = inner.first() else { continue };
        if let Some((title, desc)) = dlive::scene_title(&first.data) {
            scenes.push((num, title, desc));
        }
    }
    scenes.sort_by_key(|s| s.0);
    for (n, title, desc) in scenes {
        let mut sc = build::scene(n, &title);
        sc.notes = desc;
        show.scenes.push(sc);
    }
    show.note(NoteLevel::Info, "scenes", "scene contents are not decoded: a numbered scene's name and description are read, the mix it recalls is not");
    show.note(NoteLevel::Info, "channels", "faders, mutes, sends, preamps and processing are present in the scene blobs but not decoded; pull from the desk over MIDI/TCP for the live values");
    show.meta.source = Some(songbook_model::SourceInfo { kind: "file".into(), origin: name.to_string(), at: songbook_model::now(), firmware: None });
    Ok(Imported { show, kind: "ah-show", vendor: Some((name.to_string(), bytes.to_vec())) })
}

/// Stereo buses continue the numbering after the mono ones of the same kind
/// already added, so a show's `bus:aux:N` ids are unique.
fn stereo_offset(show: &Show, kind: BusKind) -> u32 {
    show.buses.iter().filter(|b| b.kind == kind && !b.stereo).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dlive::build as ab;

    #[test]
    fn imports_a_synthetic_avantis_show() {
        let inputs = ab::name_block("Input", &["Kick", "Snare", "", "Vox"], &[1, 2, 0, 7]);
        let auxes = ab::name_block("Mono Aux", &["Wedge 1", "Wedge 2"], &[6, 6]);
        let st_aux = ab::name_block("Stereo Aux", &["IEM 1"], &[4]);
        let dcas = ab::name_block("DCA", &["Band", "Vox"], &[1, 1]);
        let mains = ab::name_block("Main", &["LR"], &[7]);
        let live = ab::scene_blob(None, 4, &[(0x03, 0), (0x03, 1), (0x00, 4), (0x25, 0)], &[inputs, auxes, st_aux, dcas, mains]);
        let s1 = ab::scene_blob(Some(("Opening", "Band walks on")), 4, &[], &[]);
        let s2 = ab::scene_blob(Some(("Interval", "")), 4, &[], &[]);
        let archive = ab::targz(&[
            ("Show/Version.dat", b"Bridge-5"),
            ("Show/Scenes/StageBoxScene001.tar.gz", &ab::targz(&[("StageBoxScene001.dat", &s1)])),
            ("Show/Scenes/StageBoxScene002.tar.gz", &ab::targz(&[("StageBoxScene002.dat", &s2)])),
            ("Show/Scenes/StageBoxScene65535.tar.gz", &ab::targz(&[("StageBoxScene65535.dat", &live)])),
        ]);
        let r = import_bytes("FOH.tar.gz", &archive, Some(Platform::AhDlive)).unwrap();
        let show = r.show;
        assert_eq!(r.kind, "ah-show");
        assert_eq!(show.platform, Platform::AhDlive);
        assert_eq!(show.meta.name, "FOH");
        assert_eq!(show.channels.iter().filter(|c| c.kind == ChannelKind::Input).count(), 4);
        assert_eq!(show.channels[0].label, "Kick");
        assert_eq!(show.channels[0].color.as_deref(), Some("red"));
        assert_eq!(show.channels[2].label, "Ip 3");
        assert_eq!(show.channels[0].source.as_deref(), Some("skt:slink:in:1"));
        assert_eq!(show.channels[2].source.as_deref(), Some("skt:local:in:5"));
        assert_eq!(show.channels[3].source, None);
        assert_eq!(show.buses_of(BusKind::Aux).count(), 3);
        let st = show.buses_of(BusKind::Aux).find(|b| b.stereo).unwrap();
        assert_eq!(st.number, 3);
        assert_eq!(st.label, "IEM 1");
        assert_eq!(show.dcas.len(), 2);
        assert_eq!(show.scenes.len(), 2);
        assert_eq!(show.scenes[0].label, "Opening");
        assert_eq!(show.scenes[0].notes, "Band walks on");
        assert_eq!(show.system.extra["showVersion"], "Bridge-5");
        assert!(show.validate().is_empty(), "{:?}", show.validate());
        assert!(show.notes.iter().any(|n| n.message.contains("0x25")));
    }

    #[test]
    fn imports_a_synthetic_sq_show_folder_as_a_zip() {
        fn image(kind: u8) -> Vec<u8> {
            let mut d = vec![0u8; sq::IMAGE_LEN];
            d[0] = kind;
            d[2] = 0xFE;
            for b in &mut d[3..12] {
                *b = 0xFF;
            }
            d[0x0C..0x10].copy_from_slice(&[0x01, 0x06, 0x00, 0x01]);
            d
        }
        let mut nv = image(0xB5);
        for i in 0..40usize {
            let at = 0x38C + i * 336;
            nv[at - 3..at].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
            nv[at] = if i == 2 { 9 } else { i as u8 };
            nv[at + 2] = u8::from(i != 39);
            nv[at + 3] = 0xFE;
        }
        let mut sc = image(0xA1);
        sc[0x14..0x1B].copy_from_slice(b"Soundch");
        let mut files = SqFiles::new();
        files.insert("NVDATA.DAT".into(), nv);
        files.insert("SCENE003.DAT".into(), sc);
        let r = import_sq_files("Gig", files, None).unwrap();
        assert_eq!(r.kind, "sq-show");
        let show = r.show;
        assert_eq!(show.platform, Platform::AhSq);
        assert_eq!(show.channels.iter().filter(|c| c.kind == ChannelKind::Input).count(), 48);
        assert_eq!(show.channels[2].source.as_deref(), Some("skt:input:in:10"));
        assert_eq!(show.channels[39].source, None);
        assert_eq!(show.channels[40].source, None);
        assert_eq!(show.scenes.len(), 1);
        assert_eq!(show.scenes[0].number, Some(3));
        assert_eq!(show.scenes[0].label, "Soundch");
        assert_eq!(show.buses_of(BusKind::Aux).count(), 12);
        assert!(show.validate().is_empty(), "{:?}", show.validate());
        let (vname, vbytes) = r.vendor.unwrap();
        assert!(vname.ends_with(".sq-show.zip"));
        // And the zip imports again.
        let again = import_bytes(&vname, &vbytes, None).unwrap();
        assert_eq!(again.show.channels[2].source.as_deref(), Some("skt:input:in:10"));
    }

    /// Against a real Director export, when one is on this machine.
    #[test]
    fn real_avantis_show_if_present() {
        let Ok(path) = std::env::var("SONGBOOK_AH_SHOW") else { return };
        let r = import_path(Path::new(&path), Some(Platform::AhAvantis)).expect("import");
        let show = r.show;
        assert!(show.channels.len() >= 64, "{} channels", show.channels.len());
        assert!(show.channels.iter().all(|c| !c.label.is_empty()));
        assert!(show.buses_of(BusKind::Aux).count() > 0);
        assert!(show.validate().is_empty(), "{:?}", show.validate());
        eprintln!(
            "{}: {} channels ({} patched), {} buses, {} DCAs, {} scenes; first names {:?}",
            show.meta.name,
            show.channels.len(),
            show.channels.iter().filter(|c| c.source.is_some()).count(),
            show.buses.len(),
            show.dcas.len(),
            show.scenes.len(),
            show.channels.iter().take(6).map(|c| (c.label.clone(), c.color.clone())).collect::<Vec<_>>()
        );
    }
}

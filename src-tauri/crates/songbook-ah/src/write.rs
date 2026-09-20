//! Writing a show back into the SQ images it came from.
//!
//! An SQ show folder is kept as a zip of its `.DAT` images (see `import`).
//! Writing takes that zip and the edited show, rewrites the input patch in
//! `NVDATA.DAT` and the scene names in every `SCENEnnn.DAT` the show still
//! has, refreshes each image's CRC-32 (`sq::fix_image_checksum`) and returns
//! a new zip of the same file names. Nothing else in the images changes, so
//! whatever the reader does not decode (names, preamps, the mix) survives
//! untouched.
//!
//! A scene image stores the patch it was saved with (MixPad re-stored one
//! on 2026-09-20 and the same records moved), so a recall would undo a patch
//! written only to `NVDATA.DAT`. The writer therefore puts the new patch into
//! every scene whose stored patch matched the old NVDATA patch — the scenes
//! that were following the desk's patch — and leaves a scene with a patch of
//! its own alone, saying so.

use std::io::{Read, Write as _};

use songbook_model::{ChannelKind, Show};

use crate::import::{Error, Result, SqFiles};
use crate::sq;

/// What a write carried and what it could not.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub files: Vec<String>,
    pub patches_written: usize,
    pub scene_names_written: usize,
    /// Scenes that received the new patch because they were following the old one.
    pub scene_patches_written: usize,
    pub skipped: Vec<String>,
}

/// Unpack the kept zip into images by upper-cased file name.
pub fn unzip(bytes: &[u8]) -> Result<SqFiles> {
    let mut files = SqFiles::new();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..z.len() {
        let mut f = z.by_index(i)?;
        if !f.is_file() {
            continue;
        }
        let name = std::path::Path::new(f.name()).file_name().map(|s| s.to_string_lossy().to_uppercase()).unwrap_or_default();
        let mut data = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut data)?;
        files.insert(name, data);
    }
    Ok(files)
}

/// Zip images back up under their names.
pub fn zip_files(files: &SqFiles) -> Result<Vec<u8>> {
    let mut zbuf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut zbuf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, d) in files {
            zw.start_file(n.as_str(), opts)?;
            zw.write_all(d)?;
        }
        zw.finish()?;
    }
    Ok(zbuf.into_inner())
}

/// The image spelling of a channel source, when it is a socket an SQ image
/// can hold.
fn patch_socket(show: &Show, id: &str) -> Option<sq::PatchSocket> {
    let s = show.socket(id)?;
    sq::PatchSocket::from_unit(&s.unit_id, s.index)
}

/// Write the show into copies of its images.
pub fn write_images(files: &SqFiles, show: &Show) -> Result<(SqFiles, WriteReport)> {
    let mut out: SqFiles = files.clone();
    let mut report = WriteReport::default();
    let Some((nv_name, nv)) = out.iter_mut().find(|(_, d)| sq::image_kind(d) == sq::ImageKind::Nvdata) else {
        return Err(Error::Other("the kept show has no NVDATA.DAT image to write the patch into".into()));
    };
    let old_patch = sq::input_patch(nv);
    let mut patch = vec![];
    for c in show.channels.iter().filter(|c| c.kind == ChannelKind::Input) {
        let socket = match &c.source {
            None => None,
            Some(id) => match patch_socket(show, id) {
                Some(sk) => Some(sk),
                None => {
                    report.skipped.push(format!("Ip{}: source {id} is not a Local / SLink / USB / I/O Port socket an SQ image can hold; left as it was", c.number));
                    continue;
                }
            },
        };
        patch.push(sq::InputPatchEntry { channel: c.number, socket });
    }
    report.patches_written = sq::write_patch(nv, &patch).map_err(Error::Other)?;
    report.files.push(nv_name.clone());
    let nv_name = nv_name.clone();
    // Scenes: the name, and the patch where the scene was following the desk's.
    for sc in &show.scenes {
        let Some(n) = sc.number else { continue };
        let name = sq::scene_file_name(n);
        match out.get_mut(&name) {
            Some(img) if sq::image_kind(img) == sq::ImageKind::Scene => {
                if sc.label.chars().count() > sq::SCENE_NAME_LEN {
                    report.skipped.push(format!("scene {n}: name \"{}\" cut to {} characters", sc.label, sq::SCENE_NAME_LEN));
                }
                sq::write_scene_name(img, &sc.label).map_err(Error::Other)?;
                report.scene_names_written += 1;
                let stored = sq::input_patch(img);
                if patch_agrees(&stored, &old_patch) {
                    sq::write_patch(img, &patch).map_err(Error::Other)?;
                    report.scene_patches_written += 1;
                } else {
                    report.skipped.push(format!("scene {n}: keeps the input patch it was stored with (it differed from the show's patch before this write, so it was not following it)"));
                }
                report.files.push(name);
            }
            _ => report.skipped.push(format!("scene {n}: no {name} in the kept show, so its name was not written (Songbook cannot create a scene image)")),
        }
    }
    let mut images: Vec<String> = out.keys().cloned().collect();
    images.retain(|k| !report.files.contains(k) && *k != nv_name);
    if !images.is_empty() {
        report.files.extend(images.into_iter().map(|k| format!("{k} (unchanged)")));
    }
    Ok((out, report))
}

/// Whether two patch tables agree on every channel they both list.
fn patch_agrees(a: &[sq::InputPatchEntry], b: &[sq::InputPatchEntry]) -> bool {
    a.iter().all(|x| b.iter().find(|y| y.channel == x.channel).is_none_or(|y| y.socket == x.socket))
}

/// Write the show into the kept zip and return the new zip.
pub fn write_zip(zip_bytes: &[u8], show: &Show) -> Result<(Vec<u8>, WriteReport)> {
    let files = unzip(zip_bytes)?;
    let (out, report) = write_images(&files, show)?;
    Ok((zip_files(&out)?, report))
}

/// Write the show into a folder of images on disk (a USB stick's
/// `AHSQ/Shows/<name>`), from the kept zip.
pub fn write_folder(zip_bytes: &[u8], show: &Show, dir: &std::path::Path) -> Result<WriteReport> {
    let files = unzip(zip_bytes)?;
    let (out, report) = write_images(&files, show)?;
    std::fs::create_dir_all(dir)?;
    for (n, d) in &out {
        std::fs::write(dir.join(n), d)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{import_sq_files, tests_support};

    #[test]
    fn round_trips_patch_and_scene_names_through_the_zip() {
        let files = tests_support::synthetic_sq_show();
        let imported = import_sq_files("Band", files.clone(), None).unwrap();
        let (_, zip_bytes) = imported.vendor.unwrap();
        let mut show = imported.show;
        // Move Ip1 to Local 7, unpatch Ip2, put Ip4 on USB 3, rename both scenes.
        show.channels[0].source = Some(sq::socket_id(7));
        show.channels[1].source = None;
        show.channels[3].source = Some("skt:usb:in:3".into());
        show.sockets.push(songbook_model::build::socket("usb", songbook_model::Direction::In, 3, songbook_model::SocketKind::Usb, "USB 3"));
        show.scenes[0].label = "Opening".into();
        show.scenes.iter_mut().find(|s| s.number == Some(4)).unwrap().label = "Encore".into();
        let (out, report) = write_zip(&zip_bytes, &show).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        assert_eq!(report.scene_names_written, 2);
        assert_eq!(report.scene_patches_written, 2, "both scenes were following the desk's patch");
        assert_eq!(report.patches_written, 48);
        let files = unzip(&out).unwrap();
        let back = import_sq_files("Band", files.clone(), None).unwrap().show;
        assert_eq!(back.channels[0].source.as_deref(), Some("skt:local:in:7"));
        assert_eq!(back.channels[1].source, None);
        assert_eq!(back.channels[3].source.as_deref(), Some("skt:usb:in:3"));
        assert_eq!(back.scenes[0].label, "Opening");
        assert_eq!(back.scenes.iter().find(|s| s.number == Some(4)).unwrap().label, "Encore");
        for (name, d) in &files {
            assert!(sq::image_checksum_ok(d), "{name}");
            // The scenes carry the new patch too.
            if name.starts_with("SCENE") {
                let p = sq::input_patch(d);
                assert_eq!(p[0].socket.unwrap().index, 7, "{name}");
                assert_eq!(p[1].socket, None, "{name}");
            }
        }
    }

    #[test]
    fn a_scene_with_its_own_patch_keeps_it() {
        let mut files = tests_support::synthetic_sq_show();
        // Scene 2 (SCENE001.DAT) was stored with Ip1 on Local 20.
        let img = files.get_mut("SCENE001.DAT").unwrap();
        sq::write_patch(img, &[sq::InputPatchEntry { channel: 1, socket: Some(sq::PatchSocket { class: sq::CLASS_LOCAL, index: 20 }) }]).unwrap();
        let imported = import_sq_files("Band", files, None).unwrap();
        let (_, zip_bytes) = imported.vendor.unwrap();
        let mut show = imported.show;
        show.channels[0].source = Some(sq::socket_id(7));
        let (out, report) = write_zip(&zip_bytes, &show).unwrap();
        assert_eq!(report.scene_patches_written, 1);
        assert!(report.skipped.iter().any(|s| s.starts_with("scene 2: keeps")), "{:?}", report.skipped);
        let files = unzip(&out).unwrap();
        assert_eq!(sq::input_patch(&files["SCENE001.DAT"])[0].socket.unwrap().index, 20);
        assert_eq!(sq::input_patch(&files["SCENE003.DAT"])[0].socket.unwrap().index, 7);
    }

    #[test]
    fn reports_a_scene_with_no_image() {
        let files = tests_support::synthetic_sq_show();
        let imported = import_sq_files("Band", files, None).unwrap();
        let (_, zip_bytes) = imported.vendor.unwrap();
        let mut show = imported.show;
        show.scenes.push(songbook_model::build::scene(42, "Ghost"));
        let (_, report) = write_zip(&zip_bytes, &show).unwrap();
        assert!(report.skipped.iter().any(|s| s.contains("scene 42")));
    }
}

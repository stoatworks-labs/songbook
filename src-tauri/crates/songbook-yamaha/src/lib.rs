//! Yamaha driver: CL/QL console files, DM3 / TF / DM7 MBDF scenes, and the
//! SCP remote control protocol to the live desks.

pub mod clf;
pub mod mbdf;
pub mod mms;
pub mod scene;
pub mod scp;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("not a Yamaha MBDF container")]
    NotMbdf,
    #[error("not an MMSXLIT payload")]
    NotMms,
    #[error("truncated: {0}")]
    Truncated(&'static str),
    #[error("unexpected record tag {0:?}")]
    BadRecord(String),
    #[error("bad schema: {0}")]
    BadSchema(&'static str),
    #[error("not a CL/QL console file")]
    NotClf,
}

use std::path::Path;

use songbook_model::{Platform, Show};

/// What a file turned out to be.
pub struct Imported {
    pub show: Show,
    /// `clf` or `mbdf-scene`.
    pub kind: &'static str,
}

pub fn looks_like(bytes: &[u8]) -> bool {
    mbdf::Container::looks_like(bytes) || clf::looks_like(bytes)
}

/// Import a `.CLF` or an MBDF scene / preset.
pub fn import_bytes(name: &str, bytes: &[u8]) -> std::result::Result<Imported, Error> {
    if mbdf::Container::looks_like(bytes) {
        let p = scene::parse(bytes, name)?;
        return Ok(Imported { show: p.show, kind: "mbdf-scene" });
    }
    if clf::looks_like(bytes) {
        return Ok(Imported { show: clf::parse(bytes, name)?, kind: "clf" });
    }
    Err(Error::NotMbdf)
}

pub fn import_path(path: &Path) -> std::result::Result<Imported, String> {
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    import_bytes(&name, &bytes).map_err(|e| format!("{name}: {e}"))
}

/// The desk models Songbook lists per Yamaha platform.
pub fn models(p: Platform) -> &'static [&'static str] {
    match p {
        Platform::YamahaClQl => &["QL1", "QL5", "CL1", "CL3", "CL5"],
        Platform::YamahaTf => &["TF1", "TF3", "TF5", "TF-Rack"],
        Platform::YamahaDm3 => &["DM3", "DM3S"],
        Platform::YamahaDm7 => &["DM7", "DM7 Compact"],
        Platform::YamahaRivage => &["PM3", "PM5", "PM7", "PM10"],
        _ => &[],
    }
}

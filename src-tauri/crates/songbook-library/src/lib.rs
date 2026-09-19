//! The show library on disk.
//!
//! ```text
//! <root>/songbook-library.json            { "schema": "songbook-library/1", … }
//! <root>/shows/<show id>/show.json        the current model
//! <root>/shows/<show id>/history/index.json   [ commit, commit, … ] oldest first
//! <root>/shows/<show id>/history/<hash>.json.gz  one snapshot per distinct content
//! <root>/shows/<show id>/vendor/<sha256>.<ext>   vendor files (NVDATA.DAT, a dLive show .tar.gz, a .CLF, …)
//! <root>/shows/<show id>/assets/…          files the show refers to (a logo for the document)
//! ```
//!
//! Every save is a commit: the model is written, hashed, and if the hash is
//! new a gzip'd snapshot is kept and an entry appended to the index. Content
//! addressing means saving the same thing twice costs nothing and the
//! history can always be diffed pairwise ([`songbook_model::diff`]).
//!
//! The library is plain files on purpose: put the root inside a Dropbox,
//! OneDrive or Google Drive folder and their desktop clients carry it; the
//! [`sync`] module does the same over the drives' APIs when there is no
//! desktop client.

pub mod oauth;
pub mod sync;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use songbook_model::diff::Change;
use songbook_model::summary::Summary;
use songbook_model::{Platform, Show, VendorBlob};

pub const SCHEMA: &str = "songbook-library/1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no show {0} in the library")]
    NoShow(String),
    #[error("no commit {0}")]
    NoCommit(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub at: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// sha256 of the snapshot's canonical JSON.
    pub hash: String,
    pub summary: Summary,
    /// Number of changes against the parent (0 for the first commit).
    #[serde(default)]
    pub changes: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub summary: Summary,
    pub dir: String,
    pub commits: usize,
    #[serde(default)]
    pub vendor_files: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct LibraryFile {
    schema: String,
    created: String,
}

pub struct Library {
    pub root: PathBuf,
}

impl Library {
    /// Open a library, creating the marker file and `shows/` if absent.
    pub fn open(root: &Path) -> Result<Library> {
        std::fs::create_dir_all(root.join("shows"))?;
        let marker = root.join("songbook-library.json");
        if !marker.exists() {
            let f = LibraryFile { schema: SCHEMA.into(), created: songbook_model::now() };
            std::fs::write(&marker, serde_json::to_string_pretty(&f)?)?;
        }
        Ok(Library { root: root.to_path_buf() })
    }

    pub fn show_dir(&self, id: &str) -> PathBuf {
        self.root.join("shows").join(id)
    }

    pub fn list(&self) -> Result<Vec<Entry>> {
        let mut out = vec![];
        let shows = self.root.join("shows");
        if !shows.exists() {
            return Ok(out);
        }
        for e in std::fs::read_dir(&shows)?.flatten() {
            let dir = e.path();
            let file = dir.join("show.json");
            if !file.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let Ok(show) = serde_json::from_str::<Show>(&text) else { continue };
            let commits = self.history(&show.id).map(|h| h.len()).unwrap_or(0);
            let vendor_files = std::fs::read_dir(dir.join("vendor")).map(|rd| rd.count()).unwrap_or(0);
            out.push(Entry { summary: Summary::of(&show), dir: dir.display().to_string(), commits, vendor_files });
        }
        out.sort_by(|a, b| b.summary.modified.cmp(&a.summary.modified));
        Ok(out)
    }

    pub fn load(&self, id: &str) -> Result<Show> {
        let file = self.show_dir(id).join("show.json");
        if !file.is_file() {
            return Err(Error::NoShow(id.into()));
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(file)?)?)
    }

    /// Write the show and record a commit. Returns the commit, or `None`
    /// when nothing changed since the last one.
    pub fn save(&self, show: &mut Show, message: &str, author: Option<&str>) -> Result<Option<Commit>> {
        show.touch();
        let dir = self.show_dir(&show.id);
        std::fs::create_dir_all(dir.join("history"))?;
        std::fs::create_dir_all(dir.join("vendor"))?;
        let json = serde_json::to_string_pretty(show)?;
        std::fs::write(dir.join("show.json"), &json)?;

        let hash = hash_show(show);
        let mut index = self.history(&show.id).unwrap_or_default();
        if let Some(last) = index.last() {
            if last.hash == hash {
                return Ok(None);
            }
        }
        let parent = index.last().map(|c| c.id.clone());
        let changes = match &parent {
            Some(pid) => {
                let prev = self.snapshot(&show.id, pid)?;
                songbook_model::diff::diff(&serde_json::to_value(&prev)?, &serde_json::to_value(&*show)?).len()
            }
            None => 0,
        };
        let snap = dir.join("history").join(format!("{hash}.json.gz"));
        if !snap.exists() {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut enc, json.as_bytes())?;
            std::fs::write(&snap, enc.finish()?)?;
        }
        let at = songbook_model::now();
        let id = short_id(&format!("{}{}{}", parent.clone().unwrap_or_default(), hash, at));
        let commit = Commit {
            id,
            parent,
            at,
            message: if message.trim().is_empty() { "Save".into() } else { message.trim().to_string() },
            author: author.map(str::to_string),
            hash,
            summary: Summary::of(show),
            changes,
        };
        index.push(commit.clone());
        std::fs::write(dir.join("history").join("index.json"), serde_json::to_string_pretty(&index)?)?;
        Ok(Some(commit))
    }

    pub fn history(&self, id: &str) -> Result<Vec<Commit>> {
        let file = self.show_dir(id).join("history").join("index.json");
        if !file.is_file() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(file)?)?)
    }

    /// The show as it was at a commit.
    pub fn snapshot(&self, id: &str, commit_id: &str) -> Result<Show> {
        let index = self.history(id)?;
        let c = index.iter().find(|c| c.id == commit_id).ok_or_else(|| Error::NoCommit(commit_id.into()))?;
        let file = self.show_dir(id).join("history").join(format!("{}.json.gz", c.hash));
        let bytes = std::fs::read(&file)?;
        let mut dec = flate2::read::GzDecoder::new(bytes.as_slice());
        let mut text = String::new();
        std::io::Read::read_to_string(&mut dec, &mut text)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn diff(&self, id: &str, from: &str, to: &str) -> Result<Vec<Change>> {
        let a = self.snapshot(id, from)?;
        let b = self.snapshot(id, to)?;
        Ok(songbook_model::diff::diff(&serde_json::to_value(&a)?, &serde_json::to_value(&b)?))
    }

    /// Make a commit's snapshot the current model (a new commit on top).
    pub fn restore(&self, id: &str, commit_id: &str, author: Option<&str>) -> Result<Commit> {
        let index = self.history(id)?;
        let c = index.iter().find(|c| c.id == commit_id).ok_or_else(|| Error::NoCommit(commit_id.into()))?;
        let mut show = self.snapshot(id, commit_id)?;
        let msg = format!("Restore {} ({})", c.id, c.message);
        self.save(&mut show, &msg, author)?.ok_or_else(|| Error::Other("nothing to restore: already current".into()))
    }

    /// Keep a vendor file beside the model, content-addressed, and record it
    /// in `show.vendor`. Saves the show afterwards is the caller's job.
    pub fn add_vendor(&self, show: &mut Show, platform: Platform, kind: &str, file_name: &str, bytes: &[u8], note: &str) -> Result<VendorBlob> {
        let dir = self.show_dir(&show.id).join("vendor");
        std::fs::create_dir_all(&dir)?;
        let sha = hex::encode(Sha256::digest(bytes));
        let ext = vendor_ext(file_name);
        let rel = format!("vendor/{sha}{ext}");
        std::fs::write(self.show_dir(&show.id).join(&rel), bytes)?;
        let blob = VendorBlob {
            platform,
            kind: kind.into(),
            sha256: sha,
            file: rel,
            size: bytes.len() as u64,
            captured_at: songbook_model::now(),
            note: if note.is_empty() { file_name.to_string() } else { format!("{file_name} — {note}") },
        };
        show.vendor.retain(|v| v.sha256 != blob.sha256);
        show.vendor.push(blob.clone());
        Ok(blob)
    }

    pub fn vendor_bytes(&self, id: &str, blob: &VendorBlob) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.show_dir(id).join(&blob.file))?)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let dir = self.show_dir(id);
        if !dir.exists() {
            return Err(Error::NoShow(id.into()));
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Duplicate a show under a new id and name (history starts afresh).
    pub fn duplicate(&self, id: &str, new_name: &str, author: Option<&str>) -> Result<Show> {
        let mut show = self.load(id)?;
        let old_id = show.id.clone();
        show.id = uuid::Uuid::new_v4().to_string();
        show.meta.name = new_name.to_string();
        show.meta.created = songbook_model::now();
        // Vendor blobs travel with the copy.
        let blobs = show.vendor.clone();
        show.vendor.clear();
        std::fs::create_dir_all(self.show_dir(&show.id).join("vendor"))?;
        for b in blobs {
            if let Ok(bytes) = self.vendor_bytes(&old_id, &b) {
                std::fs::write(self.show_dir(&show.id).join(&b.file), bytes)?;
                show.vendor.push(b);
            }
        }
        self.save(&mut show, &format!("Duplicated from {}", old_id), author)?;
        Ok(show)
    }
}

fn vendor_ext(name: &str) -> String {
    let lower = name.to_lowercase();
    for ext in [".tar.gz", ".zip", ".json", ".tgz", ".tar", ".dat", ".clf", ".dm3s", ".tfs", ".dm7s", ".dm3f", ".tff"] {
        if lower.ends_with(ext) {
            return ext.to_string();
        }
    }
    match name.rsplit_once('.') {
        Some((_, e)) if e.len() <= 8 => format!(".{}", e.to_lowercase()),
        _ => String::new(),
    }
}

/// sha256 over the canonical JSON with `meta.modified` blanked, so a save
/// that changes nothing else is not a new snapshot.
pub fn hash_show(show: &Show) -> String {
    let mut v = serde_json::to_value(show).unwrap_or_default();
    if let Some(m) = v.get_mut("meta").and_then(|m| m.as_object_mut()) {
        m.remove("modified");
    }
    hex::encode(Sha256::digest(serde_json::to_string(&v).unwrap_or_default().as_bytes()))
}

fn short_id(seed: &str) -> String {
    hex::encode(Sha256::digest(seed.as_bytes()))[..10].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("songbook-lib-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn save_history_diff_restore() {
        let root = temp();
        let lib = Library::open(&root).unwrap();
        let mut show = Show::new("Gala", Platform::Generic);
        let c1 = lib.save(&mut show, "first", Some("allan")).unwrap().unwrap();
        assert!(lib.save(&mut show, "again", None).unwrap().is_none(), "unchanged show makes no commit");
        show.meta.notes = "changed".into();
        let c2 = lib.save(&mut show, "notes", None).unwrap().unwrap();
        assert_eq!(c2.parent.as_deref(), Some(c1.id.as_str()));
        assert_eq!(c2.changes, 1);
        let d = lib.diff(&show.id, &c1.id, &c2.id).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "meta.notes");
        let entries = lib.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].commits, 2);
        let c3 = lib.restore(&show.id, &c1.id, None).unwrap();
        assert_eq!(lib.load(&show.id).unwrap().meta.notes, "");
        assert_eq!(lib.history(&show.id).unwrap().len(), 3);
        assert_eq!(c3.hash, c1.hash);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vendor_blobs_are_content_addressed() {
        let root = temp();
        let lib = Library::open(&root).unwrap();
        let mut show = Show::new("Gala", Platform::AhSq);
        lib.save(&mut show, "first", None).unwrap();
        let b = lib.add_vendor(&mut show, Platform::AhSq, "sq-show", "NVDATA.DAT", b"PK\x03\x04hello", "from the sim").unwrap();
        assert!(b.file.ends_with(".dat"));
        let b2 = lib.add_vendor(&mut show, Platform::AhSq, "sq-show", "NVDATA.DAT", b"PK\x03\x04hello", "").unwrap();
        assert_eq!(b.sha256, b2.sha256);
        assert_eq!(show.vendor.len(), 1);
        assert_eq!(lib.vendor_bytes(&show.id, &b).unwrap(), b"PK\x03\x04hello");
        std::fs::remove_dir_all(root).unwrap();
    }
}

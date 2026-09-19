//! Mirroring the library's `shows/` tree to and from somewhere else.
//!
//! One algorithm, four back ends. The algorithm is a two-way mirror without
//! deletions: a file that exists on one side only is copied across; a file
//! on both sides goes in the direction of the newer copy (by modification
//! time, with a two-second slack for filesystems that round). Nothing is ever
//! deleted by a sync — a show removed on one side simply comes back from the
//! other, and removing it for good is a deliberate act on both. `show.json`
//! is safe to overwrite in either direction because every version the app
//! ever saved is also in `history/`, which only ever grows.
//!
//! Back ends:
//! - [`FolderProvider`] — any directory; the way to use Dropbox, OneDrive or
//!   Google Drive through their desktop clients, with nothing to register.
//! - [`DropboxProvider`] — the Dropbox HTTP API v2 with an OAuth token.
//! - [`GoogleDriveProvider`] — Drive API v3 with an OAuth token.
//! - [`OneDriveProvider`] — Microsoft Graph with an OAuth token.
//!
//! The three cloud providers follow their public API references and have not
//! been exercised against a live account from this code (that needs an app
//! registration only the account holder can make); the folder provider is
//! tested here.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{Error, Result};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    /// Path relative to the remote root, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Unix seconds.
    pub modified: u64,
    /// Provider-native id when paths are not the address (Google Drive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

pub trait SyncProvider {
    fn name(&self) -> String;
    /// Every file under the remote root, recursively.
    fn list(&mut self) -> Result<Vec<RemoteFile>>;
    fn download(&mut self, file: &RemoteFile) -> Result<Vec<u8>>;
    /// Create or overwrite; `modified` is a hint the provider may keep.
    fn upload(&mut self, path: &str, bytes: &[u8], modified: u64) -> Result<()>;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Both,
    Push,
    Pull,
}

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn local_files(root: &Path) -> Result<BTreeMap<String, (PathBuf, u64, u64)>> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, (PathBuf, u64, u64)>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for e in std::fs::read_dir(dir)?.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                walk(base, &p, out)?;
            } else if p.is_file() {
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                out.insert(rel, (p.clone(), size, mtime(&p)));
            }
        }
        Ok(())
    }
    walk(root, root, &mut out)?;
    Ok(out)
}

/// Mirror `<library root>/shows` with the provider's root.
pub fn sync(library_root: &Path, provider: &mut dyn SyncProvider, direction: Direction) -> Result<SyncReport> {
    let local_root = library_root.join("shows");
    std::fs::create_dir_all(&local_root)?;
    let local = local_files(&local_root)?;
    let remote: BTreeMap<String, RemoteFile> = provider.list()?.into_iter().map(|f| (f.path.clone(), f)).collect();
    let mut report = SyncReport::default();
    const SLACK: u64 = 2;

    for (path, (lp, lsize, lmod)) in &local {
        let want_up = match remote.get(path) {
            None => true,
            Some(r) => *lmod > r.modified + SLACK || (*lmod + SLACK >= r.modified && r.modified + SLACK >= *lmod && *lsize != r.size),
        };
        if want_up && direction != Direction::Pull {
            match std::fs::read(lp).map_err(Error::from).and_then(|b| provider.upload(path, &b, *lmod)) {
                Ok(()) => report.uploaded.push(path.clone()),
                Err(e) => report.errors.push(format!("upload {path}: {e}")),
            }
        } else if !want_up {
            report.unchanged += 1;
        }
    }
    for (path, r) in &remote {
        let want_down = match local.get(path) {
            None => true,
            Some((_, _, lmod)) => r.modified > *lmod + SLACK,
        };
        if want_down && direction != Direction::Push {
            match provider.download(r) {
                Ok(bytes) => {
                    let dest = local_root.join(path);
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    match std::fs::write(&dest, &bytes) {
                        Ok(()) => report.downloaded.push(path.clone()),
                        Err(e) => report.errors.push(format!("write {path}: {e}")),
                    }
                }
                Err(e) => report.errors.push(format!("download {path}: {e}")),
            }
        }
    }
    Ok(report)
}

// ---------------------------------------------------------------- folder

pub struct FolderProvider {
    pub root: PathBuf,
}

impl SyncProvider for FolderProvider {
    fn name(&self) -> String {
        format!("folder {}", self.root.display())
    }
    fn list(&mut self) -> Result<Vec<RemoteFile>> {
        std::fs::create_dir_all(&self.root)?;
        Ok(local_files(&self.root)?
            .into_iter()
            .map(|(path, (_, size, modified))| RemoteFile { path, size, modified, id: None })
            .collect())
    }
    fn download(&mut self, file: &RemoteFile) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.root.join(&file.path))?)
    }
    fn upload(&mut self, path: &str, bytes: &[u8], modified: u64) -> Result<()> {
        let dest = self.root.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, bytes)?;
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(modified);
        let _ = std::fs::File::open(&dest).and_then(|f| f.set_modified(t));
        Ok(())
    }
}

// ---------------------------------------------------------------- dropbox

pub struct DropboxProvider {
    pub token: String,
    /// Folder inside the app's space or the user's Dropbox, e.g. `/Songbook`.
    pub root: String,
}

fn parse_rfc3339_secs(s: &str) -> u64 {
    // 2026-09-19T15:59:16Z → unix seconds, without a date crate.
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T').unwrap_or((s, "00:00:00"));
    let d: Vec<u64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    let t: Vec<u64> = time.split(['.', '+']).next().unwrap_or("0:0:0").split(':').filter_map(|x| x.parse().ok()).collect();
    if d.len() != 3 {
        return 0;
    }
    let (y, m, day) = (d[0] as i64, d[1] as i64, d[2] as i64);
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + (t.first().copied().unwrap_or(0) * 3600 + t.get(1).copied().unwrap_or(0) * 60 + t.get(2).copied().unwrap_or(0)) as i64;
    secs.max(0) as u64
}

impl DropboxProvider {
    fn api(&self, endpoint: &str, arg: Value) -> Result<Value> {
        let resp = ureq::post(&format!("https://api.dropboxapi.com/2/{endpoint}"))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(arg)
            .map_err(|e| Error::Other(format!("dropbox {endpoint}: {e}")))?;
        resp.into_json().map_err(|e| Error::Other(e.to_string()))
    }
    fn remote_path(&self, rel: &str) -> String {
        format!("{}/{}", self.root.trim_end_matches('/'), rel)
    }
}

impl SyncProvider for DropboxProvider {
    fn name(&self) -> String {
        format!("Dropbox {}", self.root)
    }
    fn list(&mut self) -> Result<Vec<RemoteFile>> {
        let root = self.root.trim_end_matches('/').to_string();
        let mut out = vec![];
        let mut v = match self.api("files/list_folder", json!({"path": root, "recursive": true, "limit": 2000})) {
            Ok(v) => v,
            Err(e) => {
                // A missing root is an empty remote, not an error.
                if e.to_string().contains("not_found") {
                    let _ = self.api("files/create_folder_v2", json!({"path": root}));
                    return Ok(vec![]);
                }
                return Err(e);
            }
        };
        loop {
            for e in v.get("entries").and_then(Value::as_array).cloned().unwrap_or_default() {
                if e.get(".tag").and_then(Value::as_str) != Some("file") {
                    continue;
                }
                let full = e.get("path_display").and_then(Value::as_str).unwrap_or("").to_string();
                let rel = full.strip_prefix(&format!("{root}/")).unwrap_or(&full).to_string();
                out.push(RemoteFile {
                    path: rel,
                    size: e.get("size").and_then(Value::as_u64).unwrap_or(0),
                    modified: e.get("client_modified").and_then(Value::as_str).map(parse_rfc3339_secs).unwrap_or(0),
                    id: e.get("id").and_then(Value::as_str).map(str::to_string),
                });
            }
            if v.get("has_more").and_then(Value::as_bool).unwrap_or(false) {
                let cursor = v.get("cursor").and_then(Value::as_str).unwrap_or("").to_string();
                v = self.api("files/list_folder/continue", json!({"cursor": cursor}))?;
            } else {
                break;
            }
        }
        Ok(out)
    }
    fn download(&mut self, file: &RemoteFile) -> Result<Vec<u8>> {
        let resp = ureq::post("https://content.dropboxapi.com/2/files/download")
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Dropbox-API-Arg", &json!({"path": self.remote_path(&file.path)}).to_string())
            .call()
            .map_err(|e| Error::Other(format!("dropbox download: {e}")))?;
        let mut buf = vec![];
        resp.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }
    fn upload(&mut self, path: &str, bytes: &[u8], modified: u64) -> Result<()> {
        let stamp = unix_to_rfc3339(modified);
        let arg = json!({"path": self.remote_path(path), "mode": "overwrite", "mute": true, "client_modified": stamp});
        ureq::post("https://content.dropboxapi.com/2/files/upload")
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Dropbox-API-Arg", &arg.to_string())
            .set("Content-Type", "application/octet-stream")
            .send_bytes(bytes)
            .map_err(|e| Error::Other(format!("dropbox upload: {e}")))?;
        Ok(())
    }
}

fn unix_to_rfc3339(secs: u64) -> String {
    // Civil from days (Howard Hinnant), UTC.
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

// ---------------------------------------------------------------- google drive

pub struct GoogleDriveProvider {
    pub token: String,
    /// Name of the folder in My Drive that holds the library (created if absent).
    pub root_name: String,
    root_id: Option<String>,
    folders: BTreeMap<String, String>,
}

impl GoogleDriveProvider {
    pub fn new(token: String, root_name: String) -> GoogleDriveProvider {
        GoogleDriveProvider { token, root_name, root_id: None, folders: BTreeMap::new() }
    }
    fn get(&self, url: &str) -> Result<Value> {
        let resp = ureq::get(url).set("Authorization", &format!("Bearer {}", self.token)).call().map_err(|e| Error::Other(format!("drive: {e}")))?;
        resp.into_json().map_err(|e| Error::Other(e.to_string()))
    }
    fn query(&self, q: &str) -> Result<Vec<Value>> {
        let mut out = vec![];
        let mut token: Option<String> = None;
        loop {
            let mut url = url::Url::parse("https://www.googleapis.com/drive/v3/files").unwrap();
            url.query_pairs_mut()
                .append_pair("q", q)
                .append_pair("fields", "nextPageToken,files(id,name,mimeType,size,modifiedTime,parents)")
                .append_pair("pageSize", "1000")
                .append_pair("spaces", "drive");
            if let Some(t) = &token {
                url.query_pairs_mut().append_pair("pageToken", t);
            }
            let v = self.get(url.as_str())?;
            out.extend(v.get("files").and_then(Value::as_array).cloned().unwrap_or_default());
            match v.get("nextPageToken").and_then(Value::as_str) {
                Some(t) => token = Some(t.to_string()),
                None => break,
            }
        }
        Ok(out)
    }
    fn create_folder(&self, name: &str, parent: Option<&str>) -> Result<String> {
        let mut meta = json!({"name": name, "mimeType": "application/vnd.google-apps.folder"});
        if let Some(p) = parent {
            meta["parents"] = json!([p]);
        }
        let v: Value = ureq::post("https://www.googleapis.com/drive/v3/files?fields=id")
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(meta)
            .map_err(|e| Error::Other(format!("drive mkdir: {e}")))?
            .into_json()
            .map_err(|e| Error::Other(e.to_string()))?;
        v.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| Error::Other("drive mkdir: no id".into()))
    }
    fn root(&mut self) -> Result<String> {
        if let Some(id) = &self.root_id {
            return Ok(id.clone());
        }
        let q = format!("name = '{}' and mimeType = 'application/vnd.google-apps.folder' and 'root' in parents and trashed = false", self.root_name.replace('\'', "\\'"));
        let found = self.query(&q)?;
        let id = match found.first().and_then(|f| f.get("id")).and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => self.create_folder(&self.root_name, Some("root"))?,
        };
        self.root_id = Some(id.clone());
        self.folders.insert(String::new(), id.clone());
        Ok(id)
    }
    fn folder_for(&mut self, rel_dir: &str) -> Result<String> {
        if let Some(id) = self.folders.get(rel_dir) {
            return Ok(id.clone());
        }
        let (parent_rel, name) = match rel_dir.rsplit_once('/') {
            Some((p, n)) => (p.to_string(), n.to_string()),
            None => (String::new(), rel_dir.to_string()),
        };
        let parent = if parent_rel.is_empty() { self.root()? } else { self.folder_for(&parent_rel)? };
        let q = format!("name = '{}' and mimeType = 'application/vnd.google-apps.folder' and '{}' in parents and trashed = false", name.replace('\'', "\\'"), parent);
        let found = self.query(&q)?;
        let id = match found.first().and_then(|f| f.get("id")).and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => self.create_folder(&name, Some(&parent))?,
        };
        self.folders.insert(rel_dir.to_string(), id.clone());
        Ok(id)
    }
    fn walk(&mut self, folder_id: &str, prefix: &str, out: &mut Vec<RemoteFile>) -> Result<()> {
        let children = self.query(&format!("'{folder_id}' in parents and trashed = false"))?;
        for c in children {
            let name = c.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
            if c.get("mimeType").and_then(Value::as_str) == Some("application/vnd.google-apps.folder") {
                let id = c.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                self.folders.insert(path.clone(), id.clone());
                self.walk(&id, &path, out)?;
            } else {
                out.push(RemoteFile {
                    path,
                    size: c.get("size").and_then(Value::as_str).and_then(|s| s.parse().ok()).unwrap_or(0),
                    modified: c.get("modifiedTime").and_then(Value::as_str).map(parse_rfc3339_secs).unwrap_or(0),
                    id: c.get("id").and_then(Value::as_str).map(str::to_string),
                });
            }
        }
        Ok(())
    }
}

impl SyncProvider for GoogleDriveProvider {
    fn name(&self) -> String {
        format!("Google Drive /{}", self.root_name)
    }
    fn list(&mut self) -> Result<Vec<RemoteFile>> {
        let root = self.root()?;
        let mut out = vec![];
        self.walk(&root, "", &mut out)?;
        Ok(out)
    }
    fn download(&mut self, file: &RemoteFile) -> Result<Vec<u8>> {
        let id = file.id.clone().ok_or_else(|| Error::Other("drive file without id".into()))?;
        let resp = ureq::get(&format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media"))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|e| Error::Other(format!("drive download: {e}")))?;
        let mut buf = vec![];
        resp.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }
    fn upload(&mut self, path: &str, bytes: &[u8], modified: u64) -> Result<()> {
        let (dir, name) = match path.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), path.to_string()),
        };
        let parent = if dir.is_empty() { self.root()? } else { self.folder_for(&dir)? };
        let existing = self.query(&format!("name = '{}' and '{}' in parents and trashed = false", name.replace('\'', "\\'"), parent))?;
        let stamp = unix_to_rfc3339(modified);
        if let Some(id) = existing.first().and_then(|f| f.get("id")).and_then(Value::as_str) {
            ureq::request("PATCH", &format!("https://www.googleapis.com/upload/drive/v3/files/{id}?uploadType=media"))
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Content-Type", "application/octet-stream")
                .send_bytes(bytes)
                .map_err(|e| Error::Other(format!("drive update: {e}")))?;
            ureq::request("PATCH", &format!("https://www.googleapis.com/drive/v3/files/{id}"))
                .set("Authorization", &format!("Bearer {}", self.token))
                .send_json(json!({"modifiedTime": stamp}))
                .map_err(|e| Error::Other(format!("drive touch: {e}")))?;
        } else {
            let boundary = "songbook-drive-boundary";
            let meta = json!({"name": name, "parents": [parent], "modifiedTime": stamp});
            let mut body = vec![];
            body.extend_from_slice(format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
            body.extend_from_slice(bytes);
            body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
            ureq::post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id")
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Content-Type", &format!("multipart/related; boundary={boundary}"))
                .send_bytes(&body)
                .map_err(|e| Error::Other(format!("drive create: {e}")))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- onedrive

pub struct OneDriveProvider {
    pub token: String,
    /// Folder path under the drive root, e.g. `Songbook`.
    pub root: String,
}

impl OneDriveProvider {
    fn item_url(&self, rel: &str) -> String {
        let p = if rel.is_empty() { self.root.clone() } else { format!("{}/{}", self.root.trim_end_matches('/'), rel) };
        let enc: String = p.split('/').map(|seg| url::form_urlencoded::byte_serialize(seg.as_bytes()).collect::<String>()).collect::<Vec<_>>().join("/");
        format!("https://graph.microsoft.com/v1.0/me/drive/root:/{enc}")
    }
    fn get(&self, url: &str) -> Result<Value> {
        let resp = ureq::get(url).set("Authorization", &format!("Bearer {}", self.token)).call().map_err(|e| Error::Other(format!("onedrive: {e}")))?;
        resp.into_json().map_err(|e| Error::Other(e.to_string()))
    }
    fn walk(&self, rel: &str, out: &mut Vec<RemoteFile>) -> Result<()> {
        let mut url = format!("{}:/children?$top=500", self.item_url(rel));
        loop {
            let v = match self.get(&url) {
                Ok(v) => v,
                Err(e) if rel.is_empty() && e.to_string().contains("404") => return Ok(()),
                Err(e) => return Err(e),
            };
            for c in v.get("value").and_then(Value::as_array).cloned().unwrap_or_default() {
                let name = c.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                if c.get("folder").is_some() {
                    self.walk(&path, out)?;
                } else {
                    out.push(RemoteFile {
                        path,
                        size: c.get("size").and_then(Value::as_u64).unwrap_or(0),
                        modified: c
                            .pointer("/fileSystemInfo/lastModifiedDateTime")
                            .or_else(|| c.get("lastModifiedDateTime"))
                            .and_then(Value::as_str)
                            .map(parse_rfc3339_secs)
                            .unwrap_or(0),
                        id: c.get("id").and_then(Value::as_str).map(str::to_string),
                    });
                }
            }
            match v.get("@odata.nextLink").and_then(Value::as_str) {
                Some(next) => url = next.to_string(),
                None => break,
            }
        }
        Ok(())
    }
}

impl SyncProvider for OneDriveProvider {
    fn name(&self) -> String {
        format!("OneDrive /{}", self.root)
    }
    fn list(&mut self) -> Result<Vec<RemoteFile>> {
        let mut out = vec![];
        self.walk("", &mut out)?;
        Ok(out)
    }
    fn download(&mut self, file: &RemoteFile) -> Result<Vec<u8>> {
        let resp = ureq::get(&format!("{}:/content", self.item_url(&file.path)))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .map_err(|e| Error::Other(format!("onedrive download: {e}")))?;
        let mut buf = vec![];
        resp.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }
    fn upload(&mut self, path: &str, bytes: &[u8], modified: u64) -> Result<()> {
        let stamp = unix_to_rfc3339(modified);
        if bytes.len() < 4 * 1024 * 1024 {
            ureq::put(&format!("{}:/content", self.item_url(path)))
                .set("Authorization", &format!("Bearer {}", self.token))
                .set("Content-Type", "application/octet-stream")
                .send_bytes(bytes)
                .map_err(|e| Error::Other(format!("onedrive upload: {e}")))?;
        } else {
            let session: Value = ureq::post(&format!("{}:/createUploadSession", self.item_url(path)))
                .set("Authorization", &format!("Bearer {}", self.token))
                .send_json(json!({"item": {"@microsoft.graph.conflictBehavior": "replace"}}))
                .map_err(|e| Error::Other(format!("onedrive session: {e}")))?
                .into_json()
                .map_err(|e| Error::Other(e.to_string()))?;
            let upload_url = session.get("uploadUrl").and_then(Value::as_str).ok_or_else(|| Error::Other("onedrive: no uploadUrl".into()))?.to_string();
            const CHUNK: usize = 10 * 320 * 1024;
            let total = bytes.len();
            let mut off = 0;
            while off < total {
                let end = (off + CHUNK).min(total);
                ureq::put(&upload_url)
                    .set("Content-Range", &format!("bytes {}-{}/{}", off, end - 1, total))
                    .send_bytes(&bytes[off..end])
                    .map_err(|e| Error::Other(format!("onedrive chunk: {e}")))?;
                off = end;
            }
        }
        let _ = ureq::request("PATCH", &self.item_url(path))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(json!({"fileSystemInfo": {"lastModifiedDateTime": stamp}}));
        Ok(())
    }
}

/// Where each provider sends the browser, ready for [`crate::oauth::begin`].
pub fn oauth_config(provider: &str, client_id: &str, client_secret: Option<&str>) -> Option<crate::oauth::OAuthConfig> {
    Some(match provider {
        "dropbox" => crate::oauth::OAuthConfig {
            authorize_url: "https://www.dropbox.com/oauth2/authorize".into(),
            token_url: "https://api.dropboxapi.com/oauth2/token".into(),
            client_id: client_id.into(),
            client_secret: None,
            scope: "files.metadata.read files.metadata.write files.content.read files.content.write".into(),
            extra_authorize: vec![("token_access_type".into(), "offline".into())],
        },
        "google" => crate::oauth::OAuthConfig {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            client_id: client_id.into(),
            client_secret: client_secret.map(str::to_string),
            scope: "https://www.googleapis.com/auth/drive.file".into(),
            extra_authorize: vec![("access_type".into(), "offline".into()), ("prompt".into(), "consent".into())],
        },
        "onedrive" => crate::oauth::OAuthConfig {
            authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            client_id: client_id.into(),
            client_secret: None,
            scope: "Files.ReadWrite offline_access".into(),
            extra_authorize: vec![],
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_mirror_both_ways() {
        let base = std::env::temp_dir().join(format!("songbook-sync-{}", uuid::Uuid::new_v4()));
        let lib = base.join("lib");
        let remote = base.join("remote");
        std::fs::create_dir_all(lib.join("shows/a")).unwrap();
        std::fs::create_dir_all(remote.join("b")).unwrap();
        std::fs::write(lib.join("shows/a/show.json"), b"local a").unwrap();
        std::fs::write(remote.join("b/show.json"), b"remote b").unwrap();
        let mut p = FolderProvider { root: remote.clone() };
        let r = sync(&lib, &mut p, Direction::Both).unwrap();
        assert_eq!(r.uploaded, vec!["a/show.json"]);
        assert_eq!(r.downloaded, vec!["b/show.json"]);
        assert_eq!(std::fs::read(remote.join("a/show.json")).unwrap(), b"local a");
        assert_eq!(std::fs::read(lib.join("shows/b/show.json")).unwrap(), b"remote b");
        // A second pass changes nothing.
        let r2 = sync(&lib, &mut p, Direction::Both).unwrap();
        assert!(r2.uploaded.is_empty() && r2.downloaded.is_empty());
        assert_eq!(r2.unchanged, 2);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rfc3339_round_trip() {
        assert_eq!(parse_rfc3339_secs("1970-01-02T00:00:00Z"), 86400);
        assert_eq!(unix_to_rfc3339(86400), "1970-01-02T00:00:00Z");
        let t = 1_790_000_000u64;
        assert_eq!(parse_rfc3339_secs(&unix_to_rfc3339(t)), t);
        assert_eq!(parse_rfc3339_secs("2026-09-19T15:59:16.000Z"), parse_rfc3339_secs("2026-09-19T15:59:16Z"));
    }
}

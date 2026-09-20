//! The browser build (Songbook Lite) calls Rust through one function.
//!
//! A plain C ABI, no `wasm-bindgen`: the interface is bytes in and bytes out,
//! so `cargo build --target wasm32-unknown-unknown` is the whole toolchain
//! (the same choice PatchFerret made). Every integer is little-endian u32.
//!
//! Request buffer, written into memory from [`sb_alloc`]:
//!
//! ```text
//! u32 json length | JSON header | u32 blob count | (u32 length | bytes)*
//! ```
//!
//! Response buffer, returned by [`sb_call`] (free it with [`sb_free`] using the
//! total in its first word plus 4):
//!
//! ```text
//! u32 total after this word | u32 json length | JSON | u32 blob count | (u32 length | bytes)*
//! ```
//!
//! The header's `op` picks the operation; the JSON result always has either
//! `ok` or `error`. Before any call the host sets the clock and seeds the ID
//! generator (`sb_set_clock`, `sb_seed`), because wasm has neither.

use std::alloc::{alloc, dealloc, Layout};

use serde_json::{json, Value};
use songbook_model::{Platform, Show};

/// Allocate `len` bytes for the host to write a request into.
///
/// # Safety
/// The caller must hand the pointer to [`sb_call`] (which takes ownership) or
/// free it with [`sb_free`] and the same `len`.
#[no_mangle]
pub unsafe extern "C" fn sb_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    match Layout::from_size_align(len, 1) {
        Ok(layout) => alloc(layout),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release a buffer from [`sb_alloc`] or [`sb_call`].
///
/// # Safety
/// `ptr` must have come from this module with the same `len`.
#[no_mangle]
pub unsafe extern "C" fn sb_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len, 1) {
        dealloc(ptr, layout);
    }
}

#[no_mangle]
pub extern "C" fn sb_set_clock(unix_secs: i64) {
    songbook_model::set_clock(unix_secs);
}

#[no_mangle]
pub extern "C" fn sb_seed(seed: u64) {
    songbook_model::seed_ids(seed);
}

/// Run one operation. Takes ownership of the request buffer.
///
/// # Safety
/// `ptr`/`len` must be a buffer from [`sb_alloc`] holding a well-formed request.
#[no_mangle]
pub unsafe extern "C" fn sb_call(ptr: *mut u8, len: usize) -> *mut u8 {
    let input = if ptr.is_null() { Vec::new() } else { Vec::from_raw_parts(ptr, len, len) };
    let (json, blobs) = match decode(&input) {
        Ok((header, blobs)) => match dispatch(header, blobs) {
            Ok((v, b)) => (json!({ "ok": v }), b),
            Err(e) => (json!({ "error": e }), vec![]),
        },
        Err(e) => (json!({ "error": e }), vec![]),
    };
    let out = encode(&json, &blobs);
    let mut out = std::mem::ManuallyDrop::new(out);
    out.shrink_to_fit();
    out.as_mut_ptr()
}

fn u32_at(d: &[u8], at: usize) -> Result<usize, String> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(|| "request truncated".to_string())
}

fn decode(d: &[u8]) -> Result<(Value, Vec<Vec<u8>>), String> {
    let jl = u32_at(d, 0)?;
    let header: Value = serde_json::from_slice(d.get(4..4 + jl).ok_or("request truncated")?).map_err(|e| format!("request header: {e}"))?;
    let mut at = 4 + jl;
    let n = u32_at(d, at)?;
    at += 4;
    let mut blobs = Vec::with_capacity(n);
    for _ in 0..n {
        let l = u32_at(d, at)?;
        at += 4;
        blobs.push(d.get(at..at + l).ok_or("request truncated")?.to_vec());
        at += l;
    }
    Ok((header, blobs))
}

fn encode(json: &Value, blobs: &[Vec<u8>]) -> Vec<u8> {
    let js = serde_json::to_vec(json).unwrap_or_else(|_| b"{\"error\":\"unserialisable result\"}".to_vec());
    let total = 4 + js.len() + 4 + blobs.iter().map(|b| 4 + b.len()).sum::<usize>();
    let mut out = Vec::with_capacity(4 + total);
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(&js);
    out.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
    for b in blobs {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    out
}

type Out = Result<(Value, Vec<Vec<u8>>), String>;

fn field<'a>(h: &'a Value, k: &str) -> Result<&'a Value, String> {
    h.get(k).ok_or_else(|| format!("request needs `{k}`"))
}

fn show_of(h: &Value) -> Result<Show, String> {
    serde_json::from_value(field(h, "show")?.clone()).map_err(|e| format!("show: {e}"))
}

fn platform_of(v: &Value) -> Result<Platform, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("platform: {e}"))
}

fn to_value<T: serde::Serialize>(t: T) -> Result<Value, String> {
    serde_json::to_value(t).map_err(|e| e.to_string())
}

fn dispatch(h: Value, blobs: Vec<Vec<u8>>) -> Out {
    let op = h.get("op").and_then(Value::as_str).ok_or("request needs `op`")?;
    match op {
        "info" => Ok((
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "platforms": Platform::all().iter().map(|p| json!({"id": p, "label": p.label(), "vendor": p.vendor(), "models": songbook_convert::models(*p)})).collect::<Vec<_>>(),
            }),
            vec![],
        )),
        "import" => import(&h, blobs),
        "new" => {
            let name = field(&h, "name")?.as_str().unwrap_or("Untitled");
            let platform = platform_of(field(&h, "platform")?)?;
            let model = field(&h, "model")?.as_str().unwrap_or("");
            Ok((to_value(songbook_convert::blank_show(name, platform, model))?, vec![]))
        }
        "validate" => Ok((to_value(show_of(&h)?.validate())?, vec![])),
        "diff" => {
            let before = field(&h, "before")?;
            let after = field(&h, "after")?;
            Ok((to_value(songbook_model::diff::diff(before, after))?, vec![]))
        }
        "summary" => Ok((to_value(songbook_model::summary::Summary::of(&show_of(&h)?))?, vec![])),
        "capabilities" => {
            let platform = platform_of(field(&h, "platform")?)?;
            let model = h.get("model").and_then(Value::as_str).unwrap_or("");
            Ok((to_value(songbook_convert::capabilities(platform, model))?, vec![]))
        }
        "convert" => {
            let show = show_of(&h)?;
            let target = platform_of(field(&h, "target")?)?;
            let model = h.get("model").and_then(Value::as_str).unwrap_or("");
            Ok((to_value(songbook_convert::convert(&show, target, model))?, vec![]))
        }
        "companion_export" => {
            let show = show_of(&h)?;
            let opts: songbook_companion::ExportOptions = serde_json::from_value(field(&h, "opts")?.clone()).map_err(|e| format!("opts: {e}"))?;
            let doc = songbook_companion::export(&show, &opts)?;
            Ok((doc, vec![]))
        }
        "companion_import" => {
            let show = show_of(&h)?;
            let bytes = blobs.into_iter().next().ok_or("companion_import needs the file as blob 0")?;
            let report = songbook_companion::import(&bytes, &show)?;
            Ok((to_value(report)?, vec![]))
        }
        "vendor_write" => vendor_write(&h, blobs),
        other => Err(format!("unknown op `{other}`")),
    }
}

/// Import one or more files: several `.DAT` images make an SQ show, anything
/// else is one file. Returns the show, what it was, and the vendor bytes to
/// keep (blob 0) under the name in `vendorName`.
fn import(h: &Value, blobs: Vec<Vec<u8>>) -> Out {
    let names: Vec<String> = h.get("files").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.get("name").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
    if names.len() != blobs.len() || names.is_empty() {
        return Err("import needs `files: [{name}]` matching the blobs".into());
    }
    let platform: Option<Platform> = h.get("platform").filter(|v| !v.is_null()).map(platform_of).transpose()?;
    let dats: Vec<(String, Vec<u8>)> = names.iter().cloned().zip(blobs.iter().cloned()).filter(|(n, _)| n.to_uppercase().ends_with(".DAT")).collect();
    if !dats.is_empty() {
        let mut files = songbook_ah::import::SqFiles::new();
        for (n, d) in dats {
            files.insert(n.to_uppercase(), d);
        }
        let show_name = h.get("name").and_then(Value::as_str).unwrap_or("SQ show").to_string();
        let r = songbook_ah::import::import_sq_files(&show_name, files, platform).map_err(|e| e.to_string())?;
        let (vname, vbytes) = r.vendor.unwrap_or_default();
        return Ok((json!({ "show": r.show, "kind": r.kind, "vendorName": vname }), vec![vbytes]));
    }
    let name = names[0].clone();
    let bytes = blobs.into_iter().next().unwrap_or_default();
    let lower = name.to_lowercase();
    if lower.ends_with(".json") {
        let mut s: Show = serde_json::from_slice(&bytes).map_err(|e| format!("{name}: {e}"))?;
        if s.schema != "songbook/1" {
            return Err(format!("{name}: not a Songbook show (schema {})", s.schema));
        }
        s.id = songbook_model::new_id();
        return Ok((json!({ "show": s, "kind": "songbook", "vendorName": Value::Null }), vec![]));
    }
    if songbook_yamaha::looks_like(&bytes) {
        let r = songbook_yamaha::import_bytes(&name, &bytes).map_err(|e| format!("{name}: {e}"))?;
        return Ok((json!({ "show": r.show, "kind": r.kind, "vendorName": name }), vec![bytes]));
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".zip") || bytes.starts_with(&[0x1F, 0x8B]) {
        let r = songbook_ah::import::import_bytes(&name, &bytes, platform).map_err(|e| e.to_string())?;
        let (vname, vbytes) = r.vendor.unwrap_or((name.clone(), bytes.clone()));
        return Ok((json!({ "show": r.show, "kind": r.kind, "vendorName": vname }), vec![vbytes]));
    }
    Err(format!("{name}: not a show file Songbook knows (an SQ show's NVDATA.DAT + SCENEnnn.DAT, a dLive / Avantis show .tar.gz, a Yamaha .CLF, .dm3s / .tfs / .dm7s, or a Songbook .json)"))
}

/// Write the show into a copy of its kept vendor file (blob 0). SQ shows come
/// back as one blob per image (`fileNames` lists them) so the page can offer
/// the folder's files or zip them; a `.CLF` comes back as one blob.
fn vendor_write(h: &Value, blobs: Vec<Vec<u8>>) -> Out {
    let show = show_of(h)?;
    let kind = field(h, "kind")?.as_str().unwrap_or("");
    let original = blobs.into_iter().next().ok_or("vendor_write needs the kept file as blob 0")?;
    match kind {
        "sq-show" => {
            let files = songbook_ah::write::unzip(&original).map_err(|e| e.to_string())?;
            let (out, report) = songbook_ah::write::write_images(&files, &show).map_err(|e| e.to_string())?;
            let as_zip = h.get("asZip").and_then(Value::as_bool).unwrap_or(false);
            if as_zip {
                let z = songbook_ah::write::zip_files(&out).map_err(|e| e.to_string())?;
                return Ok((json!({ "report": report, "fileNames": ["show.zip"] }), vec![z]));
            }
            let names: Vec<String> = out.keys().cloned().collect();
            let bytes: Vec<Vec<u8>> = out.into_values().collect();
            Ok((json!({ "report": report, "fileNames": names }), bytes))
        }
        "clf" => {
            let (out, report) = songbook_yamaha::clf::write(&original, &show).map_err(|e| e.to_string())?;
            Ok((json!({ "report": report, "fileNames": ["show.CLF"] }), vec![out]))
        }
        other => Err(format!("Songbook cannot write a {other} file: only SQ shows and CL/QL .CLF files have solved checksums")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(header: Value, blobs: &[&[u8]]) -> (Value, Vec<Vec<u8>>) {
        let js = serde_json::to_vec(&header).unwrap();
        let mut req = Vec::new();
        req.extend_from_slice(&(js.len() as u32).to_le_bytes());
        req.extend_from_slice(&js);
        req.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
        for b in blobs {
            req.extend_from_slice(&(b.len() as u32).to_le_bytes());
            req.extend_from_slice(b);
        }
        let (h, bl) = decode(&req).unwrap();
        let (v, out) = match dispatch(h, bl) {
            Ok((v, b)) => (json!({"ok": v}), b),
            Err(e) => (json!({"error": e}), vec![]),
        };
        // Round-trip the response framing too.
        let enc = encode(&v, &out);
        let total = u32::from_le_bytes(enc[0..4].try_into().unwrap()) as usize;
        assert_eq!(total + 4, enc.len());
        (v, out)
    }

    #[test]
    fn info_lists_platforms() {
        let (v, _) = call(json!({"op": "info"}), &[]);
        assert!(v["ok"]["platforms"].as_array().unwrap().len() > 5);
    }

    #[test]
    fn new_show_then_convert_then_companion() {
        let (v, _) = call(json!({"op": "new", "name": "Lite", "platform": "ah-sq", "model": "SQ-5"}), &[]);
        let show = v["ok"].clone();
        assert_eq!(show["platform"], "ah-sq");
        let (c, _) = call(json!({"op": "convert", "show": show, "target": "yamaha-dm3", "model": "DM3"}), &[]);
        assert_eq!(c["ok"]["show"]["platform"], "yamaha-dm3");
        let (e, _) = call(json!({"op": "companion_export", "show": show, "opts": {"connectionLabel": "sq", "host": "10.0.0.5", "includeChannels": true, "includeDcas": true, "includeMuteGroups": true, "includeScenes": true, "pageName": "SQ"}}), &[]);
        assert_eq!(e["ok"]["type"], "full");
    }

    #[test]
    fn imports_a_synthetic_clf_and_writes_it_back() {
        let clf = songbook_yamaha::clf::synthetic("QL [OSX, 5.8.1.27]", &[(1, "Kick"), (2, "Snare")], &[(1, 0x01)]);
        let (v, blobs) = call(json!({"op": "import", "files": [{"name": "x.CLF"}]}), &[&clf]);
        assert_eq!(v["ok"]["kind"], "clf", "{v}");
        assert_eq!(blobs.len(), 1);
        let mut show = v["ok"]["show"].clone();
        show["channels"][0]["label"] = json!("Bass");
        let (w, out) = call(json!({"op": "vendor_write", "show": show, "kind": "clf"}), &[&blobs[0]]);
        assert!(w["ok"]["report"]["namesWritten"].as_u64().unwrap() >= 1, "{w}");
        assert_eq!(out.len(), 1);
        assert!(songbook_yamaha::clf::memapi_checksum_ok(&out[0]).unwrap_or(true));
    }

    #[test]
    fn rejects_an_unknown_file() {
        let (v, _) = call(json!({"op": "import", "files": [{"name": "notes.txt"}]}), &[b"hello"]);
        assert!(v["error"].as_str().unwrap().contains("not a show file"));
    }
}

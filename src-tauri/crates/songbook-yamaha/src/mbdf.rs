//! The `#YAMAHA MBDF…` container that wraps DM3 / DM7 / TF scenes and
//! presets (`.dm3s`, `.dm7s`, `.tfs`, `.dm3p`, `.dm7p`, `.tfp`).
//!
//! ```text
//! header, 0x48 bytes:   magic "#YAMAHA MBDFScene" NUL-padded (24)
//!                       u32be 0x24 (record header size) at 0x18
//!                       model at 0x24 (16), version word at 0x34, digest at 0x38
//! record, 36 bytes:     "#MMS FIELD" (12) | name (12) | u32be extra | u32be payload_len
//!                       then `extra` bytes of target, the payload, pad to 4
//! terminator:           "#END"
//! ```
//!
//! Container headers are big-endian; the MMSXLIT payload inside each record
//! is little-endian (see [`crate::mms`]). The layout was established on
//! Yamaha's published TF preset pack and holds unchanged for the factory
//! scenes inside DM3 Editor and TF Editor; it is the same reader PatchFerret
//! ships, carried here under the same licence.

use crate::Error;

const HEADER_LEN: usize = 0x48;
const REC_HEADER_LEN: usize = 36;
pub const MAGIC_PREFIX: &[u8] = b"#YAMAHA MBDF";

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

#[derive(Debug, Clone)]
pub struct Record {
    /// `Scene`, `Mixing`, `Process`, `FX`, …
    pub name: String,
    pub target: Vec<u8>,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Container {
    /// Text after `#YAMAHA MBDF` — `Scene`, `Preset`, …
    pub subtype: String,
    /// `DM3`, `TF`, `DM7`.
    pub model: String,
    pub version: [u8; 4],
    pub records: Vec<Record>,
}

impl Container {
    pub fn looks_like(data: &[u8]) -> bool {
        data.starts_with(MAGIC_PREFIX)
    }

    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if !Self::looks_like(data) {
            return Err(Error::NotMbdf);
        }
        if data.len() < HEADER_LEN {
            return Err(Error::Truncated("header"));
        }
        let magic = cstr(&data[..0x18]);
        let subtype = magic.strip_prefix("#YAMAHA MBDF").unwrap_or_default().to_string();
        let model = cstr(&data[0x24..0x34]);
        let mut version = [0u8; 4];
        version.copy_from_slice(&data[0x34..0x38]);

        let mut records = Vec::new();
        let mut off = HEADER_LEN;
        let mut saw_end = false;
        while off + 12 <= data.len() {
            let tag = cstr(&data[off..off + 12]);
            if tag == "#END" {
                saw_end = true;
                break;
            }
            if off + REC_HEADER_LEN > data.len() {
                return Err(Error::Truncated("record header"));
            }
            if tag != "#MMS FIELD" {
                return Err(Error::BadRecord(tag));
            }
            let name = cstr(&data[off + 12..off + 24]);
            let extra = be_u32(data, off + 24).ok_or(Error::Truncated("record header"))? as usize;
            let plen = be_u32(data, off + 28).ok_or(Error::Truncated("record header"))? as usize;
            let pstart = off.checked_add(REC_HEADER_LEN).and_then(|v| v.checked_add(extra)).ok_or(Error::Truncated("record"))?;
            let pend = pstart.checked_add(plen).ok_or(Error::Truncated("record"))?;
            if pend > data.len() {
                return Err(Error::Truncated("payload"));
            }
            records.push(Record { name, target: data[off + REC_HEADER_LEN..pstart].to_vec(), payload: data[pstart..pend].to_vec() });
            off = (pend + 3) & !3;
        }
        if !saw_end {
            return Err(Error::Truncated("no #END record"));
        }
        Ok(Container { subtype, model, version, records })
    }

    pub fn record(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.name == name)
    }
}

/// Synthesises containers to the documented layout, for tests and demos.
pub mod build {
    use super::{HEADER_LEN, REC_HEADER_LEN};

    pub fn container(subtype: &str, model: &str, records: &[(&str, &[u8], &[u8])]) -> Vec<u8> {
        let mut out = vec![0u8; HEADER_LEN];
        let magic = format!("#YAMAHA MBDF{subtype}");
        out[..magic.len()].copy_from_slice(magic.as_bytes());
        out[0x18..0x1C].copy_from_slice(&0x24u32.to_be_bytes());
        out[0x24..0x24 + model.len()].copy_from_slice(model.as_bytes());
        out[0x34..0x38].copy_from_slice(&[0x56, 0x04, 0x02, 0x00]);
        for (name, target, payload) in records {
            let mut hdr = vec![0u8; REC_HEADER_LEN];
            hdr[..10].copy_from_slice(b"#MMS FIELD");
            hdr[12..12 + name.len()].copy_from_slice(name.as_bytes());
            hdr[24..28].copy_from_slice(&(target.len() as u32).to_be_bytes());
            hdr[28..32].copy_from_slice(&(payload.len() as u32).to_be_bytes());
            out.extend_from_slice(&hdr);
            out.extend_from_slice(target);
            out.extend_from_slice(payload);
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
        }
        let mut end = vec![0u8; REC_HEADER_LEN];
        end[..4].copy_from_slice(b"#END");
        out.extend_from_slice(&end);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records_and_padding() {
        let raw = build::container("Scene", "DM3", &[("A", b"", b"x"), ("Mixing", b"CH\0\0\0\0\0\x01", b"world!!"), ("C", b"", b"zzz")]);
        let c = Container::parse(&raw).unwrap();
        assert_eq!((c.subtype.as_str(), c.model.as_str(), c.records.len()), ("Scene", "DM3", 3));
        assert_eq!(c.record("Mixing").unwrap().target, b"CH\0\0\0\0\0\x01");
        assert_eq!(c.record("C").unwrap().payload, b"zzz");
    }

    #[test]
    fn rejects_bad_input_without_panicking() {
        assert!(matches!(Container::parse(b"not yamaha"), Err(Error::NotMbdf)));
        let raw = build::container("Scene", "DM3", &[("Mixing", b"", b"data")]);
        for cut in (0..raw.len() - 36).step_by(5) {
            assert!(Container::parse(&raw[..cut]).is_err());
        }
        let mut bad = raw.clone();
        bad[0x48 + 28..0x48 + 32].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert!(matches!(Container::parse(&bad), Err(Error::Truncated(_))));
    }
}

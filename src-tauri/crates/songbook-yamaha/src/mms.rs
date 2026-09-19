//! MMSXLIT — Yamaha's self-describing parameter payload.
//!
//! ```text
//! "MMSXLIT\0" <function name>
//! schema:  COL record, 48 bytes   0 "COL" | 3 level | 4 name (28) | 36 u32le offset | 40 u32le datasize | 44 u32le arraysize
//!          PR  record, 32 bytes   0 "PR " | 3 type (0 string, 1 signed, 2 unsigned) | 4 u16le size | 6 u16le arraysize | 8 name (24)
//! values:  packed structs laid out per the schema, little-endian
//! ```
//!
//! Because the schema travels inside the file, one decoder serves every
//! model: the DM3's `Mixing` record declares 207 collections and 494
//! parameters and its `mms_Mixing.xml` says the same, with the value block
//! starting exactly where the schema walk ends. Field names still differ per
//! model (DM3 `Patch/Source` is 4 bytes, TF `Patch/Select` is 1, DM7 says
//! `InPatch`), which is why every read goes by name and reports absence.

use crate::Error;

const COL_REC: usize = 48;
const PR_REC: usize = 32;

pub const TYPE_STRING: u8 = 0x00;
pub const TYPE_SIGNED: u8 = 0x01;
pub const TYPE_UNSIGNED: u8 = 0x02;

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Collection { children: Vec<Node> },
    Parameter { type_code: u8 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    /// Byte offset within the parent element.
    pub offset: u32,
    pub datasize: u32,
    pub arraysize: u32,
    pub kind: NodeKind,
}

impl Node {
    pub fn is_collection(&self) -> bool {
        matches!(self.kind, NodeKind::Collection { .. })
    }
    pub fn children(&self) -> &[Node] {
        match &self.kind {
            NodeKind::Collection { children } => children,
            NodeKind::Parameter { .. } => &[],
        }
    }
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children().iter().find(|c| c.name == name)
    }
    pub fn span(&self) -> u32 {
        self.datasize.saturating_mul(self.arraysize)
    }
    pub fn type_code(&self) -> Option<u8> {
        match self.kind {
            NodeKind::Parameter { type_code } => Some(type_code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Payload {
    pub function: String,
    pub root: Node,
    pub values_at: usize,
    pub collections: usize,
    pub parameters: usize,
    data: Vec<u8>,
}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
    cols: usize,
    prs: usize,
}

impl Cursor<'_> {
    fn node(&mut self) -> Result<Node, Error> {
        let d = self.data;
        let o = self.at;
        let tag = d.get(o..o + 3).ok_or(Error::Truncated("schema record"))?;
        if tag == b"COL" {
            let name = cstr(d.get(o + 4..o + 32).ok_or(Error::Truncated("COL name"))?);
            let offset = u32le(d, o + 36).ok_or(Error::Truncated("COL offset"))?;
            let datasize = u32le(d, o + 40).ok_or(Error::Truncated("COL datasize"))?;
            let arraysize = u32le(d, o + 44).ok_or(Error::Truncated("COL arraysize"))?;
            self.at += COL_REC;
            self.cols += 1;
            let mut children = Vec::new();
            let mut consumed: u32 = 0;
            while consumed < datasize {
                let before = self.at;
                let child = self.node()?;
                if self.at == before {
                    return Err(Error::BadSchema("schema made no progress"));
                }
                consumed = consumed.saturating_add(child.span());
                children.push(child);
                if children.len() > 4096 {
                    return Err(Error::BadSchema("implausible child count"));
                }
            }
            Ok(Node { name, offset, datasize, arraysize, kind: NodeKind::Collection { children } })
        } else if tag == b"PR " {
            let type_code = d[o + 3];
            let size = u16le(d, o + 4).ok_or(Error::Truncated("PR size"))? as u32;
            let arraysize = u16le(d, o + 6).ok_or(Error::Truncated("PR arraysize"))? as u32;
            let name = cstr(d.get(o + 8..o + 32).ok_or(Error::Truncated("PR name"))?);
            self.at += PR_REC;
            self.prs += 1;
            Ok(Node { name, offset: 0, datasize: size, arraysize, kind: NodeKind::Parameter { type_code } })
        } else {
            Err(Error::BadSchema("expected COL or PR record"))
        }
    }
}

/// Parameters carry no offset of their own; they are laid out end to end
/// from the start of their parent, and collections state theirs.
fn assign_parameter_offsets(node: &mut Node) {
    if let NodeKind::Collection { children } = &mut node.kind {
        let mut run = 0u32;
        for c in children.iter_mut() {
            if c.is_collection() {
                run = c.offset.saturating_add(c.span());
                assign_parameter_offsets(c);
            } else {
                c.offset = run;
                run = run.saturating_add(c.span());
            }
        }
    }
}

/// A path into the tree with an element index per array level:
/// `["InputChannel", "ToMix", "Level"]` with indices `[3, 1]` is channel 4's
/// send to mix 2.
impl Payload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        if !payload.starts_with(b"MMSXLIT") {
            return Err(Error::NotMms);
        }
        let function = cstr(payload.get(8..40).ok_or(Error::Truncated("function name"))?);
        let start = payload.windows(3).position(|w| w == b"COL").ok_or(Error::BadSchema("no schema block"))?;
        let mut cur = Cursor { data: payload, at: start, cols: 0, prs: 0 };
        let mut root = cur.node()?;
        assign_parameter_offsets(&mut root);
        let values_at = cur.at;
        if values_at > payload.len() {
            return Err(Error::Truncated("schema overruns payload"));
        }
        Ok(Payload { function, root, values_at, collections: cur.cols, parameters: cur.prs, data: payload.to_vec() })
    }

    /// Whether the schema's children account for exactly the bytes the root
    /// declares; if not, offsets below the mismatch are not trustworthy.
    pub fn consistent(&self) -> bool {
        let summed: u32 = self.root.children().iter().map(|c| c.span()).sum();
        summed == self.root.datasize
    }

    /// Absolute offset of the element named by `path`, with `indices`
    /// consumed by each array node (arraysize > 1) met along the way.
    fn locate(&self, path: &[&str], indices: &[u32]) -> Option<(usize, &Node)> {
        let mut node = &self.root;
        let mut at = self.values_at as u64;
        let mut idx = indices.iter();
        for name in path {
            let child = node.child(name)?;
            at += child.offset as u64;
            if child.arraysize > 1 {
                let i = idx.next().copied().unwrap_or(0);
                if i >= child.arraysize {
                    return None;
                }
                at += (child.datasize as u64) * (i as u64);
            }
            node = child;
        }
        Some((at as usize, node))
    }

    pub fn node(&self, path: &[&str]) -> Option<&Node> {
        let mut node = &self.root;
        for name in path {
            node = node.child(name)?;
        }
        Some(node)
    }

    pub fn string(&self, path: &[&str], indices: &[u32]) -> Option<String> {
        let (at, node) = self.locate(path, indices)?;
        let end = at.checked_add(node.datasize as usize)?;
        self.data.get(at..end).map(cstr)
    }

    pub fn uint(&self, path: &[&str], indices: &[u32]) -> Option<u64> {
        let (at, node) = self.locate(path, indices)?;
        let width = node.datasize as usize;
        if width == 0 || width > 8 {
            return None;
        }
        let bytes = self.data.get(at..at.checked_add(width)?)?;
        let mut v: u64 = 0;
        for (i, b) in bytes.iter().enumerate() {
            v |= (*b as u64) << (8 * i);
        }
        Some(v)
    }

    /// A signed value of whatever width the schema declares.
    pub fn int(&self, path: &[&str], indices: &[u32]) -> Option<i64> {
        let (_, node) = self.locate(path, indices)?;
        let width = node.datasize as usize;
        let u = self.uint(path, indices)?;
        Some(match width {
            1 => u as u8 as i8 as i64,
            2 => u as u16 as i16 as i64,
            4 => u as u32 as i32 as i64,
            _ => u as i64,
        })
    }

    /// Read a number, signed or not, as the schema says.
    pub fn number(&self, path: &[&str], indices: &[u32]) -> Option<i64> {
        let (_, node) = self.locate(path, indices)?;
        match node.type_code()? {
            TYPE_SIGNED => self.int(path, indices),
            TYPE_UNSIGNED => self.uint(path, indices).map(|v| v as i64),
            _ => None,
        }
    }

    pub fn array_len(&self, path: &[&str]) -> Option<u32> {
        self.node(path).map(|n| n.arraysize)
    }

    pub fn declared_size(&self) -> u32 {
        self.root.datasize
    }
}

/// Synthesises MMSXLIT payloads for tests and demos.
pub mod build {
    pub fn col(name: &str, offset: u32, datasize: u32, arraysize: u32) -> Vec<u8> {
        let mut r = vec![0u8; 48];
        r[..3].copy_from_slice(b"COL");
        r[3] = b'0';
        r[4..4 + name.len()].copy_from_slice(name.as_bytes());
        r[36..40].copy_from_slice(&offset.to_le_bytes());
        r[40..44].copy_from_slice(&datasize.to_le_bytes());
        r[44..48].copy_from_slice(&arraysize.to_le_bytes());
        r
    }

    pub fn pr(name: &str, type_code: u8, size: u16, arraysize: u16) -> Vec<u8> {
        let mut r = vec![0u8; 32];
        r[..3].copy_from_slice(b"PR ");
        r[3] = type_code;
        r[4..6].copy_from_slice(&size.to_le_bytes());
        r[6..8].copy_from_slice(&arraysize.to_le_bytes());
        r[8..8 + name.len()].copy_from_slice(name.as_bytes());
        r
    }

    pub fn payload(function: &str, schema: Vec<Vec<u8>>, values: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 44];
        out[..7].copy_from_slice(b"MMSXLIT");
        out[8..8 + function.len()].copy_from_slice(function.as_bytes());
        for r in schema {
            out.extend_from_slice(&r);
        }
        out.extend_from_slice(values);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    /// Two channels of {Name[8], Fader{Level i16, On u8}, ToMix[2]{Level i16}}: datasize 8+3+4 = 15.
    fn sample() -> Vec<u8> {
        let schema = vec![
            col("Mixing", 0, 30, 1),
            col("InputChannel", 0, 15, 2),
            col("Label", 0, 8, 1),
            pr("Name", TYPE_STRING, 8, 1),
            col("Fader", 8, 3, 1),
            pr("Level", TYPE_SIGNED, 2, 1),
            pr("On", TYPE_UNSIGNED, 1, 1),
            col("ToMix", 11, 2, 2),
            pr("Level", TYPE_SIGNED, 2, 1),
        ];
        let mut v = Vec::new();
        v.extend_from_slice(b"Kick\0\0\0\0");
        v.extend_from_slice(&(-300i16).to_le_bytes());
        v.push(1);
        v.extend_from_slice(&(-1000i16).to_le_bytes());
        v.extend_from_slice(&(-32768i16).to_le_bytes());
        v.extend_from_slice(b"Snare\0\0\0");
        v.extend_from_slice(&(250i16).to_le_bytes());
        v.push(0);
        v.extend_from_slice(&(0i16).to_le_bytes());
        v.extend_from_slice(&(-600i16).to_le_bytes());
        payload("Mixing", schema, &v)
    }

    #[test]
    fn reads_strings_signed_and_nested_arrays() {
        let p = Payload::parse(&sample()).unwrap();
        assert_eq!((p.collections, p.parameters), (5, 4));
        assert!(p.consistent());
        assert_eq!(p.array_len(&["InputChannel"]), Some(2));
        assert_eq!(p.string(&["InputChannel", "Label", "Name"], &[1]).as_deref(), Some("Snare"));
        assert_eq!(p.number(&["InputChannel", "Fader", "Level"], &[0]), Some(-300));
        assert_eq!(p.number(&["InputChannel", "Fader", "On"], &[1]), Some(0));
        assert_eq!(p.number(&["InputChannel", "ToMix", "Level"], &[0, 1]), Some(-32768));
        assert_eq!(p.number(&["InputChannel", "ToMix", "Level"], &[1, 1]), Some(-600));
        assert_eq!(p.number(&["InputChannel", "ToMix", "Level"], &[2, 0]), None);
        assert_eq!(p.string(&["Nope"], &[]), None);
    }

    #[test]
    fn rejects_bad_schemas_and_truncation() {
        assert!(matches!(Payload::parse(b"not mms"), Err(Error::NotMms)));
        let schema = vec![col("Mixing", 0, 9999, 1), pr("X", TYPE_STRING, 4, 1)];
        assert!(Payload::parse(&payload("Mixing", schema, b"aaaa")).is_err());
        let full = sample();
        for cut in (0..full.len()).step_by(7) {
            let _ = Payload::parse(&full[..cut]);
        }
    }
}

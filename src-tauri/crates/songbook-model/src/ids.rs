//! ID spellings. Drivers build IDs through these so every platform agrees on
//! the prefixes and the inspector can tell a channel from a bus by eye.

use crate::{BusKind, Direction};

pub fn unit(name: &str) -> String {
    if name.starts_with("unit:") { name.to_string() } else { format!("unit:{name}") }
}

/// `skt:<unit>:<in|out>:<n>` — the unit name without its `unit:` prefix.
pub fn socket(unit: &str, direction: Direction, n: u32) -> String {
    let unit = unit.strip_prefix("unit:").unwrap_or(unit);
    let d = match direction {
        Direction::In => "in",
        Direction::Out => "out",
    };
    format!("skt:{unit}:{d}:{n}")
}

pub fn channel(n: u32) -> String {
    format!("ch:{n}")
}
pub fn stereo_input(n: u32) -> String {
    format!("st:{n}")
}
pub fn fx_return(n: u32) -> String {
    format!("fxr:{n}")
}

pub fn bus(kind: BusKind, n: u32) -> String {
    let k = match kind {
        BusKind::Main => "main",
        BusKind::Aux => "aux",
        BusKind::Group => "grp",
        BusKind::Matrix => "mtx",
        BusKind::FxSend => "fx",
        BusKind::Other => "other",
    };
    format!("bus:{k}:{n}")
}

pub fn dca(n: u32) -> String {
    format!("dca:{n}")
}
pub fn mute_group(n: u32) -> String {
    format!("mg:{n}")
}
pub fn scene(n: u32) -> String {
    format!("scene:{n}")
}
pub fn scene_in_bank(bank: &str, n: u32) -> String {
    format!("scene:{}:{n}", bank.to_lowercase())
}
pub fn cue(n: u32) -> String {
    format!("cue:{n}")
}

/// The part after the last `:`, for display: `bus:aux:12` → `12`.
pub fn tail(id: &str) -> &str {
    id.rsplit_once(':').map(|(_, t)| t).unwrap_or(id)
}

/// The number at the end of an ID, when there is one.
pub fn number(id: &str) -> Option<u32> {
    tail(id).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings() {
        assert_eq!(socket("local", Direction::In, 3), "skt:local:in:3");
        assert_eq!(socket("unit:dante", Direction::Out, 64), "skt:dante:out:64");
        assert_eq!(bus(BusKind::Aux, 5), "bus:aux:5");
        assert_eq!(tail("bus:aux:12"), "12");
        assert_eq!(number("scene:a:7"), Some(7));
        assert_eq!(unit("unit:x"), "unit:x");
    }
}

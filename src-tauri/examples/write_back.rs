//! Write a show back into its vendor file from the terminal:
//!
//! ```text
//! cargo run --example write_back -- <vendor file or SQ folder> <out path> [edits…]
//!   edits:  ch:<n>=<name>      rename input channel n
//!           ch:<n>@<socket>    patch input n: <n> = Local n; local:<n> | slink:<n> | usb:<n> | dante:<n> | none
//!           scene:<n>=<name>   rename SQ scene n
//! ```
//!
//! Imports the file into a throwaway library, applies the edits, and writes the
//! result with the same code path as the app's Vendor files tab.

use std::path::Path;

use songbook_lib::import_for_cli;
use songbook_library::Library;
use songbook_model::{ids, ChannelKind, Direction};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: write_back <vendor file or SQ folder> <out path> [ch:<n>=<name> | ch:<n>@<socket> | scene:<n>=<name>]…");
        std::process::exit(2);
    }
    let tmp = std::env::temp_dir().join(format!("songbook-write-back-{}", std::process::id()));
    let lib = Library::open(&tmp).expect("library");
    let mut show = import_for_cli(&lib, Path::new(&args[0]), None).unwrap_or_else(|e| {
        eprintln!("import: {e}");
        std::process::exit(1)
    });
    for e in &args[2..] {
        if let Some(rest) = e.strip_prefix("ch:") {
            if let Some((n, name)) = rest.split_once('=') {
                let n: u32 = n.parse().expect("channel number");
                let c = show.channels.iter_mut().find(|c| c.kind == ChannelKind::Input && c.number == n).expect("no such channel");
                c.label = name.to_string();
            } else if let Some((n, sock)) = rest.split_once('@') {
                let n: u32 = n.parse().expect("channel number");
                let c = show.channels.iter_mut().find(|c| c.kind == ChannelKind::Input && c.number == n).expect("no such channel");
                c.source = match sock {
                    "none" => None,
                    s if s.contains(':') => {
                        let (unit, idx) = s.split_once(':').unwrap();
                        Some(ids::socket(unit, Direction::In, idx.parse().expect("socket number")))
                    }
                    s => Some(ids::socket("local", Direction::In, s.parse().expect("socket number"))),
                };
            }
        } else if let Some(rest) = e.strip_prefix("scene:") {
            let (n, name) = rest.split_once('=').expect("scene:<n>=<name>");
            let n: u32 = n.parse().expect("scene number");
            let sc = show.scenes.iter_mut().find(|s| s.number == Some(n)).expect("no such scene");
            sc.label = name.to_string();
        }
    }
    // Sockets the edits refer to must exist in the show for the writers to resolve them.
    for c in show.channels.clone() {
        if let Some(src) = &c.source {
            if show.socket(src).is_none() {
                // `skt:<unit>:in:<n>`
                let parts: Vec<&str> = src.split(':').collect();
                let (unit, index) = (parts.get(1).copied().unwrap_or("local").to_string(), ids::number(src).unwrap_or(0));
                let unit_id = ids::unit(&unit);
                if show.system.units.iter().all(|u| u.id != unit_id) {
                    show.system.units.push(songbook_model::build::unit(&unit, &unit.to_uppercase(), "", songbook_model::UnitRole::Console));
                }
                show.sockets.push(songbook_model::build::socket(&unit, Direction::In, index, songbook_model::SocketKind::Other, &format!("{} {}", unit.to_uppercase(), index)));
            }
        }
    }
    let blob = show.vendor.first().expect("the import kept no vendor file").clone();
    let bytes = lib.vendor_bytes(&show.id, &blob).expect("vendor bytes");
    let out = Path::new(&args[1]);
    let report = match blob.kind.as_str() {
        "sq-show" => {
            if args[1].to_lowercase().ends_with(".zip") {
                let (o, r) = songbook_ah::write::write_zip(&bytes, &show).expect("write");
                std::fs::write(out, o).expect("write out");
                serde_json::to_value(r).unwrap()
            } else {
                serde_json::to_value(songbook_ah::write::write_folder(&bytes, &show, out).expect("write")).unwrap()
            }
        }
        "clf" => {
            let (o, r) = songbook_yamaha::clf::write(&bytes, &show).expect("write");
            std::fs::write(out, o).expect("write out");
            serde_json::to_value(r).unwrap()
        }
        k => {
            eprintln!("cannot write a {k} file");
            std::process::exit(1)
        }
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    let _ = std::fs::remove_dir_all(&tmp);
}

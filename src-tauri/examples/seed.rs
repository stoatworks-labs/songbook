//! `cargo run --example seed -- <library dir> <file-or-dir>...` — import show
//! files into a library from the terminal, the same way the app's Import does.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: seed <library dir> <file-or-dir>...");
        std::process::exit(2);
    }
    let lib = songbook_library::Library::open(std::path::Path::new(&args[0])).expect("open library");
    for p in &args[1..] {
        match songbook_lib::import_for_cli(&lib, std::path::Path::new(p), None) {
            Ok(s) => println!("{p}: {} ({} channels, {} buses, {} scenes)", s.meta.name, s.channels.len(), s.buses.len(), s.scenes.len()),
            Err(e) => eprintln!("{p}: {e}"),
        }
    }
}

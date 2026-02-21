mod cli;
mod diagnostic;
mod format;
mod parser;

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();

    // Handle --version before parsing (it's not a terse flag)
    if args.iter().any(|a| a == "--version") {
        println!("cargo-terse {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match cli::parse_args(args) {
        Ok(cmd) => {
            eprintln!("{cmd:?}");
            eprintln!("cargo-terse: not yet implemented");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("cargo-terse: {e}");
            std::process::exit(2);
        }
    }
}

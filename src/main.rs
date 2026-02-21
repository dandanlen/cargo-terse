fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("cargo-terse {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("cargo-terse: not yet implemented");
    std::process::exit(1);
}

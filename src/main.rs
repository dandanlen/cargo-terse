mod cache;
mod cli;
mod diagnostic;
mod format;
mod parser;
mod runner;

fn print_help() {
    println!(
        "cargo-terse — concise cargo output for AI-assisted workflows

USAGE:
    cargo terse [OPTIONS] [COMMAND] [-- <CARGO_ARGS>...]

COMMANDS:
    check       Run cargo check (default)
    build       Run cargo build
    test        Run cargo test
    clippy      Run cargo clippy
    detail <ID> Show full diagnostic for cached ID

OPTIONS:
    --format <plain|json|toon>  Output format (default: plain)
    -v                          Verbose: include code span
    -vv                         Very verbose: full rustc output
    --no-cache                  Disable drill-down cache
    --version                   Print version
    -h, --help                  Print this help"
    );
}

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();

    if args
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "--version")
    {
        println!("cargo-terse {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match cli::parse_args(args) {
        Ok(cmd) => match cmd {
            cli::Command::Run {
                cargo_cmd,
                format,
                verbosity,
                no_cache,
                cargo_args,
            } => {
                std::process::exit(runner::run_cargo(
                    &cargo_cmd,
                    &cargo_args,
                    &format,
                    &verbosity,
                    no_cache,
                ));
            }
            cli::Command::Detail { id, format } => {
                let fmt = format::create_formatter(&format, &cli::Verbosity::VeryVerbose);
                if let Some(diag) = cache::lookup_diagnostic(&id) {
                    println!("{}", fmt.format_diagnostic(&diag));
                } else if let Some(test) = cache::lookup_test_result(&id) {
                    println!("{}", fmt.format_test_failure(&test));
                } else {
                    eprintln!("cargo-terse: no cached diagnostic with id '{id}'");
                    eprintln!("hint: run a cargo terse command first");
                    std::process::exit(1);
                }
            }
            cli::Command::Help => print_help(),
        },
        Err(e) => {
            eprintln!("cargo-terse: {e}");
            std::process::exit(2);
        }
    }
}

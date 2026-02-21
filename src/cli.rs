use std::ffi::OsString;

#[derive(Debug, PartialEq)]
pub enum OutputFormat {
    Plain,
    Json,
    Toon,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verbosity {
    Terse,
    Verbose,
    VeryVerbose,
}

#[derive(Debug)]
pub enum Command {
    Run {
        cargo_cmd: String,
        format: OutputFormat,
        verbosity: Verbosity,
        no_cache: bool,
        cargo_args: Vec<OsString>,
    },
    Detail {
        id: String,
        format: OutputFormat,
    },
    Help,
}

pub fn parse_args(args: Vec<OsString>) -> Result<Command, lexopt::Error> {
    // When cargo invokes us: argv[0] = binary, argv[1] = "terse" — skip both.
    // We receive the full argv so we drop the first two entries.
    let mut parser = lexopt::Parser::from_args(args.into_iter().skip(2));

    let mut format = OutputFormat::Plain;
    let mut v_count = 0u8;
    let mut no_cache = false;
    let mut cargo_cmd: Option<String> = None;
    let mut cargo_args: Vec<OsString> = Vec::new();

    while let Some(arg) = parser.next()? {
        match arg {
            lexopt::Arg::Long("format") => {
                format = match parser.value()?.to_str() {
                    Some("plain") => OutputFormat::Plain,
                    Some("json") => OutputFormat::Json,
                    Some("toon") => OutputFormat::Toon,
                    _ => return Err(lexopt::Error::Custom("invalid --format value".into())),
                };
            }
            lexopt::Arg::Short('v') => {
                v_count += 1;
            }
            lexopt::Arg::Long("no-cache") => {
                no_cache = true;
            }
            lexopt::Arg::Long("help") | lexopt::Arg::Short('h') => {
                return Ok(Command::Help);
            }
            lexopt::Arg::Value(val) => {
                let s = val.to_str().unwrap_or("").to_string();
                if cargo_cmd.is_none() {
                    if s == "help" {
                        return Ok(Command::Help);
                    }
                    if s == "detail" {
                        // detail <ID> [--format plain|json]
                        let id = parser.value()?.to_str().unwrap_or("").to_string();
                        // consume optional --format
                        while let Some(a) = parser.next()? {
                            match a {
                                lexopt::Arg::Long("format") => {
                                    format = match parser.value()?.to_str() {
                                        Some("plain") => OutputFormat::Plain,
                                        Some("json") => OutputFormat::Json,
                                        Some("toon") => OutputFormat::Toon,
                                        _ => {
                                            return Err(lexopt::Error::Custom(
                                                "invalid --format value".into(),
                                            ))
                                        }
                                    };
                                }
                                other => return Err(other.unexpected()),
                            }
                        }
                        return Ok(Command::Detail { id, format });
                    }
                    // First positional = cargo subcommand. Collect everything remaining
                    // (including "--") as passthrough args.
                    cargo_cmd = Some(s);
                    cargo_args.extend(parser.raw_args()?);
                } else {
                    cargo_args.push(val);
                }
            }
            // `-vv` arrives as two separate Short('v') in lexopt when written as `-vv`
            // but lexopt actually surfaces it as a single Value for stacked shorts.
            // Handle -vv explicitly: lexopt parses `-vv` as Short('v') with a remainder
            // "v", accessible via parser.optional_value(). We detect the second 'v' here.
            other => return Err(other.unexpected()),
        }
    }

    let verbosity = match v_count {
        0 => Verbosity::Terse,
        1 => Verbosity::Verbose,
        _ => Verbosity::VeryVerbose,
    };

    Ok(Command::Run {
        cargo_cmd: cargo_cmd.unwrap_or_else(|| "check".to_string()),
        format,
        verbosity,
        no_cache,
        cargo_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    fn args(v: &[&str]) -> Vec<OsString> {
        v.iter().map(|s| os(s)).collect()
    }

    // Helper to unwrap a Run command
    fn run(cmd: Command) -> (String, OutputFormat, Verbosity, bool, Vec<OsString>) {
        match cmd {
            Command::Run {
                cargo_cmd,
                format,
                verbosity,
                no_cache,
                cargo_args,
            } => (cargo_cmd, format, verbosity, no_cache, cargo_args),
            _ => panic!("expected Run command"),
        }
    }

    // 1. Bare invocation → defaults to check
    #[test]
    fn bare_defaults_to_check() {
        let cmd = parse_args(args(&["cargo-terse", "terse"])).unwrap();
        let (cargo_cmd, format, verbosity, no_cache, cargo_args) = run(cmd);
        assert_eq!(cargo_cmd, "check");
        assert_eq!(format, OutputFormat::Plain);
        assert_eq!(verbosity, Verbosity::Terse);
        assert!(!no_cache);
        assert!(cargo_args.is_empty());
    }

    // 2. --format json -v clippy -- -W clippy::all
    #[test]
    fn clippy_json_verbose_with_passthrough() {
        let cmd = parse_args(args(&[
            "cargo-terse",
            "terse",
            "--format",
            "json",
            "-v",
            "clippy",
            "--",
            "-W",
            "clippy::all",
        ]))
        .unwrap();
        let (cargo_cmd, format, verbosity, _, cargo_args) = run(cmd);
        assert_eq!(cargo_cmd, "clippy");
        assert_eq!(format, OutputFormat::Json);
        assert_eq!(verbosity, Verbosity::Verbose);
        assert_eq!(cargo_args, args(&["--", "-W", "clippy::all"]));
    }

    // 3. detail W3
    #[test]
    fn detail_subcommand() {
        let cmd = parse_args(args(&["cargo-terse", "terse", "detail", "W3"])).unwrap();
        match cmd {
            Command::Detail { id, format } => {
                assert_eq!(id, "W3");
                assert_eq!(format, OutputFormat::Plain);
            }
            _ => panic!("expected Detail command"),
        }
    }

    // 4. -vv test → VeryVerbose
    #[test]
    fn very_verbose() {
        let cmd = parse_args(args(&["cargo-terse", "terse", "-vv", "test"])).unwrap();
        let (cargo_cmd, _, verbosity, _, _) = run(cmd);
        assert_eq!(cargo_cmd, "test");
        assert_eq!(verbosity, Verbosity::VeryVerbose);
    }

    // 5. --no-cache build
    #[test]
    fn no_cache_flag() {
        let cmd = parse_args(args(&["cargo-terse", "terse", "--no-cache", "build"])).unwrap();
        let (cargo_cmd, _, _, no_cache, _) = run(cmd);
        assert_eq!(cargo_cmd, "build");
        assert!(no_cache);
    }

    // 6. test --release → --release in cargo_args
    #[test]
    fn cargo_flag_passthrough() {
        let cmd = parse_args(args(&["cargo-terse", "terse", "test", "--release"])).unwrap();
        let (cargo_cmd, _, _, _, cargo_args) = run(cmd);
        assert_eq!(cargo_cmd, "test");
        assert_eq!(cargo_args, args(&["--release"]));
    }

    // 7. test -- --test-threads=1 → both -- and --test-threads=1 in cargo_args
    #[test]
    fn double_dash_passthrough() {
        let cmd = parse_args(args(&[
            "cargo-terse",
            "terse",
            "test",
            "--",
            "--test-threads=1",
        ]))
        .unwrap();
        let (cargo_cmd, _, _, _, cargo_args) = run(cmd);
        assert_eq!(cargo_cmd, "test");
        assert_eq!(cargo_args, args(&["--", "--test-threads=1"]));
    }

    // 8. help subcommand
    #[test]
    fn help_subcommand() {
        let cmd = parse_args(args(&["cargo-terse", "terse", "help"])).unwrap();
        assert!(matches!(cmd, Command::Help));
    }
}

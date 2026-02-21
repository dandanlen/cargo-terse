use crate::diagnostic::{Diagnostic, DiagnosticLevel};

pub fn parse_cargo_json_line(
    line: &str,
    next_warning_id: &mut usize,
    next_error_id: &mut usize,
) -> Option<Diagnostic> {
    if !line.starts_with('{') {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v["reason"] != "compiler-message" {
        return None;
    }
    let msg = &v["message"];
    let level = match msg["level"].as_str()? {
        "error" => DiagnosticLevel::Error,
        "warning" => DiagnosticLevel::Warning,
        _ => return None,
    };
    let primary = msg["spans"]
        .as_array()?
        .iter()
        .find(|s| s["is_primary"] == true)?;

    let id = match level {
        DiagnosticLevel::Error => {
            *next_error_id += 1;
            format!("E{}", *next_error_id)
        }
        DiagnosticLevel::Warning => {
            *next_warning_id += 1;
            format!("W{}", *next_warning_id)
        }
        _ => return None,
    };

    Some(Diagnostic {
        id,
        level,
        code: msg["code"]["code"].as_str().map(str::to_owned),
        message: msg["message"].as_str()?.to_owned(),
        file: primary["file_name"].as_str().map(str::to_owned),
        line: primary["line_start"].as_u64().map(|n| n as usize),
        col: primary["column_start"].as_u64().map(|n| n as usize),
        span_text: primary["text"][0]["text"].as_str().map(str::to_owned),
        span_label: primary["label"].as_str().map(str::to_owned),
        rendered: msg["rendered"].as_str().map(str::to_owned),
        raw_json: Some(v),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_lines() -> Vec<String> {
        include_str!("../tests/fixtures/clippy_output.json")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn ignores_compiler_artifact() {
        let lines = fixture_lines();
        assert!(parse_cargo_json_line(&lines[0], &mut 0, &mut 0).is_none());
    }

    #[test]
    fn parses_warning() {
        let lines = fixture_lines();
        let d = parse_cargo_json_line(&lines[1], &mut 0, &mut 0).expect("expected Some");
        assert_eq!(d.id, "W1");
        assert_eq!(d.level, DiagnosticLevel::Warning);
        assert_eq!(d.code.as_deref(), Some("clippy::needless_return"));
        assert_eq!(d.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(d.line, Some(42));
        assert_eq!(d.col, Some(5));
        assert_eq!(d.message, "unnecessary `return` statement");
        assert!(d.rendered.is_some());
    }

    #[test]
    fn parses_error() {
        let lines = fixture_lines();
        let d = parse_cargo_json_line(&lines[2], &mut 0, &mut 0).expect("expected Some");
        assert_eq!(d.id, "E1");
        assert_eq!(d.level, DiagnosticLevel::Error);
        assert_eq!(d.code.as_deref(), Some("E0308"));
        assert_eq!(d.file.as_deref(), Some("src/handler.rs"));
        assert_eq!(d.line, Some(93));
        assert_eq!(d.col, Some(12));
    }

    #[test]
    fn increments_ids() {
        let lines = fixture_lines();
        let mut wid = 0usize;
        let mut eid = 0usize;
        let w = parse_cargo_json_line(&lines[1], &mut wid, &mut eid).expect("expected Some");
        assert_eq!(w.id, "W1");
        assert_eq!(wid, 1);
        assert_eq!(eid, 0);
        let e = parse_cargo_json_line(&lines[2], &mut wid, &mut eid).expect("expected Some");
        assert_eq!(e.id, "E1");
        assert_eq!(wid, 1);
        assert_eq!(eid, 1);
    }

    #[test]
    fn ignores_non_json() {
        assert!(parse_cargo_json_line("Compiling foo v0.1.0", &mut 0, &mut 0).is_none());
        assert!(parse_cargo_json_line("", &mut 0, &mut 0).is_none());
        assert!(parse_cargo_json_line("not json at all", &mut 0, &mut 0).is_none());
    }

    #[test]
    fn ignores_build_finished() {
        let lines = fixture_lines();
        assert!(parse_cargo_json_line(&lines[3], &mut 0, &mut 0).is_none());
    }
}

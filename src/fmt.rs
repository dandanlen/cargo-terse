pub struct FmtResult {
    pub files: Vec<String>, // filenames that need formatting
    pub full_diff: String,  // raw diff output
}

/// Parse `cargo fmt --check` stdout (unified diff with "Diff in ..." headers).
///
/// Each file block starts with a line like:
///   `Diff in /abs/path/to/file.rs:42:`
/// Lines until the next such header (or EOF) belong to that file's hunk.
pub fn parse_fmt_output(stdout: &str) -> FmtResult {
    if stdout.is_empty() {
        return FmtResult { files: vec![], full_diff: String::new() };
    }

    let mut files: Vec<String> = Vec::new();

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Diff in ") {
            // "Diff in /path/to/file.rs:42:"
            // Strip trailing colon then split off the line number suffix.
            let trimmed = rest.trim_end_matches(':');
            // Path is everything up to the last ':' (the line number).
            if let Some(colon_pos) = trimmed.rfind(':') {
                let path = trimmed[..colon_pos].trim().to_string();
                if !files.contains(&path) {
                    files.push(path);
                }
            } else {
                // No line number — treat the whole thing as the path.
                let path = trimmed.trim().to_string();
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
    }

    FmtResult { files, full_diff: stdout.to_string() }
}

/// Returns only the diff lines belonging to `file` (the hunk text, without the header).
fn diff_for_file<'a>(full_diff: &'a str, file: &str) -> &'a str {
    // Header lines look like: "Diff in /path/to/file.rs:42:"
    let file_prefix = format!("Diff in {file}:");
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;

    let mut pos = 0usize;
    for line in full_diff.lines() {
        let next_pos = pos + line.len() + 1; // +1 for '\n'
        if start.is_none() {
            if line.starts_with(&file_prefix) {
                // hunk body starts after this header line
                start = Some(next_pos.min(full_diff.len()));
            }
        } else {
            // We're inside the hunk. Stop at the next "Diff in " header for a different file.
            if line.starts_with("Diff in ") && !line.starts_with(&file_prefix) {
                end = Some(pos);
                break;
            }
        }
        pos = next_pos;
    }

    match (start, end) {
        (Some(s), Some(e)) => full_diff[s..e].trim_end_matches('\n'),
        (Some(s), None) => full_diff[s..].trim_end_matches('\n'),
        _ => "",
    }
}

/// Format the per-file diff lines for output.
///
/// `-v`: first hunk only (up to the next blank line or end).
/// `-vv`: full diff for the file.
pub fn format_file_diff(full_diff: &str, file: &str, first_hunk_only: bool) -> String {
    let hunk = diff_for_file(full_diff, file);
    if hunk.is_empty() {
        return String::new();
    }
    if !first_hunk_only {
        return hunk.to_string();
    }
    // First hunk: lines up to (but not including) the first blank line.
    hunk.lines()
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1. parse_empty_output — empty string → 0 files
    #[test]
    fn parse_empty_output() {
        let result = parse_fmt_output("");
        assert!(result.files.is_empty());
        assert!(result.full_diff.is_empty());
    }

    // 2. parse_diff_output — single diff → extracts filename
    #[test]
    fn parse_diff_output() {
        let input = "\
Diff in /home/user/project/src/main.rs:42:
-    old line
+    new line
";
        let result = parse_fmt_output(input);
        assert_eq!(result.files, vec!["/home/user/project/src/main.rs"]);
        assert_eq!(result.full_diff, input);
    }

    // 3. parse_multiple_files — multiple diffs → multiple filenames, deduplicated
    #[test]
    fn parse_multiple_files() {
        let input = "\
Diff in /project/src/main.rs:10:
-old
+new

Diff in /project/src/lib.rs:5:
-a
+b

Diff in /project/src/main.rs:20:
-second hunk
+replaced
";
        let result = parse_fmt_output(input);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0], "/project/src/main.rs");
        assert_eq!(result.files[1], "/project/src/lib.rs");
    }

    // 4. format_file_diff extracts the correct hunk for a file
    #[test]
    fn format_file_diff_extracts_hunk() {
        let input = "\
Diff in /project/src/main.rs:10:
-old main
+new main

Diff in /project/src/lib.rs:5:
-old lib
+new lib
";
        let hunk = format_file_diff(input, "/project/src/main.rs", false);
        assert!(hunk.contains("-old main"));
        assert!(!hunk.contains("-old lib"));
    }

    // 5. format_file_diff with first_hunk_only stops at blank line
    #[test]
    fn format_file_diff_first_hunk_only() {
        let input = "\
Diff in /project/src/main.rs:10:
-old
+new

-second hunk old
+second hunk new
";
        let hunk = format_file_diff(input, "/project/src/main.rs", true);
        assert!(hunk.contains("-old"));
        assert!(!hunk.contains("-second hunk old"));
    }
}

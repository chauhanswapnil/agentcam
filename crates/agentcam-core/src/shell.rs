//! Find the files a shell command writes or deletes.
//!
//! This is a best-effort reading of common patterns (`>`, `>>`, `tee`,
//! `sed -i`, `rm`, `mv`, `cp`, `touch`), not a shell. It never runs
//! anything. It skips heredoc bodies, paths with globs or variables, and
//! anything it can't read with confidence.

use std::path::Path;

use crate::model::FileAction;

/// Files `cmd` changes, resolved against `cwd` when relative.
pub fn command_touches(cmd: &str, cwd: Option<&str>) -> Vec<(String, FileAction)> {
    let mut out = Vec::new();
    let mut dir = cwd.map(String::from);
    for segment in segments(&strip_heredocs(cmd)) {
        let words = words(&segment);
        if words.is_empty() {
            continue;
        }
        if words[0] == "cd" {
            if let Some(target) = words.get(1) {
                dir = Some(resolve(dir.as_deref(), target));
            }
            continue;
        }
        let (args, writes) = redirections(&words);
        for w in writes {
            push(&mut out, dir.as_deref(), &w, FileAction::Edit);
        }
        command_args(&args, dir.as_deref(), &mut out);
    }
    out.dedup();
    out
}

/// Drop heredoc bodies (`<<EOF ... EOF`), keeping the command lines.
fn strip_heredocs(cmd: &str) -> String {
    let mut out = String::new();
    let mut end: Option<String> = None;
    for line in cmd.lines() {
        if let Some(tag) = &end {
            if line.trim() == tag {
                end = None;
            }
            continue;
        }
        if let Some(i) = line.find("<<") {
            let rest = line[i + 2..].trim_start_matches('-').trim_start();
            let tag: String = rest
                .trim_start_matches(['\'', '"'])
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !tag.is_empty() {
                end = Some(tag);
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Split on `&&`, `||`, `;`, `|` and newlines outside quotes.
fn segments(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            (Some(_), '\\') => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, '\\') => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (None, ';' | '\n') => out.push(std::mem::take(&mut cur)),
            (None, '&') if chars.peek() == Some(&'&') => {
                chars.next();
                out.push(std::mem::take(&mut cur));
            }
            (None, '|') => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            (None, c) => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

/// Shell words with quotes removed. Redirection operators become their
/// own words (`>file` -> `>`, `file`).
fn words(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = seg.chars().peekable();
    let flush = |cur: &mut String, started: &mut bool, out: &mut Vec<String>| {
        if *started {
            out.push(std::mem::take(cur));
            *started = false;
        }
    };
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            Some(_) => cur.push(c),
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                '\\' => {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                        started = true;
                    }
                }
                c if c.is_whitespace() => flush(&mut cur, &mut started, &mut out),
                '>' => {
                    // Keep an fd prefix (`2>`, `&>`) on the operator.
                    let fd = if started && (cur == "1" || cur == "2" || cur == "&") {
                        started = false;
                        std::mem::take(&mut cur)
                    } else {
                        flush(&mut cur, &mut started, &mut out);
                        String::new()
                    };
                    let mut op = format!("{fd}>");
                    if chars.peek() == Some(&'>') {
                        chars.next();
                        op.push('>');
                    }
                    if chars.peek() == Some(&'&') {
                        // `>&2`, `2>&1`: fd duplication, not a file.
                        chars.next();
                        op.push('&');
                    }
                    out.push(op);
                }
                c => {
                    cur.push(c);
                    started = true;
                }
            },
        }
    }
    flush(&mut cur, &mut started, &mut out);
    out
}

/// Pull out redirection targets that write files; return the other words.
fn redirections(words: &[String]) -> (Vec<String>, Vec<String>) {
    let mut args = Vec::new();
    let mut writes = Vec::new();
    let mut it = words.iter();
    while let Some(w) = it.next() {
        if w.ends_with('>') || w.ends_with(">>") {
            let target = it.next();
            // stdout and both-stream redirects write files; stderr to a file
            // is a log and `>&` is fd duplication.
            if !w.starts_with('2')
                && let Some(t) = target
                && !t.starts_with('&')
            {
                writes.push(t.clone());
            }
        } else if w.ends_with("&") && w.contains('>') {
            it.next();
        } else {
            args.push(w.clone());
        }
    }
    (args, writes)
}

fn command_args(words: &[String], cwd: Option<&str>, out: &mut Vec<(String, FileAction)>) {
    // Skip `VAR=value` prefixes, `sudo`, `env`.
    let mut i = 0;
    while i < words.len()
        && (words[i].contains('=') && !words[i].starts_with('-')
            || words[i] == "sudo"
            || words[i] == "env")
    {
        i += 1;
    }
    let Some(cmd) = words.get(i) else { return };
    let name = Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd);
    let args = &words[i + 1..];
    let operands = || {
        args.iter()
            .filter(|a| !a.starts_with('-'))
            .cloned()
            .collect::<Vec<_>>()
    };

    match name {
        "tee" => {
            for f in operands() {
                push(out, cwd, &f, FileAction::Edit);
            }
        }
        "touch" => {
            for f in operands() {
                push(out, cwd, &f, FileAction::Create);
            }
        }
        "rm" | "unlink" => {
            for f in operands() {
                push(out, cwd, &f, FileAction::Delete);
            }
        }
        "mv" => {
            let ops = operands();
            if let [srcs @ .., dst] = ops.as_slice()
                && !srcs.is_empty()
            {
                for s in srcs {
                    push(out, cwd, s, FileAction::Delete);
                }
                if srcs.len() == 1 {
                    push(out, cwd, dst, FileAction::Create);
                }
            }
        }
        "cp" => {
            let ops = operands();
            if let [srcs @ .., dst] = ops.as_slice()
                && srcs.len() == 1
            {
                push(out, cwd, dst, FileAction::Create);
            }
        }
        "sed" | "gsed" => sed_files(args, cwd, out),
        _ => {}
    }
}

/// `sed -i[suffix] [-e script]... [script] files...`
fn sed_files(args: &[String], cwd: Option<&str>, out: &mut Vec<(String, FileAction)>) {
    let mut in_place = false;
    let mut have_script = false;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-i" || a == "--in-place" {
            in_place = true;
            // BSD sed: `-i ''` or `-i .bak` takes a suffix word.
            if args
                .get(i + 1)
                .is_some_and(|n| n.is_empty() || n.starts_with('.'))
            {
                i += 1;
            }
        } else if a.starts_with("-i") || a.starts_with("--in-place=") {
            in_place = true;
        } else if a == "-e" || a == "-f" || a == "--expression" {
            have_script = true;
            i += 1;
        } else if a.starts_with('-') {
            if a.contains('i') && !a.starts_with("--") {
                in_place = true; // e.g. `-Ei`
            }
        } else if !have_script {
            have_script = true;
        } else {
            files.push(a.clone());
        }
        i += 1;
    }
    if in_place {
        for f in files {
            push(out, cwd, &f, FileAction::Edit);
        }
    }
}

fn push(out: &mut Vec<(String, FileAction)>, cwd: Option<&str>, path: &str, action: FileAction) {
    let unsure = path.is_empty()
        || path.starts_with("/dev/")
        || path.contains(['$', '*', '?', '`', '{', '(', '<'])
        || path.starts_with('~');
    if unsure {
        return;
    }
    out.push((resolve(cwd, path), action));
}

/// Join `path` onto `cwd` and fold `.` and `..` without touching the disk.
/// Shell paths use `/` on every OS, so this works on strings rather than
/// `std::path`, which would switch to `\` on Windows.
fn resolve(cwd: Option<&str>, path: &str) -> String {
    let joined = match cwd {
        Some(c) if !path.starts_with('/') => format!("{}/{path}", c.trim_end_matches('/')),
        _ => path.to_string(),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let body = parts.join("/");
    if joined.starts_with('/') {
        format!("/{body}")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FileAction::*;

    fn t(cmd: &str) -> Vec<(String, FileAction)> {
        command_touches(cmd, Some("/w"))
    }

    #[test]
    fn redirects() {
        assert_eq!(t("echo hi > out.txt"), [("/w/out.txt".into(), Edit)]);
        assert_eq!(t("echo hi >>log/a.log"), [("/w/log/a.log".into(), Edit)]);
        assert_eq!(t("cargo build 2>&1 | tail -3"), []);
        assert_eq!(t("ls 2>/dev/null; grep x f > /dev/null"), []);
        assert_eq!(t("make 2> err.log"), []);
        assert_eq!(t("echo x >&2"), []);
        assert_eq!(t("cmd &> all.log"), [("/w/all.log".into(), Edit)]);
    }

    #[test]
    fn heredocs() {
        let cmd = "cat > src/a.rs <<'EOF'\nfn main() { x > y; }\nrm -rf /\nEOF\necho done";
        assert_eq!(t(cmd), [("/w/src/a.rs".into(), Edit)]);
    }

    #[test]
    fn file_commands() {
        assert_eq!(
            t("rm -rf build dist"),
            [("/w/build".into(), Delete), ("/w/dist".into(), Delete)]
        );
        assert_eq!(
            t("mv a.txt b.txt"),
            [("/w/a.txt".into(), Delete), ("/w/b.txt".into(), Create)]
        );
        assert_eq!(
            t("cp -r tpl/x.md docs/x.md"),
            [("/w/docs/x.md".into(), Create)]
        );
        assert_eq!(t("touch .keep"), [("/w/.keep".into(), Create)]);
        assert_eq!(
            t("echo x | tee -a notes.md"),
            [("/w/notes.md".into(), Edit)]
        );
        assert_eq!(t("sudo rm /etc/hosts"), [("/etc/hosts".into(), Delete)]);
    }

    #[test]
    fn sed_in_place_only() {
        assert_eq!(
            t("sed -i '' 's/a/b/' CHANGELOG.md"),
            [("/w/CHANGELOG.md".into(), Edit)]
        );
        assert_eq!(
            t("sed -i.bak -e 's/a/b/' -e 's/c/d/' x.txt"),
            [("/w/x.txt".into(), Edit)]
        );
        assert_eq!(t("sed -n '80,100p' Orion/x.swift"), []);
        assert_eq!(
            t("sed 's/a/b/' in.txt > out.txt"),
            [("/w/out.txt".into(), Edit)]
        );
    }

    #[test]
    fn cd_changes_directory() {
        assert_eq!(t("cd web && rm -rf dist"), [("/w/web/dist".into(), Delete)]);
        assert_eq!(t("cd /tmp/x && touch ../y"), [("/tmp/y".into(), Create)]);
    }

    #[test]
    fn resolves_with_forward_slashes() {
        assert_eq!(resolve(Some("/a/b/"), "../c/./d"), "/a/c/d");
        assert_eq!(resolve(Some("/a"), "/x/y"), "/x/y");
        assert_eq!(resolve(None, "rel/x"), "rel/x");
    }

    #[test]
    fn skips_what_it_cannot_read() {
        assert_eq!(t("rm -f $OUT/*.o"), []);
        assert_eq!(t("echo hi > \"$FILE\""), []);
        assert_eq!(t("rm ~/x"), []);
        assert_eq!(t("FOO=1 git status && grep -rn 'a > b' src"), []);
    }
}

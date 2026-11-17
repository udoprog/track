//! Guard: every class a hand-written Sass file in `crates/frontend/style`
//! defines must be used by the frontend's Rust sources, so dead classes are
//! deleted instead of piling up. Run with `--nocapture` to see the counts.
//!
//! A class counts as used when it appears as a whole token (letters, digits,
//! `-` and `_`) anywhere in `crates/frontend/src`. Classes no frontend source
//! spells out are listed in `ALLOW`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Classes rendered without being spelled out in `crates/frontend/src`. An
/// entry ending in `-` allows every class with that prefix: `auth-` for
/// watchword's sign-in, account, users and Cloudflare Access markup, `trakt`
/// and `xem` for `.logo` modifiers chosen by `source.as_id()`, and `tone-`
/// for the numbering editor's `format!("tone-{}", ..)` range colours.
const ALLOW: &[&str] = &["auth-", "trakt", "xem", "tone-"];

fn allowed(class: &str) -> bool {
    ALLOW.iter().any(|a| match a.strip_suffix('-') {
        Some(_) => class.starts_with(a),
        None => class == *a,
    })
}

fn frontend() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../frontend")
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();

        if path.is_dir() {
            files(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Removes `//` and `/* */` comments and quoted strings.
fn strip(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';

                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }

                    last = c;
                }
            }
            '"' | '\'' => {
                for d in chars.by_ref() {
                    if d == c {
                        break;
                    }
                }

                out.push_str("''");
            }
            c => out.push(c),
        }
    }

    out
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Class names in the selectors of a Sass source: the text before each `{`
/// that is not an at-rule (`@include g.on-mobile` is a mixin, not a class).
fn classes(source: &str) -> BTreeSet<String> {
    let source = strip(source);
    let mut out = BTreeSet::new();
    let mut start = 0;

    for (i, c) in source.char_indices() {
        match c {
            ';' | '}' => start = i + 1,
            '{' => {
                let selector = source[start..i].trim();
                start = i + 1;

                // `#{...}` interpolation opens a brace inside a value.
                if selector.starts_with('@') || selector.ends_with('#') {
                    continue;
                }

                let bytes = selector.as_bytes();

                for (j, _) in selector.match_indices('.') {
                    let prev = j.checked_sub(1).map(|p| bytes[p] as char);

                    if prev.is_some_and(|p| p.is_ascii_digit()) {
                        continue;
                    }

                    let name: String = selector[j + 1..]
                        .chars()
                        .take_while(|&c| is_ident(c))
                        .collect();

                    if name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                        out.insert(name);
                    }
                }
            }
            _ => {}
        }
    }

    out
}

fn tokens(source: &str) -> BTreeSet<&str> {
    source
        .split(|c: char| !is_ident(c))
        .filter(|t| !t.is_empty())
        .collect()
}

#[test]
fn every_class_is_used() {
    let root = frontend();

    let mut styles = Vec::new();
    files(&root.join("style"), "scss", &mut styles);
    styles.retain(|p| {
        !p.file_name()
            .unwrap()
            .to_string_lossy()
            .contains("_generated")
    });

    let mut sources = Vec::new();
    files(&root.join("src"), "rs", &mut sources);

    let mut defined = BTreeMap::<String, Vec<String>>::new();

    for path in &styles {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        for class in classes(&fs::read_to_string(path).unwrap()) {
            defined.entry(class).or_default().push(name.clone());
        }
    }

    let texts: Vec<_> = sources
        .iter()
        .map(|p| fs::read_to_string(p).unwrap())
        .collect();
    let mut used = BTreeSet::new();
    let mut users = BTreeMap::<&str, usize>::new();

    for text in &texts {
        for token in tokens(text) {
            used.insert(token);

            if defined.contains_key(token) {
                *users.entry(token).or_default() += 1;
            }
        }
    }

    let dead: Vec<_> = defined
        .iter()
        .filter(|(class, _)| !used.contains(class.as_str()) && !allowed(class))
        .map(|(class, files)| format!("{class} ({})", files.join(", ")))
        .collect();

    let single = users.values().filter(|&&n| n == 1).count();
    println!(
        "css classes: {} defined, {} used by one .rs file, {} dead",
        defined.len(),
        single,
        dead.len()
    );

    assert!(
        dead.is_empty(),
        "Sass classes no Rust source uses; delete them, or list them in ALLOW if markup outside crates/frontend/src renders them:\n{}",
        dead.join("\n")
    );
}

#[test]
fn parses_selectors() {
    let found = classes(
        "@use 'vars' as v;\n.a, a.b > .c-d { &.e { x: v.$gap; } @include g.on-mobile { &.f { y: 0.5em; } } }\n// .g\n/* .h */\n@keyframes k { 50% { opacity: .5; } }\n.i { background: url('x.png'); }",
    );

    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        ["a", "b", "c-d", "e", "f", "i"]
    );
}

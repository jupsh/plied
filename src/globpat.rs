//! Shell-style patterns: `fnmatch` (where `*` also crosses `/`), `glob` with recursive `**` (hidden
//! names skipped unless the pattern names them), and `glob_match` for allow and deny lists.

use std::path::Path;

enum Tok {
    Lit(char),
    One,
    Star,
    Set(bool, Vec<(char, char)>),
}

fn compile(pat: &str) -> Vec<Tok> {
    let p: Vec<char> = pat.chars().collect();
    let (n, mut i, mut out) = (p.len(), 0, vec![]);
    while i < n {
        let c = p[i];
        i += 1;
        match c {
            '*' => {
                if !matches!(out.last(), Some(Tok::Star)) {
                    out.push(Tok::Star);
                }
            }
            '?' => out.push(Tok::One),
            '[' => {
                let mut j = i;
                if j < n && p[j] == '!' {
                    j += 1;
                }
                if j < n && p[j] == ']' {
                    j += 1;
                }
                while j < n && p[j] != ']' {
                    j += 1;
                }
                if j >= n {
                    out.push(Tok::Lit('['));
                    continue;
                }
                let mut stuff = &p[i..j];
                i = j + 1;
                let neg = stuff.first() == Some(&'!');
                if neg {
                    stuff = &stuff[1..];
                }
                let mut ranges = vec![];
                let mut k = 0;
                while k < stuff.len() {
                    if k + 2 < stuff.len() && stuff[k + 1] == '-' {
                        if stuff[k] <= stuff[k + 2] {
                            ranges.push((stuff[k], stuff[k + 2]));
                        }
                        k += 3;
                    } else {
                        ranges.push((stuff[k], stuff[k]));
                        k += 1;
                    }
                }
                out.push(Tok::Set(neg, ranges));
            }
            c => out.push(Tok::Lit(c)),
        }
    }
    out
}

fn one(t: &Tok, c: char) -> bool {
    match t {
        Tok::Lit(l) => *l == c,
        Tok::One => true,
        Tok::Set(neg, rs) => rs.iter().any(|&(a, b)| a <= c && c <= b) != *neg,
        Tok::Star => false,
    }
}

/// `fnmatch.fnmatchcase`
pub fn fnmatch(name: &str, pat: &str) -> bool {
    let t = compile(pat);
    let s: Vec<char> = name.chars().collect();
    let (mut ti, mut si, mut star): (usize, usize, Option<(usize, usize)>) = (0, 0, None);
    while si < s.len() {
        if ti < t.len() && !matches!(t[ti], Tok::Star) && one(&t[ti], s[si]) {
            ti += 1;
            si += 1;
        } else if ti < t.len() && matches!(t[ti], Tok::Star) {
            star = Some((ti, si));
            ti += 1;
        } else if let Some((st, ss)) = star {
            ti = st + 1;
            si = ss + 1;
            star = Some((st, ss + 1));
        } else {
            return false;
        }
    }
    while ti < t.len() && matches!(t[ti], Tok::Star) {
        ti += 1;
    }
    ti == t.len()
}

/// fnmatch with `**` spanning directories.
pub fn glob_match(relpath: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pat| {
        fnmatch(relpath, pat)
            || (pat.starts_with("**/") && fnmatch(relpath, &pat[3..]))
            || (pat.contains("/**/") && fnmatch(relpath, &pat.replace("/**/", "/")))
    })
}

fn has_magic(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

fn name_match(name: &str, pat: &str) -> bool {
    if cfg!(windows) { fnmatch(&name.to_lowercase(), &pat.to_lowercase()) } else { fnmatch(name, pat) }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") }
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => vec![],
    };
    out.sort();
    out
}

const MAX_DEPTH: usize = 64; // symlinked directories can loop

/// `glob.glob(pattern, root_dir=root, recursive=True)`: names relative to `root`, joined with `/`.
pub fn glob(root: &Path, pattern: &str) -> Vec<String> {
    let comps: Vec<&str> = pattern.split(|c| c == '/' || (cfg!(windows) && c == '\\')).filter(|c| !c.is_empty()).collect();
    let mut out = vec![];
    rec(root, "", &comps, &mut out, 0);
    out
}

fn rec(root: &Path, prefix: &str, comps: &[&str], out: &mut Vec<String>, depth: usize) {
    let Some((c, rest)) = comps.split_first() else {
        if !prefix.is_empty() {
            out.push(prefix.to_string());
        }
        return;
    };
    if depth > MAX_DEPTH {
        return;
    }
    if *c == "**" {
        if !rest.is_empty() {
            rec(root, prefix, rest, out, depth + 1);
        }
        descend(root, prefix, rest, out, depth + 1);
    } else if has_magic(c) {
        for name in names(&root.join(prefix)) {
            if name.starts_with('.') && !c.starts_with('.') {
                continue;
            }
            if name_match(&name, c) {
                let p = join(prefix, &name);
                if rest.is_empty() {
                    out.push(p);
                } else if root.join(&p).is_dir() {
                    rec(root, &p, rest, out, depth + 1);
                }
            }
        }
    } else {
        let p = join(prefix, c);
        if rest.is_empty() {
            if root.join(&p).symlink_metadata().is_ok() {
                out.push(p);
            }
        } else {
            rec(root, &p, rest, out, depth + 1);
        }
    }
}

fn descend(root: &Path, prefix: &str, rest: &[&str], out: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for name in names(&root.join(prefix)) {
        if name.starts_with('.') {
            continue;
        }
        let p = join(prefix, &name);
        let is_dir = root.join(&p).is_dir();
        if rest.is_empty() {
            out.push(p.clone());
        } else if is_dir {
            rec(root, &p, rest, out, depth + 1);
        }
        if is_dir {
            descend(root, &p, rest, out, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_like_python() {
        assert!(fnmatch("plied/threads/T001.md", "plied/*.md"));
        assert!(fnmatch("a.md", "*.md"));
        assert!(fnmatch("x1", "x[0-9]"));
        assert!(!fnmatch("x1", "x[!0-9]"));
        assert!(fnmatch("[", "["));
        assert!(!fnmatch("abc", "ab"));
        assert!(glob_match("findings/F001.md", &["**/F*.md".into()]));
        assert!(glob_match("logs/a/b", &["logs/**".into()]));
    }
}

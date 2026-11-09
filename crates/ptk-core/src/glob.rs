//! A small glob matcher for project-relative paths (`/` separators).
//!
//! - `*` matches anything except `/`, `?` matches one character except `/`.
//! - `**` matches across folders; `**/` may also match no folder at all.
//! - A pattern without `/` matches the file name in any folder (like
//!   `.gitignore`), and a pattern ending in `/` matches everything below it.

/// True if `path` matches `pattern`.
pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    if let Some(dir) = pattern.strip_suffix('/') {
        let full = format!("{}/**", dir.trim_start_matches('/'));
        return matches(full.as_bytes(), path.as_bytes());
    }
    if !pattern.contains('/') {
        let name = path.rsplit('/').next().unwrap_or(path);
        return matches(pattern.as_bytes(), name.as_bytes());
    }
    matches(pattern.trim_start_matches('/').as_bytes(), path.as_bytes())
}

fn matches(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') if p.get(1) == Some(&b'*') => {
            let rest = &p[2..];
            if let Some(after_slash) = rest.strip_prefix(b"/") {
                if matches(after_slash, s) {
                    return true;
                }
                return s
                    .iter()
                    .enumerate()
                    .any(|(i, &c)| c == b'/' && matches(after_slash, &s[i + 1..]));
            }
            (0..=s.len()).any(|i| matches(rest, &s[i..]))
        }
        Some(b'*') => {
            let rest = &p[1..];
            for i in 0..=s.len() {
                if matches(rest, &s[i..]) {
                    return true;
                }
                if i < s.len() && s[i] == b'/' {
                    break;
                }
            }
            false
        }
        Some(b'?') => !s.is_empty() && s[0] != b'/' && matches(&p[1..], &s[1..]),
        Some(&c) => !s.is_empty() && s[0] == c && matches(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn globs() {
        assert!(glob_match("*.rpf", "extra/x64a.rpf"));
        assert!(glob_match("extra/*.png", "extra/a.png"));
        assert!(!glob_match("extra/*.png", "extra/sub/a.png"));
        assert!(glob_match("extra/**/*.png", "extra/a.png"));
        assert!(glob_match("extra/**/*.png", "extra/sub/deep/a.png"));
        assert!(glob_match("extra/vendor/", "extra/vendor/a/b.bin"));
        assert!(!glob_match("extra/vendor/", "extra/vendor2/a"));
        assert!(glob_match("patches/??.diff", "patches/01.diff"));
        assert!(!glob_match("", "anything"));
        assert!(glob_match("**", "a/b/c"));
    }
}

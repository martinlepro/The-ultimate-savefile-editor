//! Détection automatique des sauvegardes : motifs `~`, `%VAR%`, `*` et `{d}` (nom du dossier parent).
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub size: u64,
}

fn expand_env(p: &str) -> Option<String> {
    let mut s = p.replace('\\', "/");
    if let Some(r) = s.strip_prefix('~') {
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok()?;
        s = format!("{}{}", home.replace('\\', "/"), r);
    }
    while let Some(a) = s.find('%') {
        let b = s[a + 1..].find('%')? + a + 1;
        let v = std::env::var(&s[a + 1..b]).ok()?;
        s = format!("{}{}{}", &s[..a], v.replace('\\', "/"), &s[b + 1..]);
    }
    Some(s)
}

fn wild(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', r)) => (0..=s.len()).any(|i| wild(r, &s[i..])),
        Some((c, r)) => s.first().map_or(false, |x| x.eq_ignore_ascii_case(c)) && wild(r, &s[1..]),
    }
}

fn walk(cur: PathBuf, segs: &[&str], out: &mut Vec<PathBuf>) {
    let Some((seg, rest)) = segs.split_first() else {
        if cur.is_file() {
            out.push(cur);
        }
        return;
    };
    if *seg == "{d}" {
        if let Some(n) = cur.file_name() {
            walk(cur.join(n), rest, out);
        }
    } else if seg.contains('*') {
        if let Ok(rd) = fs::read_dir(&cur) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if wild(seg.as_bytes(), n.as_bytes()) {
                    walk(cur.join(&n), rest, out);
                }
            }
        }
    } else {
        walk(cur.join(seg), rest, out);
    }
}

pub fn scan(patterns: &[String]) -> Vec<Found> {
    let mut paths = vec![];
    for pat in patterns {
        let Some(s) = expand_env(pat) else { continue };
        let mut segs: Vec<&str> = s.split('/').filter(|x| !x.is_empty()).collect();
        let base = if s.starts_with('/') {
            PathBuf::from("/")
        } else if segs.first().map_or(false, |f| f.ends_with(':')) {
            PathBuf::from(format!("{}/", segs.remove(0)))
        } else {
            PathBuf::new()
        };
        walk(base, &segs, &mut paths);
    }
    paths.sort();
    paths.dedup();
    let mut v: Vec<Found> = paths
        .into_iter()
        .filter_map(|p| {
            let m = fs::metadata(&p).ok()?;
            Some(Found { path: p, modified: m.modified().ok(), size: m.len() })
        })
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn wildcard() {
        assert!(super::wild(b"user*.dat", b"user1.dat"));
        assert!(super::wild(b"*.celeste", b"0.celeste"));
        assert!(!super::wild(b"*.celeste", b"0.celeste.bak"));
    }
}

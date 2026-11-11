//! Détection automatique des sauvegardes : motifs `~`, `%VAR%`, `*` et `{d}` (nom du dossier parent).
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Found {
    pub path: PathBuf,
    /// dossier fixe du motif (avant le premier joker) ; `rel` = chemin sous ce dossier, en `/` : identique sur tous les ordinateurs
    pub root: PathBuf,
    pub rel: String,
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

/// Début fixe d'un motif développé : (dossier de départ, segments, indice du premier segment à joker).
fn split_pattern(pat: &str) -> Option<(PathBuf, Vec<String>, usize)> {
    let s = expand_env(pat)?;
    let mut segs: Vec<String> = s.split('/').filter(|x| !x.is_empty()).map(|x| x.to_string()).collect();
    let base = if s.starts_with('/') {
        PathBuf::from("/")
    } else if segs.first().map_or(false, |f| f.ends_with(':')) {
        PathBuf::from(format!("{}/", segs.remove(0)))
    } else {
        PathBuf::new()
    };
    // sans joker, le « dossier fixe » est le dossier du fichier
    let k = segs.iter().position(|x| x.contains('*') || x == "{d}").unwrap_or(segs.len().saturating_sub(1));
    Some((base, segs, k))
}

fn root_of(base: &PathBuf, segs: &[String], k: usize) -> PathBuf {
    let mut r = base.clone();
    for s in &segs[..k] {
        r.push(s);
    }
    r
}

pub fn scan(patterns: &[String]) -> Vec<Found> {
    let mut seen = std::collections::HashSet::new();
    let mut v: Vec<Found> = vec![];
    for pat in patterns {
        let Some((base, segs, k)) = split_pattern(pat) else { continue };
        let root = root_of(&base, &segs, k);
        let refs: Vec<&str> = segs.iter().map(|x| x.as_str()).collect();
        let mut paths = vec![];
        walk(base.clone(), &refs, &mut paths);
        paths.sort();
        for p in paths {
            if !seen.insert(p.clone()) {
                continue;
            }
            let Ok(m) = fs::metadata(&p) else { continue };
            let rel = p.strip_prefix(&root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            v.push(Found { path: p, root: root.clone(), rel, modified: m.modified().ok(), size: m.len() });
        }
    }
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

/// Un chemin relatif d'ailleurs (`rel`) n'est accepté que s'il suit la forme d'un des motifs du jeu :
/// retourne l'emplacement local correspondant (qui peut ne pas exister encore, mais dont le dossier fixe existe).
/// `mirror` : nom du fichier compagnon, accepté à côté d'une sauvegarde.
pub fn locate(patterns: &[String], rel: &str, mirror: Option<&str>) -> Option<PathBuf> {
    if rel.is_empty() || rel.starts_with('/') || rel.contains('\\') || rel.contains(':') || rel.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return None;
    }
    let rs: Vec<&str> = rel.split('/').collect();
    for pat in patterns {
        let Some((base, segs, k)) = split_pattern(pat) else { continue };
        let tail = &segs[k..];
        if tail.len() != rs.len() {
            continue;
        }
        let root = root_of(&base, &segs, k);
        if !root.is_dir() {
            continue;
        }
        let last = rs.len() - 1;
        let ok = tail.iter().enumerate().all(|(i, seg)| {
            if i == last && mirror.map_or(false, |m| m.eq_ignore_ascii_case(rs[i])) {
                return true; // fichier compagnon à côté de la sauvegarde
            }
            if seg == "{d}" {
                i > 0 && rs[i] == rs[i - 1]
            } else if seg.contains('*') {
                wild(seg.as_bytes(), rs[i].as_bytes())
            } else {
                seg.eq_ignore_ascii_case(rs[i])
            }
        });
        if ok {
            return Some(rel.split('/').fold(root, |p, s| p.join(s)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ss-detect-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn scan_gives_machine_independent_rel_paths() {
        let r = tmp("scan");
        fs::create_dir_all(r.join("Saves/Farm_1")).unwrap();
        fs::write(r.join("Saves/Farm_1/Farm_1"), "x").unwrap();
        fs::write(r.join("Saves/0.celeste"), "y").unwrap();
        let root = r.to_string_lossy().replace('\\', "/");
        let a = scan(&[format!("{root}/Saves/*/{{d}}")]);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].rel, "Farm_1/Farm_1");
        assert_eq!(a[0].root, r.join("Saves"));
        let b = scan(&[format!("{root}/Saves/*.celeste")]);
        assert_eq!(b[0].rel, "0.celeste");
        let c = scan(&[format!("{root}/Saves/0.celeste")]);
        assert_eq!(c[0].rel, "0.celeste");
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn locate_follows_the_pattern_and_refuses_the_rest() {
        let r = tmp("loc");
        fs::create_dir_all(r.join("Saves")).unwrap();
        let root = r.to_string_lossy().replace('\\', "/");
        let pats = vec![format!("{root}/Saves/*/{{d}}")];
        assert_eq!(locate(&pats, "Farm_9/Farm_9", None), Some(r.join("Saves/Farm_9/Farm_9")));
        assert_eq!(locate(&pats, "Farm_9/Other", None), None, "{{d}} = nom du dossier parent");
        assert_eq!(locate(&pats, "Farm_9/SaveGameInfo", Some("SaveGameInfo")), Some(r.join("Saves/Farm_9/SaveGameInfo")));
        for bad in ["../x/x", "/etc/passwd", "a/../a", "a\\a", "C:/a", "", "a//a", "a/a/a"] {
            assert_eq!(locate(&pats, bad, None), None, "{bad}");
        }
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn wildcard() {
        assert!(super::wild(b"user*.dat", b"user1.dat"));
        assert!(super::wild(b"*.celeste", b"0.celeste"));
        assert!(!super::wild(b"*.celeste", b"0.celeste.bak"));
    }
}

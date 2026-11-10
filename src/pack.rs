//! Pack de jeux : tout ce qui décrit les jeux (définitions, emplacements, thèmes, icônes, polices) vit dans le dépôt
//! « The-ultimate-savefile-editor-games ». L'application le télécharge, vérifie les empreintes SHA-256, l'installe
//! dans son dossier de données puis le relit d'ici.
use crate::def::{self, Game, Store};
use crate::theme::Theme;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Dépôt des éditeurs de sauvegardes de jeux (branche principale).
pub const REPO_RAW: &str = "https://raw.githubusercontent.com/martinlepro/The-ultimate-savefile-editor-games/main";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "linux"
    }
}

/// Dossier de données de l'application (réglages + pack téléchargé). `SAVESTUDIO_HOME` le remplace.
pub fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SAVESTUDIO_HOME") {
        return PathBuf::from(p);
    }
    if cfg!(target_os = "android") {
        return PathBuf::from("/storage/emulated/0/SaveStudio");
    }
    if cfg!(target_os = "windows") {
        if let Ok(a) = std::env::var("APPDATA") {
            return PathBuf::from(a).join("SaveStudio");
        }
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("SaveStudio");
        }
    }
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/SaveStudio")
}

pub fn pack_dir() -> PathBuf {
    data_dir().join("pack")
}

pub struct Manifest {
    pub version: String,
    pub min_app: String,
    pub updated: String,
    /// chemin relatif → SHA-256 (hexadécimal minuscule)
    pub files: BTreeMap<String, String>,
}

/// Un chemin du manifeste ne doit jamais sortir du dossier du pack.
pub fn safe_rel(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('/') && !p.contains('\\') && !p.contains(':') && !p.split('/').any(|s| s == ".." || s.is_empty())
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("manifest.json illisible : {e}"))?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let version = s("version");
    if version.is_empty() {
        return Err("manifest.json sans « version »".into());
    }
    let mut files = BTreeMap::new();
    if let Some(o) = v.get("files").and_then(|x| x.as_object()) {
        for (k, h) in o {
            let h = h.as_str().unwrap_or("").to_ascii_lowercase();
            if !safe_rel(k) {
                return Err(format!("chemin refusé dans le manifeste : {k}"));
            }
            if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("empreinte invalide pour {k}"));
            }
            files.insert(k.clone(), h);
        }
    }
    Ok(Manifest { version, min_app: s("min_app"), updated: s("updated"), files })
}

fn parts(v: &str) -> Vec<u64> {
    v.split('.').map(|x| x.trim().parse().unwrap_or(0)).collect()
}

/// Compare deux numéros de version (1.2.3 < 1.10.0).
pub fn cmp_version(a: &str, b: &str) -> Ordering {
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        match a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0)) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

pub struct FontSpec {
    pub name: String,
    pub regular: PathBuf,
    pub bold: Option<PathBuf>,
    pub snap: f32,
}

pub struct Pack {
    pub version: String,
    pub updated: String,
    pub games: Vec<Game>,
    pub fonts: Vec<FontSpec>,
    pub themes: Vec<(String, Theme)>,
    pub icons: Option<String>,
}

/// Lit `locations.svl` : lignes `<id>|<motif>`, `#` = commentaire.
pub fn parse_locations(src: &str) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in src.lines() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some((id, pat)) = l.split_once('|') {
            m.entry(id.trim().to_string()).or_default().push(pat.trim().to_string());
        }
    }
    m
}

pub fn load(dir: &Path) -> Result<Pack, String> {
    let mbytes = std::fs::read(dir.join("manifest.json")).map_err(|e| format!("manifest.json introuvable dans {} : {e}", dir.display()))?;
    let man = parse_manifest(&mbytes)?;
    let v: Value = serde_json::from_slice(&mbytes).map_err(|e| e.to_string())?;
    let read = |rel: &str| -> Result<String, String> {
        if !safe_rel(rel) {
            return Err(format!("chemin refusé : {rel}"));
        }
        std::fs::read_to_string(dir.join(rel)).map_err(|e| format!("{rel} : {e}"))
    };

    // emplacements des sauvegardes (uniquement ceux de la plateforme courante)
    let locs = match v.get("locations").and_then(|l| l.get(platform())).and_then(|x| x.as_str()) {
        Some(rel) => parse_locations(&read(rel)?),
        None => BTreeMap::new(),
    };

    // thèmes : les clés absentes reprennent celles de « wood »
    let mut themes: Vec<(String, Theme)> = vec![];
    if let Some(rel) = v.get("themes").and_then(|x| x.as_str()) {
        let tv: Value = serde_json::from_str(&read(rel)?).map_err(|e| format!("{rel} : {e}"))?;
        if let Some(o) = tv.as_object() {
            let wood = Theme::from_json(&Theme::builtin("wood"), o.get("wood").unwrap_or(&Value::Null));
            for (name, t) in o {
                themes.push((name.clone(), Theme::from_json(&wood, t)));
            }
        }
    }

    let icons = match v.get("icons").and_then(|x| x.as_str()) {
        Some(rel) => read(rel).ok(),
        None => None,
    };

    let mut fonts = vec![];
    if let Some(o) = v.get("fonts").and_then(|x| x.as_object()) {
        for (name, f) in o {
            let Some(reg) = f.get("regular").and_then(|x| x.as_str()) else { continue };
            if !safe_rel(reg) {
                continue;
            }
            let bold = f.get("bold").and_then(|x| x.as_str()).filter(|b| safe_rel(b)).map(|b| dir.join(b));
            fonts.push(FontSpec { name: name.clone(), regular: dir.join(reg), bold, snap: f.get("snap").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 });
        }
    }

    // jeux, dans l'ordre du manifeste
    let mut games = vec![];
    if let Some(o) = v.get("games").and_then(|x| x.as_object()) {
        for (id, e) in o {
            let Some(file) = e.get("file").and_then(|x| x.as_str()) else { continue };
            match read(file).and_then(|s| def::parse(&s)) {
                Ok(mut g) => {
                    if let Some(p) = locs.get(id) {
                        g.patterns.extend(p.iter().cloned());
                    }
                    if let Some(a) = e.get("stores").and_then(|x| x.as_array()) {
                        for s in a {
                            let t = |k: &str| s.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                            g.stores.push(Store { name: t("name"), version: t("version"), url: t("url") });
                        }
                    }
                    games.push(g);
                }
                Err(err) => eprintln!("jeu « {id} » ignoré : {err}"),
            }
        }
    }
    Ok(Pack { version: man.version, updated: man.updated, games, fonts, themes, icons })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(cmp_version("1.2.3", "1.10.0"), Ordering::Less);
        assert_eq!(cmp_version("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(cmp_version("2.0.0", "1.99.99"), Ordering::Greater);
    }

    #[test]
    fn unsafe_paths_are_refused() {
        for p in ["../x", "/etc/passwd", "a/../b", "C:/x", "a\\b", "", "a//b"] {
            assert!(!safe_rel(p), "{p}");
        }
        assert!(safe_rel("games/celeste.sve"));
    }

    #[test]
    fn manifest_parsing() {
        let h = "a".repeat(64);
        let m = parse_manifest(format!(r#"{{"version":"1.2.3","min_app":"0.3.0","files":{{"x/y.txt":"{h}"}}}}"#).as_bytes()).unwrap();
        assert_eq!(m.version, "1.2.3");
        assert_eq!(m.files.len(), 1);
        assert!(parse_manifest(br#"{"version":"1","files":{"../x":"00"}}"#).is_err());
        assert!(parse_manifest(br#"{"files":{}}"#).is_err());
    }

    #[test]
    fn locations_lines() {
        let m = parse_locations("# c\n\nceleste|~/a/*.celeste\nceleste|/b\nminecraft|/c\n");
        assert_eq!(m["celeste"].len(), 2);
        assert_eq!(m["minecraft"], vec!["/c".to_string()]);
    }
}

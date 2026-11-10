//! Lecteur du format compact `.sve` : tout tient sur une seule ligne, sans retour chariot.
//!
//!   @id|Nom|Court|police|codec|thème|l1=..|l2=..|ver=chemin|mirror=fichier:src->dst;   en-tête du jeu
//!   (anciens motifs de détection `win:` / `lin:` / `and:` encore acceptés ; les emplacements viennent de `*/locations.svl`)
//!   #Catégorie|icône;                                         onglet
//!   +Sous-catégorie|icône|c=2;                                carte (c = colonnes)
//!   n|Libellé|chemin|min=0|max=9|u=G|sc=1|dp=0|btn=Etiq:val|bh=Titre;   champ
//!
//! Types de champ : n nombre · t texte · b interrupteur · e liste déroulante (o=Etiq:val,…)
//! s curseur · l tableau (n=,v=) · g grille d'interrupteurs (r=1-40,names=…) · i info.
//! `\` échappe `; | \`. Les espaces et retours à la ligne entre enregistrements sont ignorés,
//! on peut donc écrire une version « lisible » puis la compacter sans rien changer au sens.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Num,
    Text,
    Bool,
    Enum,
    Slider,
    List,
    Grid,
    Info,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub kind: Kind,
    pub label: String,
    pub path: String,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub scale: f64,
    pub dp: usize,
    pub step: f64,
    pub btns: Vec<(String, String)>,
    pub bh: String,
    pub opts: Vec<(String, String)>,
    pub name_path: String,
    pub val_path: String,
    pub range: (i32, i32),
    pub names: Vec<String>,
    pub xp: String,
    pub xt: Vec<f64>,
    pub ro: bool,
    pub off: f64,
    pub also: Vec<(String, String)>,
    /// `sync=chemin` : à chaque modification, la même valeur est aussi écrite à `chemin` (bool = `:#`, indice de l'option choisie).
    pub sync: Vec<(String, bool)>,
}

#[derive(Clone, Debug, Default)]
pub struct Sub {
    pub name: String,
    pub icon: String,
    pub cols: usize,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Default)]
pub struct Cat {
    pub name: String,
    pub icon: String,
    pub subs: Vec<Sub>,
}

#[derive(Clone, Debug, Default)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub short: String,
    pub font: String,
    pub codec: String,
    pub theme: String,
    pub patterns: Vec<String>,
    pub l1: String,
    pub l2: String,
    /// `ver=chemin` : où lire, dans la sauvegarde, la version du jeu qui l'a écrite.
    pub ver: String,
    /// `mirror=fichier:préfixe->préfixe` : fichier compagnon recopié à chaque modification.
    pub mirror: Option<Mirror>,
    /// Versions des boutiques (venues de `manifest.json`).
    pub stores: Vec<Store>,
    pub cats: Vec<Cat>,
}

#[derive(Clone, Debug, Default)]
pub struct Mirror {
    pub file: String,
    pub src: String,
    pub dst: String,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct Store {
    pub name: String,
    pub version: String,
    pub url: String,
}

fn split_esc(s: &str, sep: char) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                cur.push('\\');
                cur.push(n);
            }
        } else if c == sep {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn unesc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                o.push(n);
            }
        } else {
            o.push(c);
        }
    }
    o
}

fn num(s: &str) -> Result<f64, String> {
    s.trim().parse().map_err(|_| format!("nombre invalide « {s} »"))
}

fn os_tag() -> &'static str {
    if cfg!(target_os = "windows") {
        "win"
    } else if cfg!(target_os = "android") {
        "and"
    } else {
        "lin"
    }
}

pub fn parse(src: &str) -> Result<Game, String> {
    let mut g = Game::default();
    for (n, rec) in split_esc(src, ';').iter().enumerate() {
        let rec = rec.trim();
        if rec.is_empty() {
            continue;
        }
        let ctx = |e: String| format!("enregistrement {} (« {} ») : {e}", n + 1, rec.chars().take(40).collect::<String>());
        let parts = split_esc(rec, '|');
        let p0 = parts[0].trim();
        if let Some(id) = p0.strip_prefix('@') {
            g.id = unesc(id);
            let get = |i: usize| parts.get(i).map(|s| unesc(s.trim())).unwrap_or_default();
            g.name = get(1);
            g.short = get(2);
            g.font = get(3);
            g.codec = get(4);
            g.theme = get(5);
            for p in parts.iter().skip(6) {
                let p = unesc(p);
                if let Some(v) = p.strip_prefix("l1=") {
                    g.l1 = v.into();
                } else if let Some(v) = p.strip_prefix("l2=") {
                    g.l2 = v.into();
                } else if let Some(v) = p.strip_prefix("ver=") {
                    g.ver = v.trim().into();
                } else if let Some(v) = p.strip_prefix("mirror=") {
                    // fichier:préfixe_source->préfixe_destination
                    let (file, rest) = v.split_once(':').ok_or_else(|| ctx("mirror=fichier:src->dst attendu".into()))?;
                    let (src, dst) = rest.split_once("->").ok_or_else(|| ctx("mirror=fichier:src->dst attendu".into()))?;
                    g.mirror = Some(Mirror { file: file.trim().into(), src: src.trim().trim_matches('/').into(), dst: dst.trim().trim_matches('/').into() });
                } else if let Some((os, pat)) = p.split_once(':') {
                    // ancien format : un motif par plateforme (macOS n'est plus géré)
                    if os == os_tag() {
                        g.patterns.push(pat.trim().to_string());
                    }
                }
            }
        } else if let Some(name) = p0.strip_prefix('#') {
            g.cats.push(Cat { name: unesc(name), icon: parts.get(1).map(|s| unesc(s.trim())).unwrap_or_default(), subs: vec![] });
        } else if let Some(name) = p0.strip_prefix('+') {
            let cat = g.cats.last_mut().ok_or_else(|| ctx("« + » avant le premier « # »".into()))?;
            let mut s = Sub { name: unesc(name), icon: parts.get(1).map(|s| unesc(s.trim())).unwrap_or_default(), cols: 1, fields: vec![] };
            for p in parts.iter().skip(2) {
                if let Some(c) = unesc(p).strip_prefix("c=") {
                    s.cols = c.trim().parse().unwrap_or(1).clamp(1, 3);
                }
            }
            cat.subs.push(s);
        } else {
            let sub = g.cats.last_mut().and_then(|c| c.subs.last_mut()).ok_or_else(|| ctx("champ avant la première carte".into()))?;
            sub.fields.push(field(&parts).map_err(ctx)?);
        }
    }
    if g.id.is_empty() || g.cats.is_empty() {
        return Err("définition vide (en-tête « @ » ou onglets manquants)".into());
    }
    Ok(g)
}

fn field(parts: &[String]) -> Result<Field, String> {
    let kind = match parts[0].trim() {
        "n" => Kind::Num,
        "t" => Kind::Text,
        "b" => Kind::Bool,
        "e" => Kind::Enum,
        "s" => Kind::Slider,
        "l" => Kind::List,
        "g" => Kind::Grid,
        "i" => Kind::Info,
        k => return Err(format!("type de champ inconnu « {k} »")),
    };
    let mut f = Field {
        kind,
        label: parts.get(1).map(|s| unesc(s)).unwrap_or_default(),
        path: parts.get(2).map(|s| unesc(s.trim())).unwrap_or_default(),
        min: f64::NEG_INFINITY,
        max: f64::INFINITY,
        unit: String::new(),
        scale: 1.0,
        dp: 0,
        step: 1.0,
        btns: vec![],
        bh: String::new(),
        opts: vec![],
        name_path: String::new(),
        val_path: String::new(),
        range: (0, 0),
        names: vec![],
        xp: String::new(),
        xt: vec![],
        ro: false,
        off: 0.0,
        also: vec![],
        sync: vec![],
    };
    for p in parts.iter().skip(3) {
        let p = unesc(p);
        let Some((k, v)) = p.split_once('=') else { return Err(format!("option sans « = » : {p}")) };
        match k {
            "min" => f.min = num(v)?,
            "max" => f.max = num(v)?,
            "u" => f.unit = v.into(),
            "sc" => f.scale = num(v)?,
            "dp" => f.dp = num(v)? as usize,
            "st" => f.step = num(v)?,
            "btn" => f.btns.push(match v.rsplit_once(':') {
                Some((l, val)) => (l.into(), val.into()),
                None => (String::new(), v.into()),
            }),
            "bh" => f.bh = v.into(),
            "o" => f.opts = v.split(',').filter_map(|o| o.rsplit_once(':').map(|(l, v)| (l.to_string(), v.to_string()))).collect(),
            "n" => f.name_path = v.into(),
            "v" => f.val_path = v.into(),
            "r" => {
                let (a, b) = v.split_once('-').ok_or("r=début-fin attendu")?;
                f.range = (num(a)? as i32, num(b)? as i32);
            }
            "names" => f.names = v.split(',').map(|s| s.to_string()).collect(),
            "xp" => f.xp = v.into(),
            "xt" => f.xt = v.split(',').filter_map(|x| x.parse().ok()).collect(),
            "ro" => f.ro = v == "1",
            "off" => f.off = num(v)?,
            "also" => f.also.push(v.rsplit_once(':').map(|(p, x)| (p.to_string(), x.to_string())).ok_or("also=chemin:valeur attendu")?),
            "sync" => f.sync.push(match v.strip_suffix(":#") {
                Some(p) => (p.to_string(), true),
                None => (v.to_string(), false),
            }),
            k => return Err(format!("option inconnue « {k} »")),
        }
    }
    if f.scale == 0.0 {
        f.scale = 1.0;
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_os_patterns_are_filtered() {
        let g = parse("@t|T|T|pixelify|xml|wood|win:C:/w|lin:/l|mac:/m|and:/a;#C|gear;+S|gear;n|N|a/b").unwrap();
        let expect = match os_tag() { "win" => "C:/w", "and" => "/a", _ => "/l" };
        assert_eq!(g.patterns, vec![expect.to_string()]);
    }

    #[test]
    fn header_ver_and_mirror() {
        let g = parse("@t|T|T|pixelify|xml|wood|l1=a|ver=SaveGame/gameVersion|mirror=SaveGameInfo:SaveGame/player->Farmer;#C|gear;+S|gear;n|N|a/b").unwrap();
        assert_eq!(g.ver, "SaveGame/gameVersion");
        let m = g.mirror.unwrap();
        assert_eq!((m.file.as_str(), m.src.as_str(), m.dst.as_str()), ("SaveGameInfo", "SaveGame/player", "Farmer"));
        assert!(g.patterns.is_empty());
    }

    #[test]
    fn field_sync_option() {
        let g = parse("@t|T|T|pixelify|xml|wood;#C|gear;+S|gear;s|D|a/d|min=1|max=28|sync=x/y;e|S|a/s|o=A:a,B:b|sync=x/z:#").unwrap();
        let f = &g.cats[0].subs[0].fields;
        assert_eq!(f[0].sync, vec![("x/y".to_string(), false)]);
        assert_eq!(f[1].sync, vec![("x/z".to_string(), true)]);
    }

    /// Essai contre un vrai pack : `SAVESTUDIO_TEST_PACK=/chemin/du/depot-games cargo test`.
    #[test]
    fn real_pack_games_parse() {
        let Ok(dir) = std::env::var("SAVESTUDIO_TEST_PACK") else { return };
        let mut n = 0;
        for e in std::fs::read_dir(std::path::Path::new(&dir).join("games")).unwrap().flatten() {
            let src = std::fs::read_to_string(e.path()).unwrap();
            parse(&src).unwrap_or_else(|er| panic!("{} : {er}", e.path().display()));
            n += 1;
        }
        assert!(n > 0);
    }
}

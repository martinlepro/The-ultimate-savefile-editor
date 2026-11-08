//! Modèle commun : chaque format de sauvegarde expose le même petit contrat.
//! Un chemin est une suite d'étapes séparées par `/` :
//!   `nom`  `nom[2]`  `nom[*]` (joker)  `a?b` (alternatives)  `~clé` (plist : valeur après <k>clé</k>)  `@attr`

#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Scalar {
    pub fn text(&self) -> String {
        match self {
            Scalar::Bool(b) => b.to_string(),
            Scalar::Int(i) => i.to_string(),
            Scalar::Float(f) => f.to_string(),
            Scalar::Text(t) => t.clone(),
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Scalar::Bool(b) => Some(*b as u8 as f64),
            Scalar::Int(i) => Some(*i as f64),
            Scalar::Float(f) => Some(*f),
            Scalar::Text(t) => t.trim().parse().ok(),
        }
    }
    pub fn flag(&self) -> Option<bool> {
        match self {
            Scalar::Bool(b) => Some(*b),
            Scalar::Int(i) => Some(*i != 0),
            Scalar::Float(f) => Some(*f != 0.0),
            Scalar::Text(t) => match t.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" | "" => Some(false),
                _ => None,
            },
        }
    }
}

pub trait Doc {
    fn get(&self, path: &str) -> Option<Scalar>;
    fn set(&mut self, path: &str, v: Scalar) -> Result<(), String>;
    /// Remplace le premier joker du chemin par chaque index/clé existant.
    fn expand(&self, path: &str) -> Vec<String>;
    fn encode(&self) -> Result<Vec<u8>, String>;
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Idx {
    None,
    N(usize),
    All,
}

pub struct Step<'a> {
    pub alts: Vec<&'a str>,
    pub key: bool,
    pub idx: Idx,
}

pub fn parse_step(s: &str) -> Step<'_> {
    let (s, key) = match s.strip_prefix('~') {
        Some(r) => (r, true),
        None => (s, false),
    };
    let mut name = s;
    let mut idx = Idx::None;
    if s.ends_with(']') {
        if let Some(p) = s.rfind('[') {
            let inside = &s[p + 1..s.len() - 1];
            if inside == "*" {
                idx = Idx::All;
                name = &s[..p];
            } else if let Ok(n) = inside.parse() {
                idx = Idx::N(n);
                name = &s[..p];
            }
        }
    }
    if name == "*" && idx == Idx::None {
        idx = Idx::All;
    }
    Step { alts: name.split('?').collect(), key, idx }
}

pub fn first_wildcard(path: &str) -> Option<usize> {
    path.split('/').position(|s| parse_step(s).idx == Idx::All)
}

pub fn scalar_to_i64(v: &Scalar) -> Option<i64> {
    v.num().map(|f| f.round() as i64)
}

/// Charge un fichier selon le codec déclaré par le jeu.
pub fn open(codec: &str, bytes: &[u8]) -> Result<Box<dyn Doc>, String> {
    match codec {
        "xml" => Ok(Box::new(crate::xml::XmlDoc::parse(bytes.to_vec(), crate::xml::Wrap::Plain)?)),
        "gd" => Ok(Box::new(crate::gd::open(bytes)?)),
        "hk" => Ok(Box::new(crate::hk::HkDoc::open(bytes)?)),
        "nbt" => Ok(Box::new(crate::nbt::NbtDoc::open(bytes)?)),
        c => Err(format!("codec inconnu : {c}")),
    }
}

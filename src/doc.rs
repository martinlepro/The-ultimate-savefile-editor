//! Valeurs et interface d'un document de sauvegarde. La lecture/écriture des formats (xml, gd, hk, nbt) n'est pas ici :
//! elle vient du pack de jeux (`wasm/savecodecs.wasm`), exécuté par `plugin.rs`.

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

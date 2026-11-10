//! Réglages de l'utilisateur (tout ce qui est personnalisable), enregistrés dans `settings.json`.
use crate::pack;
use crate::theme::{parse_hex, to_hex, Theme};
use eframe::egui::Color32;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TabsPos {
    Left,
    Right,
    Top,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActPos {
    Bottom,
    Top,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Density {
    Compact,
    Normal,
    Roomy,
}

impl Density {
    pub fn factor(self) -> f32 {
        match self {
            Density::Compact => 0.55,
            Density::Normal => 1.0,
            Density::Roomy => 1.4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub scale: f64,
    pub backup: bool,
    pub auto_update: bool,
    // polices : clé d'une police du pack (« pixelify »…) ou « sys:<fichier> » ; vide = celle du jeu (éditeur seulement)
    pub title_font: String,
    pub settings_font: String,
    pub editor_font: String,
    pub title_theme: String,
    // apparence
    pub text_scale: f32,
    /// 0 = selon le thème, 1 = majuscules, 2 = casse normale
    pub caps: u8,
    /// 0 = selon le thème, 1 = angles droits, 2 = arrondis
    pub corners: u8,
    pub ov_bg: Option<Color32>,
    pub ov_panel: Option<Color32>,
    pub ov_accent: Option<Color32>,
    pub ov_title: Option<Color32>,
    // disposition de l'éditeur
    pub tabs: TabsPos,
    pub actions: ActPos,
    pub density: Density,
    /// 0 = selon la carte, sinon 1 à 3 colonnes partout
    pub cols: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            scale: 1.0,
            backup: true,
            auto_update: true,
            title_font: "pixelify".into(),
            settings_font: "pixelify".into(),
            editor_font: String::new(),
            title_theme: "home".into(),
            text_scale: 1.0,
            caps: 0,
            corners: 0,
            ov_bg: None,
            ov_panel: None,
            ov_accent: None,
            ov_title: None,
            tabs: TabsPos::Left,
            actions: ActPos::Bottom,
            density: Density::Normal,
            cols: 0,
        }
    }
}

pub fn settings_path() -> PathBuf {
    pack::data_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).map(|v| Settings::from_json(&v)).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = settings_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&self.to_json()).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    pub fn from_json(v: &Value) -> Settings {
        let d = Settings::default();
        let s = |k: &str, def: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or(def).to_string();
        let b = |k: &str, def: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(def);
        let f = |k: &str, def: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(def);
        let u = |k: &str, def: u8, max: u8| (v.get(k).and_then(|x| x.as_u64()).unwrap_or(def as u64) as u8).min(max);
        let c = |k: &str| v.get(k).and_then(|x| x.as_str()).and_then(parse_hex);
        Settings {
            scale: f("scale", d.scale).clamp(0.7, 2.0),
            backup: b("backup", d.backup),
            auto_update: b("auto_update", d.auto_update),
            title_font: s("title_font", &d.title_font),
            settings_font: s("settings_font", &d.settings_font),
            editor_font: s("editor_font", ""),
            title_theme: s("title_theme", &d.title_theme),
            text_scale: (f("text_scale", 1.0) as f32).clamp(0.7, 1.6),
            caps: u("caps", 0, 2),
            corners: u("corners", 0, 2),
            ov_bg: c("ov_bg"),
            ov_panel: c("ov_panel"),
            ov_accent: c("ov_accent"),
            ov_title: c("ov_title"),
            tabs: match s("tabs", "left").as_str() {
                "right" => TabsPos::Right,
                "top" => TabsPos::Top,
                _ => TabsPos::Left,
            },
            actions: if s("actions", "bottom") == "top" { ActPos::Top } else { ActPos::Bottom },
            density: match s("density", "normal").as_str() {
                "compact" => Density::Compact,
                "roomy" => Density::Roomy,
                _ => Density::Normal,
            },
            cols: u("cols", 0, 3),
        }
    }

    pub fn to_json(&self) -> Value {
        let c = |o: Option<Color32>| o.map(to_hex).map(Value::from).unwrap_or(Value::Null);
        json!({
            "scale": self.scale, "backup": self.backup, "auto_update": self.auto_update,
            "title_font": self.title_font, "settings_font": self.settings_font, "editor_font": self.editor_font,
            "title_theme": self.title_theme,
            "text_scale": self.text_scale, "caps": self.caps, "corners": self.corners,
            "ov_bg": c(self.ov_bg), "ov_panel": c(self.ov_panel), "ov_accent": c(self.ov_accent), "ov_title": c(self.ov_title),
            "tabs": match self.tabs { TabsPos::Left => "left", TabsPos::Right => "right", TabsPos::Top => "top" },
            "actions": match self.actions { ActPos::Bottom => "bottom", ActPos::Top => "top" },
            "density": match self.density { Density::Compact => "compact", Density::Normal => "normal", Density::Roomy => "roomy" },
            "cols": self.cols,
        })
    }

    /// Applique les personnalisations de l'utilisateur par-dessus un thème de jeu.
    pub fn themed(&self, base: &Theme) -> Theme {
        let mut t = base.clone();
        t.fs *= self.text_scale;
        match self.caps {
            1 => t.caps = true,
            2 => t.caps = false,
            _ => {}
        }
        match self.corners {
            1 => {
                t.round = 0;
            }
            2 => {
                t.round = 8;
                t.bevel = false;
            }
            _ => {}
        }
        if let Some(c) = self.ov_bg {
            t.bg = c;
        }
        if let Some(c) = self.ov_panel {
            t.panel = c;
            t.input = crate::theme::shade(c, if c.r() > 128 { 1.08 } else { 0.7 });
        }
        if let Some(c) = self.ov_accent {
            t.accent = c;
        }
        if let Some(c) = self.ov_title {
            t.title = c;
        }
        // texte lisible quel que soit le fond choisi
        if self.ov_panel.is_some() {
            let light = t.panel.r() as u32 + t.panel.g() as u32 + t.panel.b() as u32 > 384;
            t.ink = if light { Color32::from_rgb(0x20, 0x20, 0x24) } else { Color32::from_rgb(0xf0, 0xf0, 0xf0) };
            t.ink_soft = if light { Color32::from_rgb(0x55, 0x55, 0x5c) } else { Color32::from_rgb(0xb4, 0xb4, 0xbc) };
            t.input_ink = if t.input.r() > 128 { Color32::from_rgb(0x20, 0x20, 0x24) } else { Color32::WHITE };
        }
        t
    }
}

// ------------------------------------------------------------------ polices du système

#[derive(Clone, Debug)]
pub struct SysFont {
    /// clé : « sys:<nom de fichier sans extension> »
    pub key: String,
    pub name: String,
    pub path: PathBuf,
}

fn walk_fonts(dir: &Path, depth: u8, out: &mut Vec<SysFont>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk_fonts(&p, depth - 1, out);
            }
        } else if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
            if matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    out.push(SysFont { key: format!("sys:{stem}"), name: stem.replace(['-', '_'], " "), path: p });
                }
            }
        }
    }
}

/// Toutes les polices .ttf / .otf installées sur la machine (triées, sans doublon).
pub fn system_fonts() -> Vec<SysFont> {
    let mut dirs: Vec<PathBuf> = vec![];
    if cfg!(target_os = "windows") {
        if let Ok(w) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(w).join("Fonts"));
        }
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
        }
    } else if cfg!(target_os = "android") {
        dirs.push("/system/fonts".into());
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Ok(h) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&h).join(".local/share/fonts"));
            dirs.push(PathBuf::from(&h).join(".fonts"));
        }
    }
    let mut v = vec![];
    for d in dirs {
        walk_fonts(&d, 4, &mut v);
    }
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v.dedup_by(|a, b| a.key == b.key);
    v.truncate(3000);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let mut s = Settings::default();
        s.title_font = "sys:DejaVuSans".into();
        s.ov_accent = parse_hex("#ff8800");
        s.tabs = TabsPos::Top;
        s.density = Density::Compact;
        s.cols = 2;
        let back = Settings::from_json(&s.to_json());
        assert_eq!(back.title_font, "sys:DejaVuSans");
        assert_eq!(back.ov_accent, parse_hex("#ff8800"));
        assert_eq!(back.tabs, TabsPos::Top);
        assert_eq!(back.density, Density::Compact);
        assert_eq!(back.cols, 2);
        assert!(back.ov_bg.is_none());
    }

    #[test]
    fn bad_values_are_clamped() {
        let s = Settings::from_json(&serde_json::json!({"scale": 99, "text_scale": 0.01, "cols": 77, "tabs": "bogus"}));
        assert_eq!(s.scale, 2.0);
        assert_eq!(s.text_scale, 0.7);
        assert_eq!(s.cols, 3);
        assert_eq!(s.tabs, TabsPos::Left);
    }
}

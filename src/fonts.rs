//! Polices : Pixelify Sans intégrée (interface minimale sans pack), polices du pack, et polices du système
//! chargées seulement quand l'utilisateur les choisit.
use crate::gfx;
use crate::pack::FontSpec;
use crate::settings::SysFont;
use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

const BUILTIN: &[u8] = include_bytes!("../assets/fonts/pixelify.ttf");
const BUILTIN_BOLD: &[u8] = include_bytes!("../assets/fonts/pixelify-bold.ttf");

pub struct Choice {
    pub key: String,
    pub name: String,
    pub system: bool,
}

/// Toutes les polices proposables : celles du pack, puis celles du système.
pub fn choices(pack: &[FontSpec], sys: &[SysFont]) -> Vec<Choice> {
    let mut v: Vec<Choice> = vec![];
    if !pack.iter().any(|f| f.name == "pixelify") {
        v.push(Choice { key: "pixelify".into(), name: "Pixelify Sans".into(), system: false });
    }
    for f in pack {
        v.push(Choice { key: f.name.clone(), name: pretty_pack(&f.name), system: false });
    }
    for s in sys {
        v.push(Choice { key: s.key.clone(), name: s.name.clone(), system: true });
    }
    v
}

fn pretty_pack(n: &str) -> String {
    match n {
        "pixelify" => "Pixelify Sans".into(),
        "vt323" => "VT323".into(),
        "trajan" => "Trajan".into(),
        "celeste" => "Celeste".into(),
        "blocks" => "Blocks".into(),
        o => o.to_string(),
    }
}

pub fn display_name(key: &str, sys: &[SysFont]) -> String {
    if let Some(s) = sys.iter().find(|s| s.key == key) {
        return s.name.clone();
    }
    if let Some(rest) = key.strip_prefix("sys:") {
        return rest.replace(['-', '_'], " ");
    }
    pretty_pack(key)
}

/// (Re)construit les familles `b:<clé>` (texte) et `h:<clé>` (titres) pour egui.
/// `wanted` : clés de polices système à charger maintenant.
pub fn install(ctx: &Context, pack: &[FontSpec], sys: &[SysFont], wanted: &[String]) -> Vec<String> {
    let mut fd = FontDefinitions::empty();
    let add = |fd: &mut FontDefinitions, id: &str, bytes: Vec<u8>| fd.font_data.insert(id.to_string(), Arc::new(FontData::from_owned(bytes)));
    add(&mut fd, "builtin", BUILTIN.to_vec());
    add(&mut fd, "builtin-bold", BUILTIN_BOLD.to_vec());
    let fam = |k: &str, h: bool| FontFamily::Name(format!("{}:{k}", if h { "h" } else { "b" }).into());
    // « pixelify » existe toujours, même sans pack
    fd.families.insert(fam("pixelify", false), vec!["builtin".into()]);
    fd.families.insert(fam("pixelify", true), vec!["builtin-bold".into(), "builtin".into()]);

    let mut snaps = vec![];
    let mut defined = vec!["pixelify".to_string()];
    for f in pack {
        let Ok(reg) = std::fs::read(&f.regular) else { continue };
        let rid = format!("p:{}", f.name);
        add(&mut fd, &rid, reg);
        let mut head = vec![rid.clone()];
        if let Some(Ok(b)) = f.bold.as_ref().map(std::fs::read) {
            let bid = format!("p:{}:b", f.name);
            add(&mut fd, &bid, b);
            head = vec![bid];
        }
        let tail = |mut v: Vec<String>| {
            if f.name != "pixelify" {
                v.push("builtin".into());
            }
            v
        };
        fd.families.insert(fam(&f.name, false), tail(vec![rid.clone()]));
        fd.families.insert(fam(&f.name, true), tail(head));
        snaps.push((f.name.clone(), f.snap));
        defined.push(f.name.clone());
    }
    for key in wanted {
        if fd.families.contains_key(&fam(key, false)) {
            continue;
        }
        if let Some(s) = sys.iter().find(|s| &s.key == key) {
            if let Ok(bytes) = std::fs::read(&s.path) {
                let id = format!("s:{}", s.key);
                add(&mut fd, &id, bytes);
                fd.families.insert(fam(key, false), vec![id.clone(), "builtin".into()]);
                fd.families.insert(fam(key, true), vec![id, "builtin".into()]);
                defined.push(key.clone());
            }
        }
    }
    gfx::set_snaps(snaps);
    fd.families.insert(FontFamily::Proportional, vec!["builtin".into()]);
    fd.families.insert(FontFamily::Monospace, vec!["builtin".into()]);
    ctx.set_fonts(fd);
    defined.sort();
    defined.dedup();
    defined
}

//! Thèmes visuels : un par jeu. Tout est paramétré (couleurs, épaisseur des bordures, police).
use eframe::egui::Color32;

pub const fn hex(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

#[derive(Clone)]
pub struct Theme {
    pub fs: f32,
    pub bg: Color32,
    pub bg_edge: Color32,
    pub title: Color32,
    pub title_shadow: Color32,
    pub panel: Color32,
    pub panel_edge: Color32,
    pub card: Color32,
    pub card_alt: Color32,
    pub ink: Color32,
    pub ink_soft: Color32,
    pub input: Color32,
    pub input_ink: Color32,
    pub tab: Color32,
    pub tab_sel: Color32,
    pub tab_ink: Color32,
    pub outline: Color32,
    pub hi: Color32,
    pub lo: Color32,
    pub accent: Color32,
    pub knob: Color32,
    pub btn: [Color32; 4],
    pub btn_ink: Color32,
    pub preset: Color32,
    pub bw: f32,
    pub round: u8,
    pub caps: bool,
    pub bevel: bool,
}

impl Theme {
    /// Thèmes intégrés (secours quand aucun pack n'est installé) : `home` (écran titre) et `wood` (défaut).
    pub fn builtin(n: &str) -> Theme {
        match n {
            "home" => Theme {
                fs: 1.0,
                bg: hex(0x15171d), bg_edge: hex(0x262a33), title: hex(0xf4f1e8), title_shadow: hex(0x000000),
                panel: hex(0x1d2028), panel_edge: hex(0x3a3f4b), card: hex(0x242833), card_alt: hex(0x2a2f3b),
                ink: hex(0xece9e0), ink_soft: hex(0x9aa0ad), input: hex(0x0f1116), input_ink: hex(0xffffff),
                tab: hex(0x242833), tab_sel: hex(0x3a4050), tab_ink: hex(0xece9e0),
                outline: hex(0x0b0c10), hi: hex(0x3a4050), lo: hex(0x0b0c10), accent: hex(0x7ec8a0), knob: hex(0xffffff),
                btn: [hex(0xd98b3a), hex(0x2d8fe3), hex(0x58b83f), hex(0xf0e04f)], btn_ink: hex(0xffffff), preset: hex(0x3a4050),
                bw: 2.0, round: 6, caps: false, bevel: false,
            },
            _ => Theme {
                fs: 1.0,
                bg: hex(0x2a140a), bg_edge: hex(0x5a3318), title: hex(0xf6c64a), title_shadow: hex(0x3a1a08),
                panel: hex(0xecd7ac), panel_edge: hex(0xa5692f), card: hex(0xdcae7c), card_alt: hex(0xe2b583),
                ink: hex(0x3a1a08), ink_soft: hex(0x6b4423), input: hex(0xfff4d6), input_ink: hex(0x3a1a08),
                tab: hex(0xbc956c), tab_sel: hex(0xe9ccaa), tab_ink: hex(0x3a1a08),
                outline: hex(0x3a1a08), hi: hex(0xf6e3b8), lo: hex(0xa87a4c), accent: hex(0x56ba3c), knob: hex(0xfff4d6),
                btn: [hex(0xd98b3a), hex(0x2d8fe3), hex(0x58b83f), hex(0xf0e04f)], btn_ink: hex(0xfffaf0), preset: hex(0xefe2b6),
                bw: 3.0, round: 0, caps: true, bevel: true,
            },
        }
    }

    /// Thème décrit par un objet de `themes.json` ; les clés absentes reprennent celles de `base`.
    pub fn from_json(base: &Theme, v: &serde_json::Value) -> Theme {
        let mut t = base.clone();
        let col = |k: &str| v.get(k).and_then(|x| x.as_str()).and_then(parse_hex);
        macro_rules! c {
            ($f:ident) => {
                if let Some(x) = col(stringify!($f)) {
                    t.$f = x;
                }
            };
        }
        c!(bg); c!(bg_edge); c!(title); c!(title_shadow); c!(panel); c!(panel_edge); c!(card); c!(card_alt);
        c!(ink); c!(ink_soft); c!(input); c!(input_ink); c!(tab); c!(tab_sel); c!(tab_ink); c!(outline);
        c!(hi); c!(lo); c!(accent); c!(knob); c!(btn_ink); c!(preset);
        if let Some(a) = v.get("btn").and_then(|x| x.as_array()) {
            for (i, x) in a.iter().take(4).enumerate() {
                if let Some(c) = x.as_str().and_then(parse_hex) {
                    t.btn[i] = c;
                }
            }
        }
        if let Some(x) = v.get("fs").and_then(|x| x.as_f64()) {
            t.fs = x as f32;
        }
        if let Some(x) = v.get("bw").and_then(|x| x.as_f64()) {
            t.bw = x as f32;
        }
        if let Some(x) = v.get("round").and_then(|x| x.as_f64()) {
            t.round = x.clamp(0.0, 30.0) as u8;
        }
        if let Some(x) = v.get("caps").and_then(|x| x.as_bool()) {
            t.caps = x;
        }
        if let Some(x) = v.get("bevel").and_then(|x| x.as_bool()) {
            t.bevel = x;
        }
        t
    }
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(hex)
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

pub fn shade(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| ((v as f32 * f).clamp(0.0, 255.0)) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

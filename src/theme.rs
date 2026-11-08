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
    pub fn by_name(n: &str) -> Theme {
        match n {
            "celeste" => Theme {
                fs: 1.12,
                bg: hex(0x120f2b), bg_edge: hex(0x2c2460), title: hex(0xff8fb1), title_shadow: hex(0x2a1038),
                panel: hex(0x1c1940), panel_edge: hex(0x8a7bd8), card: hex(0x2a2560), card_alt: hex(0x302a6c),
                ink: hex(0xf4eefc), ink_soft: hex(0xb7acf0), input: hex(0x0f0c26), input_ink: hex(0xffffff),
                tab: hex(0x2a2560), tab_sel: hex(0x5b4cc4), tab_ink: hex(0xf4eefc),
                outline: hex(0x090719), hi: hex(0x6f60d8), lo: hex(0x120f30), accent: hex(0xff6fa0), knob: hex(0xfff0f6),
                btn: [hex(0xd94a7a), hex(0x4a8ad9), hex(0x3fbf98), hex(0xf0c040)], btn_ink: hex(0xffffff), preset: hex(0x4b3fa8),
                bw: 2.0, round: 7, caps: false, bevel: false,
            },
            "hollow" => Theme {
                fs: 0.92,
                bg: hex(0x050507), bg_edge: hex(0x1a1a22), title: hex(0xe9ecef), title_shadow: hex(0x000000),
                panel: hex(0x0b0c10), panel_edge: hex(0x9aa0a8), card: hex(0x14161c), card_alt: hex(0x191c23),
                ink: hex(0xdfe3e8), ink_soft: hex(0x8e96a0), input: hex(0x06070a), input_ink: hex(0xf2f4f7),
                tab: hex(0x0f1114), tab_sel: hex(0x252a33), tab_ink: hex(0xdfe3e8),
                outline: hex(0x7d848d), hi: hex(0x3a404a), lo: hex(0x050507), accent: hex(0xdfe6ee), knob: hex(0x0b0c10),
                btn: [hex(0x2c2f38), hex(0x27384a), hex(0x2a3d36), hex(0x413d2a)], btn_ink: hex(0xeef1f5), preset: hex(0x20242c),
                bw: 1.5, round: 2, caps: true, bevel: false,
            },
            "blocks" => Theme {
                fs: 1.5,
                bg: hex(0x1b3fa8), bg_edge: hex(0x2f6bf0), title: hex(0xffffff), title_shadow: hex(0x000000),
                panel: hex(0x2f63d9), panel_edge: hex(0x000000), card: hex(0x1e4cc0), card_alt: hex(0x2356cf),
                ink: hex(0xffffff), ink_soft: hex(0xbcd4ff), input: hex(0x0c2468), input_ink: hex(0xffffff),
                tab: hex(0x1a3f9c), tab_sel: hex(0x42a8ff), tab_ink: hex(0xffffff),
                outline: hex(0x000000), hi: hex(0x6fa2ff), lo: hex(0x0e2a78), accent: hex(0x6ee03a), knob: hex(0xffffff),
                btn: [hex(0xff8c1a), hex(0x39a0ff), hex(0x6ee03a), hex(0xffd83a)], btn_ink: hex(0xffffff), preset: hex(0x6ee03a),
                bw: 3.0, round: 0, caps: true, bevel: true,
            },
            "stone" => Theme {
                fs: 1.22,
                bg: hex(0x262626), bg_edge: hex(0x3c3c3c), title: hex(0xffffff), title_shadow: hex(0x3f3f3f),
                panel: hex(0x8b8b8b), panel_edge: hex(0x373737), card: hex(0xc6c6c6), card_alt: hex(0xb9b9b9),
                ink: hex(0x303030), ink_soft: hex(0x555555), input: hex(0x000000), input_ink: hex(0xffffff),
                tab: hex(0x6b6b6b), tab_sel: hex(0xc6c6c6), tab_ink: hex(0xffffff),
                outline: hex(0x1a1a1a), hi: hex(0xffffff), lo: hex(0x555555), accent: hex(0x55ff55), knob: hex(0xe0e0e0),
                btn: [hex(0x866043), hex(0x3a8fd5), hex(0x3c8527), hex(0xe8c53a)], btn_ink: hex(0xffffff), preset: hex(0x6d6d6d),
                bw: 3.0, round: 0, caps: false, bevel: true,
            },
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
            // "wood" (Stardew) par défaut
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
}

pub fn shade(c: Color32, f: f32) -> Color32 {
    let m = |v: u8| ((v as f32 * f).clamp(0.0, 255.0)) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

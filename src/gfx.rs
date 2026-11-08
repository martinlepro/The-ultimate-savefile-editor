//! Primitives graphiques : icônes pixel-art, cadres, boutons, interrupteurs, ressorts numériques.
use crate::theme::{hex, shade, Theme};
use eframe::egui::{self, *};
use std::collections::HashMap;

pub const PAL: &[(char, u32)] = &[
    ('k', 0x2b1a0e), ('w', 0xfff6dc), ('r', 0xd8402e), ('R', 0x9e2a20), ('y', 0xf6d445), ('o', 0xec9a2e), ('b', 0x9a6330), ('B', 0x5e3a1c),
    ('n', 0xdcb67f), ('g', 0x4fae3e), ('G', 0x2f7a2c), ('l', 0x9ad84f), ('c', 0x52c4ee), ('d', 0x2f72d8), ('D', 0x20449a), ('p', 0xf08bb0),
    ('s', 0xcfd3d8), ('a', 0x8d939b), ('v', 0x8e5cd6), ('t', 0x3a3a46),
];

pub struct Icons {
    tex: HashMap<String, TextureHandle>,
}

impl Icons {
    pub fn new(ctx: &Context) -> Icons {
        let mut tex = HashMap::new();
        for block in include_str!("../assets/icons.txt").split("\n\n") {
            let mut lines = block.trim().lines();
            let Some(name) = lines.next() else { continue };
            let mut px = Vec::with_capacity(144);
            for l in lines.take(12) {
                for ch in l.chars() {
                    px.push(PAL.iter().find(|(c, _)| *c == ch).map(|(_, v)| hex(*v)).unwrap_or(Color32::TRANSPARENT));
                }
            }
            if px.len() == 144 {
                let rgba: Vec<u8> = px.iter().flat_map(|c| c.to_array()).collect();
                let img = ColorImage::from_rgba_unmultiplied([12, 12], &rgba);
                tex.insert(name.to_string(), ctx.load_texture(name, img, TextureOptions::NEAREST));
            }
        }
        Icons { tex }
    }

    pub fn paint(&self, p: &Painter, name: &str, rect: Rect) {
        let Some(t) = self.tex.get(name).or_else(|| self.tex.get("gear")) else { return };
        let ppp = p.ctx().pixels_per_point();
        let s = ((rect.width().min(rect.height()) * ppp / 12.0).floor().max(1.0) * 12.0) / ppp;
        let r = Rect::from_center_size(rect.center(), vec2(s, s));
        p.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}

pub struct Gfx<'a> {
    pub th: &'a Theme,
    pub icons: &'a Icons,
    pub font: &'a str,
}

impl<'a> Gfx<'a> {
    /// Les polices pixel (Pixelify) ne restent nettes qu'à des multiples de 10 px.
    fn size(&self, sz: f32) -> f32 {
        let v = sz * self.th.fs;
        if self.font == "pixelify" { ((v / 10.0).round() * 10.0).max(20.0) } else { v }
    }
    pub fn body(&self, sz: f32) -> FontId {
        FontId::new(self.size(sz), FontFamily::Name(format!("b:{}", self.font).into()))
    }
    pub fn head(&self, sz: f32) -> FontId {
        FontId::new(self.size(sz), FontFamily::Name(format!("h:{}", self.font).into()))
    }
    pub fn text_w(&self, p: &Painter, s: &str, f: FontId) -> f32 {
        p.layout_no_wrap(s.to_string(), f, Color32::WHITE).size().x
    }
    pub fn cap(&self, s: &str) -> String {
        if self.th.caps { s.to_uppercase() } else { s.to_string() }
    }
}

pub fn apply_style(ctx: &Context, g: &Gfx) {
    let th = g.th;
    let mut st = (*ctx.style()).clone();
    st.text_styles = [
        (TextStyle::Small, g.body(15.0)),
        (TextStyle::Body, g.body(21.0)),
        (TextStyle::Button, g.body(21.0)),
        (TextStyle::Heading, g.head(28.0)),
        (TextStyle::Monospace, g.body(19.0)),
    ]
    .into();
    let v = &mut st.visuals;
    v.dark_mode = th.bg.r() < 128;
    v.override_text_color = None;
    v.panel_fill = th.bg;
    v.window_fill = th.input;
    v.window_stroke = Stroke::new(th.bw, th.outline);
    v.window_corner_radius = CornerRadius::same(th.round);
    v.menu_corner_radius = CornerRadius::same(th.round);
    v.popup_shadow = Shadow::NONE;
    v.window_shadow = Shadow::NONE;
    v.extreme_bg_color = th.input;
    v.faint_bg_color = th.input;
    v.selection.bg_fill = th.accent;
    v.selection.stroke = Stroke::new(1.0_f32, th.input_ink);
    v.text_cursor.stroke = Stroke::new(2.0, th.input_ink);
    v.handle_shape = style::HandleShape::Rect { aspect_ratio: 0.5 };
    v.slider_trailing_fill = true;
    let rail = shade(th.input, if th.input.r() > 128 { 0.82 } else { 1.7 });
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, th.ink);
    v.widgets.noninteractive.bg_fill = th.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, th.outline);
    for (i, w) in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open].into_iter().enumerate() {
        w.corner_radius = CornerRadius::same(th.round);
        w.expansion = 0.0;
        w.bg_fill = if i == 0 { rail } else { shade(rail, 0.92) };
        w.weak_bg_fill = th.input;
        w.bg_stroke = Stroke::new(th.bw.min(2.0), th.outline);
        w.fg_stroke = Stroke::new(1.5, th.input_ink);
    }
    st.spacing.item_spacing = vec2(10.0, 8.0);
    st.spacing.slider_width = 220.0;
    st.spacing.interact_size = vec2(40.0, 34.0);
    let mut sc = style::ScrollStyle::solid();
    sc.bar_width = 14.0;
    sc.foreground_color = false;
    st.spacing.scroll = sc;
    ctx.set_style(st);
}

pub fn boxed(p: &Painter, r: Rect, fill: Color32, th: &Theme, edge: Color32) {
    p.rect_filled(r, CornerRadius::same(th.round), edge);
    let inner = r.shrink(th.bw);
    p.rect_filled(inner, CornerRadius::same(th.round.saturating_sub(1)), fill);
    if th.bevel && th.round == 0 && inner.height() > 14.0 {
        let t = (th.bw * 0.66).round().max(1.0);
        let z = CornerRadius::ZERO;
        let (hi, lo) = (blend(fill, th.hi), blend(fill, th.lo));
        p.rect_filled(Rect::from_min_size(inner.min, vec2(inner.width(), t)), z, hi);
        p.rect_filled(Rect::from_min_size(inner.min, vec2(t, inner.height())), z, hi);
        p.rect_filled(Rect::from_min_max(pos2(inner.min.x, inner.max.y - t), inner.max), z, lo);
        p.rect_filled(Rect::from_min_max(pos2(inner.max.x - t, inner.min.y), inner.max), z, lo);
    }
}

fn blend(a: Color32, b: Color32) -> Color32 {
    let m = |x: u8, y: u8| ((x as u16 + y as u16 * 2) / 3) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

pub fn text_shadow(p: &Painter, pos: Pos2, al: Align2, s: &str, f: FontId, c: Color32, sh: Color32, off: f32) {
    p.text(pos + vec2(off, off), al, s, f.clone(), sh);
    p.text(pos, al, s, f, c);
}

/// Réduit la taille de police pour que le texte tienne dans `maxw`.
pub fn fit(p: &Painter, text: &str, font: FontId, maxw: f32) -> FontId {
    let w = p.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x;
    if w > maxw && w > 0.0 { FontId::new((font.size * maxw / w).max(11.0), font.family) } else { font }
}

pub fn thousands(v: f64, dp: usize) -> String {
    let s = format!("{:.*}", dp, v.abs());
    let (ip, fp) = match s.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (s, None),
    };
    let mut out = String::new();
    for (i, ch) in ip.chars().enumerate() {
        if i > 0 && (ip.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if let Some(f) = fp {
        out.push('.');
        out.push_str(&f);
    }
    if v < 0.0 && out.chars().any(|c| c != '0' && c != '.' && c != ',') {
        out.insert(0, '-');
    }
    out
}

pub fn pretty(s: &str) -> String {
    let t = s.rsplit(':').next().unwrap_or(s).replace('_', " ");
    if t.chars().any(|c| c.is_uppercase()) || t.contains(' ') && t.chars().next().map_or(false, |c| c.is_uppercase()) {
        return t;
    }
    let mut c = t.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => t,
    }
}

fn tri(p: &Painter, c: Pos2, up: bool, s: f32, col: Color32) {
    let d = if up { -1.0 } else { 1.0 };
    p.add(Shape::convex_polygon(vec![pos2(c.x - s, c.y - d * s * 0.5), pos2(c.x + s, c.y - d * s * 0.5), pos2(c.x, c.y + d * s * 0.6)], col, Stroke::NONE));
}

pub const ROW_H: f32 = 40.0;

/// Champ numérique encadré avec flèches. Retourne true si la valeur a changé.
pub fn spin(ui: &mut Ui, g: &Gfx, v: &mut f64, min: f64, max: f64, step: f64, dp: usize, unit: &str, w: f32, ro: bool, id: Id) -> bool {
    let th = g.th;
    let h = ROW_H - 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter_at(rect.expand(1.0));
    boxed(&p, rect, th.input, th, th.outline);
    let aw = if ro { 0.0 } else { 26.0 };
    let inner = Rect::from_min_max(rect.min + vec2(th.bw + 4.0, th.bw), pos2(rect.max.x - aw - th.bw, rect.max.y - th.bw));
    let unit_s = if unit.is_empty() { String::new() } else { format!(" {unit}") };
    let mut changed = false;
    if ro {
        p.text(inner.center(), Align2::CENTER_CENTER, format!("{}{}", thousands(*v, dp), unit_s), g.body(21.0), th.input_ink);
        return false;
    }
    let range = if max.is_finite() && min.is_finite() { max - min } else { 1e6 };
    let speed = if range <= 200.0 { step.max(0.05) } else { (range / 4000.0).clamp(step, 5000.0) };
    let mut tmp = *v;
    let us = unit_s.clone();
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::centered_and_justified(Direction::LeftToRight)).id_salt(id), |ui| {
        let vis = ui.visuals_mut();
        for w in [&mut vis.widgets.inactive, &mut vis.widgets.hovered, &mut vis.widgets.active] {
            w.bg_fill = Color32::TRANSPARENT;
            w.weak_bg_fill = Color32::TRANSPARENT;
            w.bg_stroke = Stroke::NONE;
        }
        let dv = DragValue::new(&mut tmp)
            .range(min..=max)
            .speed(speed)
            .custom_formatter(move |n, _| format!("{}{}", thousands(n, dp), us))
            .custom_parser(|s| {
                let t: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                t.parse().ok()
            });
        ui.add(dv);
    });
    if tmp != *v {
        *v = tmp;
        changed = true;
    }
    // flèches
    let ax = rect.max.x - aw - th.bw + 2.0;
    let up = Rect::from_min_max(pos2(ax, rect.min.y + th.bw), pos2(rect.max.x - th.bw, rect.center().y));
    let dn = Rect::from_min_max(pos2(ax, rect.center().y), pos2(rect.max.x - th.bw, rect.max.y - th.bw));
    for (r, is_up) in [(up, true), (dn, false)] {
        let rs = ui.interact(r, id.with(is_up), Sense::click());
        let col = if rs.hovered() { th.accent } else { shade(th.input_ink, 0.75) };
        tri(&p, r.center(), is_up, 5.0, col);
        if rs.clicked() {
            let nv = (*v + if is_up { step } else { -step }).clamp(min, max);
            if nv != *v {
                *v = nv;
                changed = true;
            }
        }
    }
    p.line_segment([pos2(ax - 1.0, rect.min.y + th.bw), pos2(ax - 1.0, rect.max.y - th.bw)], Stroke::new(1.0, shade(th.input_ink, 0.5)));
    changed
}

pub fn toggle(ui: &mut Ui, g: &Gfx, on: &mut bool, id: Id) -> bool {
    let th = g.th;
    let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 32.0), Sense::click());
    let _ = id;
    let mut changed = false;
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }
    let p = ui.painter_at(rect.expand(1.0));
    boxed(&p, rect, if *on { th.accent } else { shade(th.input, 0.9) }, th, th.outline);
    let k = 22.0;
    let x = if *on { rect.max.x - th.bw - 3.0 - k } else { rect.min.x + th.bw + 3.0 };
    let kr = Rect::from_min_size(pos2(x, rect.center().y - k / 2.0), vec2(k, k));
    boxed(&p, kr, th.knob, &Theme { bw: 2.0, ..th.clone() }, th.outline);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    changed
}

/// Gros bouton coloré (préréglages). `header` s'affiche au-dessus du bouton.
pub fn button(ui: &mut Ui, g: &Gfx, size: Vec2, text: &str, fill: Color32, ink: Color32, header: Option<&str>) -> Response {
    let th = g.th;
    let hh = if header.is_some() { 24.0 } else { 0.0 };
    let (full, _) = ui.allocate_exact_size(vec2(size.x, size.y + hh), Sense::hover());
    let rect = Rect::from_min_max(pos2(full.min.x, full.min.y + hh), full.max);
    let resp = ui.interact(rect, ui.id().with(("btn", text, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let p = ui.painter_at(full.expand(2.0));
    if let Some(h) = header {
        ui.painter().text(pos2(rect.center().x, full.min.y + hh / 2.0), Align2::CENTER_CENTER, g.cap(h), g.head(20.0), th.ink);
    }
    let f = if resp.is_pointer_button_down_on() { shade(fill, 0.85) } else if resp.hovered() { shade(fill, 1.08) } else { fill };
    let r = if resp.is_pointer_button_down_on() { rect.translate(vec2(0.0, 2.0)) } else { rect };
    boxed(&p, r, f, th, th.outline);
    let t = g.cap(text);
    let f = fit(&p, &t, g.head(20.0), r.width() - 14.0);
    p.with_clip_rect(r).text(r.center(), Align2::CENTER_CENTER, t, f, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

pub fn chip(ui: &mut Ui, g: &Gfx, text: &str, on: bool, w: f32) -> Response {
    let th = g.th;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
    let p = ui.painter_at(rect.expand(1.0));
    let fill = if on { th.accent } else { shade(th.input, 0.92) };
    let fill = if resp.hovered() { shade(fill, 1.1) } else { fill };
    boxed(&p, rect, fill, &Theme { bw: 2.0, ..th.clone() }, th.outline);
    let ink = if on { if th.accent.r() as u32 + th.accent.g() as u32 + th.accent.b() as u32 > 380 { hex(0x20140a) } else { Color32::WHITE } } else { th.input_ink };
    let f = fit(&p, text, g.body(18.0), rect.width() - 12.0);
    p.with_clip_rect(rect.shrink(3.0)).text(rect.center(), Align2::CENTER_CENTER, text, f, ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

pub fn tab(ui: &mut Ui, g: &Gfx, size: Vec2, icon: &str, text: &str, sel: bool) -> Response {
    let th = g.th;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter_at(rect.expand(2.0));
    let fill = if sel { th.tab_sel } else if resp.hovered() { shade(th.tab, 1.1) } else { th.tab };
    boxed(&p, rect, fill, th, th.outline);
    g.icons.paint(&p, icon, Rect::from_center_size(pos2(rect.center().x, rect.min.y + size.y * 0.37), vec2(size.y * 0.5, size.y * 0.5)));
    let t = g.cap(text);
    let f = fit(&p, &t, g.head(18.0), size.x - 14.0);
    p.with_clip_rect(rect.shrink(2.0)).text(pos2(rect.center().x, rect.max.y - size.y * 0.17), Align2::CENTER_CENTER, t, f, th.tab_ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// Gros bouton d'action du bas : icône + libellé + bandeau sombre.
pub fn action(ui: &mut Ui, g: &Gfx, size: Vec2, icon: &str, big: &str, small: &str, fill: Color32, ink: Color32, enabled: bool) -> Response {
    let th = g.th;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = if enabled { resp } else { resp.on_hover_cursor(CursorIcon::NotAllowed) };
    let p = ui.painter_at(rect.expand(2.0));
    let fill = if !enabled { shade(fill, 0.55) } else if resp.is_pointer_button_down_on() { shade(fill, 0.88) } else if resp.hovered() { shade(fill, 1.07) } else { fill };
    boxed(&p, rect, fill, th, th.outline);
    let band_h = 26.0_f32.min(size.y * 0.3);
    let band = Rect::from_min_max(pos2(rect.min.x + th.bw, rect.max.y - th.bw - band_h), pos2(rect.max.x - th.bw, rect.max.y - th.bw));
    p.rect_filled(band, CornerRadius::ZERO, shade(fill, 0.55));
    let top = Rect::from_min_max(rect.min, pos2(rect.max.x, band.min.y));
    g.icons.paint(&p, icon, Rect::from_center_size(pos2(top.center().x, top.min.y + 6.0 + 18.0), vec2(36.0, 36.0)));
    let big = g.cap(big);
    let bf = fit(&p, &big, g.head(20.0), size.x - 16.0);
    p.with_clip_rect(rect.shrink(3.0)).text(pos2(top.center().x, top.max.y - 15.0), Align2::CENTER_CENTER, big, bf, ink);
    let sf = fit(&p, small, g.body(20.0), size.x - 14.0);
    p.with_clip_rect(rect.shrink(3.0)).text(band.center(), Align2::CENTER_CENTER, small, sf, hex(0xfff4e0));
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

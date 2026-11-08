use crate::def::{self, Cat, Field, Game, Kind};
use crate::detect::{self, Found};
use crate::doc::{self, Doc, Scalar};
use crate::gfx::*;
use crate::theme::{hex, shade, Theme};
use eframe::egui::{self, *};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

const BUILTIN: [&str; 5] = [
    include_str!("../games/stardewvalley.sve"),
    include_str!("../games/celeste.sve"),
    include_str!("../games/hollowknight.sve"),
    include_str!("../games/geometrydash.sve"),
    include_str!("../games/minecraft.sve"),
];
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Settings {
    pub scale: f64,
    pub backup: bool,
}

pub struct Session {
    pub game: usize,
    pub path: PathBuf,
    pub doc: Box<dyn Doc>,
    pub dirty: bool,
    pub cat: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Home,
    Saves(usize),
    Editor,
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Load,
    Refresh,
    Restore,
    Save,
    Home,
}

pub struct App {
    games: Vec<Game>,
    themes: Vec<Theme>,
    home: Theme,
    icons: Icons,
    screen: Screen,
    search: String,
    scans: Vec<Vec<Found>>,
    sess: Option<Session>,
    st: Settings,
    toast: Option<(String, bool, Instant)>,
    manual: String,
    styled: String,
    pending: Option<Act>,
    open_arg: Option<(PathBuf, usize)>,
}

pub fn install_fonts(ctx: &Context) {
    let mut fd = FontDefinitions::empty();
    let data: [(&str, &'static [u8]); 8] = [
        ("pixelify", include_bytes!("../assets/fonts/pixelify.ttf")),
        ("pixelify-bold", include_bytes!("../assets/fonts/pixelify-bold.ttf")),
        ("vt323", include_bytes!("../assets/fonts/vt323.ttf")),
        ("trajan", include_bytes!("../assets/fonts/trajan.ttf")),
        ("trajan-bold", include_bytes!("../assets/fonts/trajan-bold.otf")),
        ("celeste", include_bytes!("../assets/fonts/celeste.otf")),
        ("celeste-bold", include_bytes!("../assets/fonts/celeste-bold.otf")),
        ("blocks", include_bytes!("../assets/fonts/blocks.ttf")),
    ];
    for (n, b) in data {
        fd.font_data.insert(n.to_string(), std::sync::Arc::new(FontData::from_static(b)));
    }
    let pairs = [("pixelify", "pixelify", "pixelify-bold"), ("vt323", "vt323", "vt323"), ("trajan", "trajan", "trajan-bold"), ("celeste", "celeste", "celeste-bold"), ("blocks", "blocks", "blocks")];
    for (name, body, head) in pairs {
        let mk = |f: &str| if f == "pixelify" { vec![f.to_string()] } else { vec![f.to_string(), "pixelify".to_string()] };
        fd.families.insert(FontFamily::Name(format!("b:{name}").into()), mk(body));
        fd.families.insert(FontFamily::Name(format!("h:{name}").into()), mk(head));
    }
    fd.families.insert(FontFamily::Proportional, vec!["pixelify".into()]);
    fd.families.insert(FontFamily::Monospace, vec!["pixelify".into()]);
    ctx.set_fonts(fd);
}

fn load_games() -> Vec<Game> {
    let mut v: Vec<Game> = BUILTIN.iter().filter_map(|s| def::parse(s).map_err(|e| eprintln!("définition intégrée invalide : {e}")).ok()).collect();
    // définitions externes : ./games/*.sve (ou à côté de l'exécutable) remplacent/ajoutent un jeu
    let mut dirs = vec![PathBuf::from("games")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("games"));
        }
    }
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.path().extension().map_or(false, |x| x == "sve") {
                    match std::fs::read_to_string(e.path()).map_err(|e| e.to_string()).and_then(|s| def::parse(&s)) {
                        Ok(g) => match v.iter().position(|x| x.id == g.id) {
                            Some(i) => v[i] = g,
                            None => v.push(g),
                        },
                        Err(err) => eprintln!("{} ignoré : {err}", e.path().display()),
                    }
                }
            }
        }
    }
    v
}

fn rd(env_doc: &dyn Doc, st: &Settings, p: &str) -> Option<Scalar> {
    match p {
        "@scale" => Some(Scalar::Float(st.scale)),
        "@backup" => Some(Scalar::Bool(st.backup)),
        _ => env_doc.get(p),
    }
}

struct Env<'a> {
    doc: &'a mut dyn Doc,
    st: &'a mut Settings,
    dirty: &'a mut bool,
    err: &'a mut Option<String>,
}

impl Env<'_> {
    fn get(&self, p: &str) -> Option<Scalar> {
        rd(&*self.doc, self.st, p)
    }
    fn put(&mut self, p: &str, v: Scalar) {
        match p {
            "@scale" => self.st.scale = v.num().unwrap_or(1.0).clamp(0.7, 2.0),
            "@backup" => self.st.backup = v.flag().unwrap_or(true),
            _ => match self.doc.set(p, v) {
                Ok(()) => *self.dirty = true,
                Err(e) => *self.err = Some(e),
            },
        }
    }
}

fn write_num(env: &mut Env, f: &Field, path: &str, disp: f64) {
    let raw = disp * f.scale + f.off;
    let orig = env.get(path).map(|s| s.text()).unwrap_or_default();
    let v = if !orig.contains('.') && !orig.contains('e') && !orig.contains('E') { Scalar::Int(raw.round() as i64) } else { Scalar::Float(raw) };
    env.put(path, v);
}

fn apply_also(env: &mut Env, f: &Field, same: &Scalar) {
    for (p, v) in &f.also {
        let s = if v == "=" { same.clone() } else { Scalar::Text(v.clone()) };
        env.put(p, s);
    }
}

fn template(doc: &dyn Doc, tpl: &str) -> String {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(a) = rest.find('{') {
        out.push_str(&rest[..a]);
        let Some(b) = rest[a..].find('}') else { break };
        let key = &rest[a + 1..a + b];
        let (path, cap) = match key.strip_suffix(":c") {
            Some(p) => (p, true),
            None => (key, false),
        };
        let val = doc.get(path).map(|s| s.text()).unwrap_or_else(|| "?".into());
        if cap {
            let mut c = val.chars();
            if let Some(f) = c.next() {
                out.push_str(&(f.to_uppercase().collect::<String>() + c.as_str()));
            }
        } else {
            out.push_str(&val);
        }
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

fn matches_game(doc: &dyn Doc, g: &Game) -> bool {
    for c in &g.cats {
        for s in &c.subs {
            for f in &s.fields {
                if f.path.starts_with('@') {
                    continue;
                }
                let ok = match f.kind {
                    Kind::List => !doc.expand(&f.path).is_empty(),
                    Kind::Grid => doc.get(&f.path.replace('%', &f.range.0.to_string())).is_some(),
                    _ => doc.get(&f.path).is_some(),
                };
                if ok {
                    return true;
                }
            }
        }
    }
    false
}

fn ago(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let s = SystemTime::now().duration_since(t).map(|d| d.as_secs()).unwrap_or(0);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

fn human_size(n: u64) -> String {
    if n >= 1 << 20 { format!("{:.1} MB", n as f64 / (1 << 20) as f64) } else if n >= 1024 { format!("{:.0} KB", n as f64 / 1024.0) } else { format!("{n} B") }
}

impl App {
    pub fn new(cc: &eframe::CreationContext, open: Option<PathBuf>, page: usize) -> App {
        install_fonts(&cc.egui_ctx);
        let games = load_games();
        let themes = games.iter().map(|g| Theme::by_name(&g.theme)).collect();
        let scans = games.iter().map(|g| detect::scan(&g.patterns)).collect();
        let mut app = App {
            games,
            themes,
            home: Theme::by_name("home"),
            icons: Icons::new(&cc.egui_ctx),
            screen: Screen::Home,
            search: String::new(),
            scans,
            sess: None,
            st: Settings { scale: 1.0, backup: true },
            toast: None,
            manual: String::new(),
            styled: String::new(),
            pending: None,
            open_arg: open.map(|p| (p, page)),
        };
        if let Some((p, pg)) = app.open_arg.take() {
            app.open_any(&p);
            if let Some(s) = app.sess.as_mut() {
                s.cat = pg.min(app.games[s.game].cats.len().saturating_sub(1));
            }
        }
        app
    }

    fn say(&mut self, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), ok, Instant::now()));
    }

    fn load(&mut self, game: usize, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("lecture impossible : {e}"))?;
        let g = &self.games[game];
        let doc = doc::open(&g.codec, &bytes).map_err(|e| format!("Not a readable {} save ({e})", g.name))?;
        if !matches_game(&*doc, g) {
            return Err(format!("This file does not look like a {} save", g.name));
        }
        self.sess = Some(Session { game, path: path.to_path_buf(), doc, dirty: false, cat: 0 });
        self.screen = Screen::Editor;
        Ok(())
    }

    fn open_any(&mut self, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.say(format!("Cannot read {} : {e}", path.display()), false),
        };
        for (i, g) in self.games.iter().enumerate() {
            if let Ok(d) = doc::open(&g.codec, &bytes) {
                if matches_game(&*d, g) {
                    return match self.load(i, path) {
                        Ok(()) => self.say(format!("Loaded {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), true),
                        Err(e) => self.say(e, false),
                    };
                }
            }
        }
        self.say("No supported game recognises this file", false);
    }

    fn backup_dir(path: &Path) -> PathBuf {
        path.parent().unwrap_or(Path::new(".")).join("savestudio-backups")
    }

    fn do_save(&mut self) {
        let Some(s) = self.sess.as_mut() else { return };
        let bytes = match s.doc.encode() {
            Ok(b) => b,
            Err(e) => return self.say(format!("Encode failed: {e}"), false),
        };
        if self.st.backup && s.path.exists() {
            let dir = Self::backup_dir(&s.path);
            let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let name = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(&s.path, dir.join(format!("{name}.{stamp}.bak"))).map(|_| ())) {
                return self.say(format!("Backup failed, nothing written: {e}"), false);
            }
        }
        let tmp = s.path.with_extension("savestudio-tmp");
        let r = std::fs::write(&tmp, &bytes).and_then(|_| std::fs::rename(&tmp, &s.path));
        match r {
            Ok(()) => {
                s.dirty = false;
                let n = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.say(format!("Saved {n} ({})", human_size(bytes.len() as u64)), true);
            }
            Err(e) => self.say(format!("Write failed: {e}"), false),
        }
    }

    fn reload(&mut self) {
        let Some(s) = self.sess.as_ref() else { return };
        let (g, p, cat) = (s.game, s.path.clone(), s.cat);
        match self.load(g, &p) {
            Ok(()) => {
                if let Some(s) = self.sess.as_mut() {
                    s.cat = cat;
                }
                self.say("Reloaded from disk", true)
            }
            Err(e) => self.say(e, false),
        }
    }

    fn restore(&mut self) {
        let Some(s) = self.sess.as_ref() else { return };
        let dir = Self::backup_dir(&s.path);
        let name = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut best: Option<(u64, PathBuf)> = None;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if let Some(ts) = f.strip_prefix(&format!("{name}.")).and_then(|r| r.strip_suffix(".bak")).and_then(|t| t.parse::<u64>().ok()) {
                    if best.as_ref().map_or(true, |(b, _)| ts > *b) {
                        best = Some((ts, e.path()));
                    }
                }
            }
        }
        match best {
            None => self.say("No backup found for this save", false),
            Some((_, b)) => match std::fs::copy(&b, &s.path) {
                Ok(_) => {
                    self.reload();
                    self.say("Last backup restored", true);
                }
                Err(e) => self.say(format!("Restore failed: {e}"), false),
            },
        }
    }

    fn act(&mut self, a: Act) {
        match a {
            Act::Load => self.screen = Screen::Saves(self.sess.as_ref().map(|s| s.game).unwrap_or(0)),
            Act::Refresh => {
                self.reload();
                for (i, g) in self.games.iter().enumerate() {
                    self.scans[i] = detect::scan(&g.patterns);
                }
            }
            Act::Restore => self.restore(),
            Act::Save => self.do_save(),
            Act::Home => {
                self.sess = None;
                self.screen = Screen::Home;
            }
        }
    }

    fn style_for(&mut self, ctx: &Context, key: &str, th: &Theme, font: &str) {
        // Android : la mise en page « bureau » (~760 pt de large) est réduite pour tenir dans l'écran
        let fit = if cfg!(target_os = "android") { (ctx.screen_rect().width() * ctx.zoom_factor() / 760.0).clamp(0.4, 1.0) } else { 1.0 };
        let k = format!("{key}:{}:{fit:.2}", self.st.scale);
        if self.styled != k {
            let g = Gfx { th, icons: &self.icons, font };
            apply_style(ctx, &g);
            ctx.set_zoom_factor(self.st.scale as f32 * fit);
            self.styled = k;
        }
    }

    fn toast_ui(&mut self, ctx: &Context, th: &Theme, font: &str) {
        let Some((msg, ok, t)) = self.toast.clone() else { return };
        if t.elapsed() > Duration::from_secs(4) {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let g = Gfx { th, icons: &self.icons, font };
        Area::new(Id::new("toast")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -24.0)).order(Order::Tooltip).show(ctx, |ui| {
            let col = if ok { hex(0x2f8f46) } else { hex(0xc2392b) };
            Frame::new().fill(col).stroke(Stroke::new(th.bw, th.outline)).corner_radius(CornerRadius::same(th.round)).inner_margin(Margin::symmetric(18, 10)).show(ui, |ui| {
                ui.label(RichText::new(msg).font(g.body(20.0)).color(Color32::WHITE));
            });
        });
    }

    // ------------------------------------------------------------------ écrans
    fn home_ui(&mut self, ctx: &Context) {
        let th = self.home.clone();
        self.style_for(ctx, "home", &th, "pixelify");
        let mut open: Option<usize> = None;
        let mut rescan = false;
        let mut manual_open = false;
        let q = self.search.trim().to_lowercase();
        let shown: Vec<usize> = (0..self.games.len()).filter(|&i| q.is_empty() || self.games[i].name.to_lowercase().contains(&q) || self.games[i].id.contains(&q) || self.games[i].short.to_lowercase().contains(&q)).collect();
        CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(28, 20))).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: "pixelify" };
            let w = ui.available_width().min(820.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(w);
                let (r, _) = ui.allocate_exact_size(vec2(w, 74.0), Sense::hover());
                let p = ui.painter_at(r);
                text_shadow(&p, pos2(r.center().x, r.min.y + 30.0), Align2::CENTER_CENTER, "SAVESTUDIO", g.head(46.0), th.title, th.title_shadow, 3.0);
                p.text(pos2(r.center().x, r.max.y - 8.0), Align2::CENTER_CENTER, "Type the name of a game. Its saves are found for you.", g.body(18.0), th.ink_soft);
                ui.add_space(6.0);
                let te = TextEdit::singleline(&mut self.search).hint_text(RichText::new("Game name...").color(th.ink_soft)).font(g.body(26.0)).text_color(th.input_ink).desired_width(w).margin(Margin::symmetric(14, 10));
                let resp = ui.add(te);
                if self.search.is_empty() && self.toast.is_none() {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && shown.len() == 1 {
                    open = Some(shown[0]);
                }
                ui.add_space(14.0);
                ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 110.0).show(ui, |ui| {
                    for &i in &shown {
                        let gm = &self.games[i];
                        let t = &self.themes[i];
                        let gg = Gfx { th: t, icons: &self.icons, font: &gm.font };
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 88.0), Sense::click());
                        let p = ui.painter_at(r.expand(2.0));
                        let fill = if resp.hovered() { shade(t.bg, 1.25) } else { t.bg };
                        boxed(&p, r, fill, t, t.panel_edge);
                        let icon = gm.cats.first().map(|c| c.icon.as_str()).unwrap_or("gear");
                        let ir = Rect::from_center_size(pos2(r.min.x + 52.0, r.center().y), vec2(48.0, 48.0));
                        gg.icons.paint(&p, icon, ir);
                        text_shadow(&p, pos2(r.min.x + 96.0, r.center().y - 12.0), Align2::LEFT_CENTER, &gg.cap(&gm.name), gg.head(30.0), t.title, t.title_shadow, 2.0);
                        let n = self.scans[i].len();
                        let sub = if n == 0 { "no save found yet - drop a file or enter a path".to_string() } else { format!("{n} save{} found - latest {}", if n > 1 { "s" } else { "" }, ago(self.scans[i][0].modified)) };
                        p.text(pos2(r.min.x + 96.0, r.center().y + 22.0), Align2::LEFT_CENTER, sub, g.body(17.0), t.ink_soft);
                        p.text(pos2(r.max.x - 24.0, r.center().y), Align2::RIGHT_CENTER, ">", g.head(34.0), t.title);
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            open = Some(i);
                        }
                        ui.add_space(10.0);
                    }
                    if shown.is_empty() {
                        ui.add_space(20.0);
                        ui.label(RichText::new("No game with that name.").font(g.body(22.0)).color(th.ink_soft));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let te = TextEdit::singleline(&mut self.manual).hint_text(RichText::new("...or paste the path of a save file").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width(w - 282.0).margin(Margin::symmetric(10, 8));
                    if ui.add(te).lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        manual_open = true;
                    }
                    if button(ui, &g, vec2(110.0, 36.0), "OPEN", th.btn[2], th.btn_ink, None).clicked() {
                        manual_open = true;
                    }
                    if button(ui, &g, vec2(110.0, 36.0), "RESCAN", th.btn[1], th.btn_ink, None).clicked() {
                        rescan = true;
                    }
                });
            });
        });
        if rescan {
            for (i, g) in self.games.iter().enumerate() {
                self.scans[i] = detect::scan(&g.patterns);
            }
            self.say("Rescanned save folders", true);
        }
        if manual_open && !self.manual.trim().is_empty() {
            let p = PathBuf::from(self.manual.trim().trim_matches('"'));
            self.open_any(&p);
        }
        if let Some(i) = open {
            if self.scans[i].len() == 1 {
                let p = self.scans[i][0].path.clone();
                if let Err(e) = self.load(i, &p) {
                    self.say(e, false);
                    self.screen = Screen::Saves(i);
                }
            } else {
                self.screen = Screen::Saves(i);
            }
        }
        self.toast_ui(ctx, &th, "pixelify");
    }

    fn saves_ui(&mut self, ctx: &Context, gi: usize) {
        let th = self.themes[gi].clone();
        let font = self.games[gi].font.clone();
        self.style_for(ctx, &format!("g{gi}"), &th, &font);
        let name = self.games[gi].name.clone();
        let short = self.games[gi].short.clone();
        let mut load: Option<PathBuf> = None;
        let mut back = false;
        TopBottomPanel::top("hdr").exact_height(76.0).frame(Frame::new().fill(th.bg)).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            let (r, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            let p = ui.painter_at(r);
            text_shadow(&p, pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, &g.cap(&format!("{short} Save Editor")), g.head(38.0), th.title, th.title_shadow, 3.0);
        });
        TopBottomPanel::bottom("bar").exact_height(92.0).frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(24, 12))).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            ui.horizontal(|ui| {
                if button(ui, &g, vec2(130.0, 54.0), "< BACK", th.btn[0], th.btn_ink, None).clicked() {
                    back = true;
                }
                let te = TextEdit::singleline(&mut self.manual).hint_text(RichText::new("or paste a file path").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width(ui.available_width() - 160.0).margin(Margin::symmetric(10, 12));
                let r = ui.add(te);
                let go = button(ui, &g, vec2(130.0, 54.0), "OPEN", th.btn[2], th.btn_ink, None).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                if go && !self.manual.trim().is_empty() {
                    load = Some(PathBuf::from(self.manual.trim().trim_matches('"')));
                }
            });
        });
        CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(24, 0))).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            let r = ui.available_rect_before_wrap();
            boxed(ui.painter(), r, th.panel, &th, th.panel_edge);
            let inner = r.shrink(th.bw + 14.0);
            ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                ui.label(RichText::new(format!("{name} - detected saves")).font(g.head(26.0)).color(th.ink));
                ui.add_space(8.0);
                let list = self.scans[gi].clone();
                if list.is_empty() {
                    ui.label(RichText::new("Nothing found in the usual folders.").font(g.body(21.0)).color(th.ink_soft));
                    ui.label(RichText::new("Paste the path of your save below, or drop the file on this window.").font(g.body(19.0)).color(th.ink_soft));
                    for p in &self.games[gi].patterns {
                        ui.label(RichText::new(format!("searched: {p}")).font(g.body(15.0)).color(th.ink_soft));
                    }
                }
                scroll_area(ui, &th, |ui| {
                    for f in &list {
                        let (rr, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
                        let p = ui.painter_at(rr.expand(2.0));
                        boxed(&p, rr, if resp.hovered() { shade(th.card, 1.08) } else { th.card }, &th, th.outline);
                        let fname = f.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let dir = f.path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                        let pc = p.with_clip_rect(rr.shrink(4.0));
                        pc.text(pos2(rr.min.x + 16.0, rr.min.y + 20.0), Align2::LEFT_CENTER, &fname, g.head(21.0), th.ink);
                        pc.text(pos2(rr.min.x + 16.0, rr.max.y - 17.0), Align2::LEFT_CENTER, &dir, g.body(14.0), th.ink_soft);
                        pc.text(pos2(rr.max.x - 16.0, rr.min.y + 20.0), Align2::RIGHT_CENTER, ago(f.modified), g.body(17.0), th.ink);
                        pc.text(pos2(rr.max.x - 16.0, rr.max.y - 17.0), Align2::RIGHT_CENTER, human_size(f.size), g.body(15.0), th.ink_soft);
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            load = Some(f.path.clone());
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        });
        if back {
            self.screen = if self.sess.is_some() { Screen::Editor } else { Screen::Home };
        }
        if let Some(p) = load {
            match self.load(gi, &p) {
                Ok(()) => self.say("Save loaded", true),
                Err(e) => self.say(e, false),
            }
        }
        self.toast_ui(ctx, &th, &font);
    }

    fn editor_ui(&mut self, ctx: &Context) {
        let gi = self.sess.as_ref().map(|s| s.game).unwrap_or(0);
        let th = self.themes[gi].clone();
        let font = self.games[gi].font.clone();
        self.style_for(ctx, &format!("g{gi}"), &th, &font);
        let mut act: Option<Act> = None;
        let mut err: Option<String> = None;
        {
            let App { games, icons, sess, st, .. } = self;
            let game = &games[gi];
            let s = sess.as_mut().unwrap();
            let g = Gfx { th: &th, icons, font: &font };
            let title = g.cap(&format!("{} Save Editor", game.short));

            TopBottomPanel::top("hdr").exact_height(78.0).frame(Frame::new().fill(th.bg)).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
                let p = ui.painter_at(r);
                let tr = p.text(pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, "", g.head(40.0), th.title);
                let _ = tr;
                let gal = ui.painter().layout_no_wrap(title.clone(), g.head(40.0), th.title);
                let tw = gal.size().x;
                text_shadow(&p, pos2(r.min.x + 24.0, r.center().y - 2.0), Align2::LEFT_CENTER, &title, g.head(40.0), th.title, th.title_shadow, 3.0);
                p.text(pos2(r.min.x + 24.0 + tw + 14.0, r.center().y + 6.0), Align2::LEFT_CENTER, format!("v{VERSION}"), g.body(21.0), shade(th.title, 0.85));
                let mut x = r.max.x - 28.0;
                for (ic, tip, go) in [("leaf", "Back to the game list", true), ("hammer", "", false), ("coin", "", false)] {
                    let ir = Rect::from_center_size(pos2(x - 18.0, r.center().y), vec2(40.0, 40.0));
                    let resp = ui.interact(ir, Id::new(("hdr", ic)), Sense::click());
                    g.icons.paint(&p, ic, ir.expand(if resp.hovered() && go { 3.0 } else { 0.0 }));
                    if go {
                        let resp = resp.on_hover_text(tip);
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            act = Some(Act::Home);
                        }
                    }
                    x -= 56.0;
                }
            });

            TopBottomPanel::bottom("act").exact_height(144.0).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 24, right: 24, top: 8, bottom: 6 })).show(ctx, |ui| {
                let gap = 14.0;
                let w = (ui.available_width() - gap * 3.0) / 4.0;
                let mut ui_h = ui.new_child(UiBuilder::new().max_rect(ui.available_rect_before_wrap()).layout(Layout::left_to_right(Align::TOP)));
                ui_h.spacing_mut().item_spacing.x = gap;
                let btns: [(&str, &str, &str, Color32, Act); 4] = [
                    ("folder", "Load save", "Load another...", th.btn[0], Act::Load),
                    ("refresh", "Refresh save", "Reload from disk", th.btn[1], Act::Refresh),
                    ("restore", "Backups", "Restore last backup", th.btn[2], Act::Restore),
                    ("disk", "Save changes", if s.dirty { "Unsaved changes" } else { "Write to disk" }, th.btn[3], Act::Save),
                ];
                for (ic, big, small, col, a) in btns {
                    let ink = if a == Act::Save && th.btn[3].g() > 200 { hex(0x2a1a06) } else { th.btn_ink };
                    if action(&mut ui_h, &g, vec2(w, 100.0), ic, big, small, col, ink, true).clicked() {
                        act = Some(a);
                    }
                }
                let r = ui.max_rect();
                let comps: Vec<String> = s.path.components().rev().take(3).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                let short_path = format!(".../{}", comps.into_iter().rev().collect::<Vec<_>>().join("/"));
                let ft = format!("SaveStudio {VERSION}{}", if s.dirty { " *" } else { "" });
                let rw = g.text_w(ui.painter(), &ft, g.body(15.0)) + 8.0;
                let pl = ui.painter().with_clip_rect(Rect::from_min_max(r.min, pos2(r.max.x - rw, r.max.y)));
                pl.text(pos2(r.min.x, r.max.y - 2.0), Align2::LEFT_BOTTOM, short_path, g.body(15.0), th.ink_soft);
                ui.painter().text(pos2(r.max.x, r.max.y - 2.0), Align2::RIGHT_BOTTOM, ft, g.body(15.0), th.ink_soft);
            });

            SidePanel::left("tabs").exact_width(196.0).resizable(false).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 20, right: 6, top: 4, bottom: 8 })).show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                scroll_area(ui, &th, |ui| {
                    for (i, c) in game.cats.iter().enumerate() {
                        if tab(ui, &g, vec2(ui.available_width().min(166.0), 98.0), &c.icon, &c.name, s.cat == i).clicked() {
                            s.cat = i;
                        }
                    }
                });
            });

            CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 6, right: 24, top: 4, bottom: 4 })).show(ctx, |ui| {
                let r = ui.available_rect_before_wrap();
                boxed(ui.painter(), r, th.panel, &th, th.panel_edge);
                let inner = r.shrink(th.bw + 12.0);
                let cat: &Cat = &game.cats[s.cat.min(game.cats.len() - 1)];
                let mut env = Env { doc: &mut *s.doc, st, dirty: &mut s.dirty, err: &mut err };
                ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                    scroll_area(ui, &th, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        let l1 = template(&*env.doc, &game.l1);
                        let l2 = template(&*env.doc, &game.l2);
                        if !l1.is_empty() {
                            ui.label(RichText::new(g.cap(&l1)).font(g.head(25.0)).color(th.ink));
                        }
                        if !l2.is_empty() {
                            ui.label(RichText::new(&l2).font(g.body(23.0)).color(th.ink));
                        }
                        ui.add_space(8.0);
                        for (si, sub) in cat.subs.iter().enumerate() {
                            let fill = if si % 2 == 0 { th.card } else { th.card_alt };
                            Frame::new().fill(fill).stroke(Stroke::new(th.bw, th.outline)).corner_radius(CornerRadius::same(th.round)).inner_margin(Margin::same(14)).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                let (tr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
                                let p = ui.painter_at(tr);
                                g.icons.paint(&p, &sub.icon, Rect::from_min_size(tr.min, vec2(36.0, 36.0)));
                                p.text(pos2(tr.min.x + 46.0, tr.center().y), Align2::LEFT_CENTER, g.cap(&sub.name), g.head(28.0), th.ink);
                                ui.add_space(4.0);
                                sub_fields(ui, &g, sub, &mut env, Id::new(("sub", gi, s.cat, si)));
                            });
                            ui.add_space(12.0);
                        }
                    });
                });
            });
        }
        if let Some(e) = err {
            self.say(e, false);
        }
        if let Some(a) = act {
            if a == Act::Home && self.sess.as_ref().map_or(false, |s| s.dirty) {
                self.pending = Some(Act::Home);
            } else if a == Act::Refresh && self.sess.as_ref().map_or(false, |s| s.dirty) {
                self.pending = Some(Act::Refresh);
            } else {
                self.act(a);
            }
        }
        if let Some(pa) = self.pending {
            let mut choice: Option<bool> = None;
            Window::new("confirm").title_bar(false).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).frame(Frame::new().fill(th.panel).stroke(Stroke::new(th.bw, th.outline)).inner_margin(Margin::same(18))).show(ctx, |ui| {
                let g = Gfx { th: &th, icons: &self.icons, font: &font };
                ui.label(RichText::new("Discard unsaved changes?").font(g.head(26.0)).color(th.ink));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if button(ui, &g, vec2(150.0, 46.0), "DISCARD", th.btn[0], th.btn_ink, None).clicked() {
                        choice = Some(true);
                    }
                    if button(ui, &g, vec2(150.0, 46.0), "KEEP EDITING", th.btn[2], th.btn_ink, None).clicked() {
                        choice = Some(false);
                    }
                });
            });
            if let Some(c) = choice {
                self.pending = None;
                if c {
                    if let Some(s) = self.sess.as_mut() {
                        s.dirty = false;
                    }
                    self.act(pa);
                }
            }
        }
        self.toast_ui(ctx, &th, &font);
    }
}

/// ScrollArea verticale avec barre de défilement aux couleurs du thème (le contenu garde le style normal).
fn scroll_area(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui)) {
    let base = ui.visuals().clone();
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        v.extreme_bg_color = shade(th.panel_edge, 0.55);
        let handle = shade(th.panel_edge, 1.1);
        v.widgets.inactive.bg_fill = handle;
        v.widgets.hovered.bg_fill = shade(handle, 1.15);
        v.widgets.active.bg_fill = shade(handle, 1.3);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            *ui.visuals_mut() = base;
            add(ui)
        });
    });
}

// --------------------------------------------------------------------- champs
fn simple(f: &Field) -> bool {
    matches!(f.kind, Kind::Num | Kind::Text | Kind::Bool | Kind::Enum | Kind::Slider | Kind::Info) && f.btns.is_empty()
}

fn sub_fields(ui: &mut Ui, g: &Gfx, sub: &def::Sub, env: &mut Env, id: Id) {
    let total = ui.available_width();
    let cols = sub.cols.max(1);
    let gap = 18.0;
    let pnt = ui.painter().clone();
    let lab_w = |f: &Field| g.text_w(&pnt, &format!("{}:", g.cap(&f.label)), g.head(20.0));
    let lw_all = (sub.fields.iter().filter(|f| !matches!(f.kind, Kind::List | Kind::Grid)).map(|f| lab_w(f)).fold(0.0, f32::max) + 16.0).clamp(110.0, total * 0.45);
    let mut i = 0;
    while i < sub.fields.len() {
        let f = &sub.fields[i];
        if cols > 1 && simple(f) {
            let mut group = vec![i];
            while group.len() < cols && i + group.len() < sub.fields.len() && simple(&sub.fields[i + group.len()]) {
                group.push(i + group.len());
            }
            let cw = (total - gap * (cols as f32 - 1.0)) / cols as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for &k in &group {
                    // un interrupteur n'a besoin que de ~76 px : le libellé peut prendre presque toute la cellule
                    let room = if sub.fields[k].kind == Kind::Bool { cw - 76.0 } else { cw * 0.5 };
                    let lw = (lab_w(&sub.fields[k]) + 14.0).min(room);
                    ui.allocate_ui_with_layout(vec2(cw, ROW_H), Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        field(ui, g, &sub.fields[k], env, cw, id.with(k), true, lw);
                    });
                }
            });
            i += group.len();
        } else {
            field(ui, g, f, env, total, id.with(i), false, lw_all);
            i += 1;
        }
        ui.add_space(4.0);
    }
}

fn label(ui: &mut Ui, g: &Gfx, text: &str, w: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
    let s = if text.is_empty() { String::new() } else { format!("{}:", g.cap(text)) };
    let p = ui.painter_at(r);
    let f = fit(&p, &s, g.head(20.0), w - 2.0);
    p.text(pos2(r.min.x, r.center().y), Align2::LEFT_CENTER, s, f, g.th.ink);
}

fn field(ui: &mut Ui, g: &Gfx, f: &Field, env: &mut Env, w: f32, id: Id, cell: bool, lw: f32) {
    let th = g.th;
    match f.kind {
        Kind::Num | Kind::Info => {
            let Some(raw) = env.get(&f.path).and_then(|s| s.num()) else { return unavailable(ui, g, f, lw) };
            let mut disp = ((raw - f.off) / f.scale).clamp(f.min, f.max);
            let ro = f.ro || f.kind == Kind::Info;
            let (min, max) = (f.min, f.max);
            let has_btn = !f.btns.is_empty();
            let has_head = !f.bh.is_empty();
            let gap = 8.0;
            let avail = (w - lw - gap).max(if cell { 120.0 } else { 200.0 });
            let sw = if cell { avail } else if has_btn { (avail * 0.42).clamp(150.0, 230.0) } else { (avail * 0.55).clamp(190.0, 300.0) };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let h = if has_head { ROW_H + 24.0 } else { ROW_H };
                ui.allocate_ui_with_layout(vec2(lw, h), Layout::left_to_right(Align::Center), |ui| label(ui, g, &f.label, lw));
                let before = disp;
                ui.vertical(|ui| {
                    if has_head {
                        ui.add_space(24.0);
                    }
                    if spin(ui, g, &mut disp, min, max, f.step, f.dp, &f.unit, sw, ro, id) && disp != before {
                        write_num(env, f, &f.path, disp);
                        if !f.xp.is_empty() {
                            if let Some(&x) = f.xt.get(disp.round().max(0.0) as usize) {
                                env.put(&f.xp, Scalar::Int(x as i64));
                            }
                        }
                    }
                });
                if has_btn {
                    let rest = avail - sw - gap;
                    let n = f.btns.len() as f32;
                    let bw = ((rest - gap * (n - 1.0)) / n).max(60.0);
                    for (bi, (bl, bv)) in f.btns.iter().enumerate() {
                        let v: f64 = bv.parse().unwrap_or(0.0);
                        let text = if bl.is_empty() { format!("{}{}", thousands(v, f.dp), if f.unit.is_empty() { String::new() } else { format!(" {}", f.unit) }) } else { bl.clone() };
                        let hdr = if bi == 0 && has_head { Some(f.bh.as_str()) } else { None };
                        let col = if th.preset == th.panel { th.btn[2] } else { th.preset };
                        if button(ui, g, vec2(bw, ROW_H - 4.0), &text, col, if th.preset.r() > 200 { th.ink } else { th.btn_ink }, hdr).clicked() {
                            if (disp - v).abs() > f64::EPSILON {
                                write_num(env, f, &f.path, v);
                                if !f.xp.is_empty() {
                                    if let Some(&x) = f.xt.get(v.round().max(0.0) as usize) {
                                        env.put(&f.xp, Scalar::Int(x as i64));
                                    }
                                }
                            }
                            let same = Scalar::Float(v * f.scale + f.off);
                            apply_also(env, f, &same);
                        }
                    }
                }
            });
        }
        Kind::Bool => {
            let Some(mut on) = env.get(&f.path).and_then(|s| s.flag()) else { return unavailable(ui, g, f, lw) };
            ui.horizontal(|ui| {
                label(ui, g, &f.label, lw.max(if cell { 0.0 } else { 0.0 }));
                if toggle(ui, g, &mut on, id) {
                    env.put(&f.path, Scalar::Bool(on));
                    apply_also(env, f, &Scalar::Bool(on));
                }
            });
        }
        Kind::Text => {
            let Some(cur) = env.get(&f.path).map(|s| s.text()) else { return unavailable(ui, g, f, lw) };
            let mut s = cur.clone();
            ui.horizontal(|ui| {
                label(ui, g, &f.label, lw);
                let te = TextEdit::singleline(&mut s).font(g.body(21.0)).text_color(th.input_ink).desired_width(if cell { w - lw - 8.0 } else { ((w - lw) * 0.6).clamp(200.0, 380.0) }).margin(Margin::symmetric(10, 7));
                ui.add(te);
            });
            if s != cur {
                env.put(&f.path, Scalar::Text(s));
            }
        }
        Kind::Enum => {
            let Some(cur) = env.get(&f.path).map(|s| s.text()) else { return unavailable(ui, g, f, lw) };
            let shown = f.opts.iter().find(|(_, v)| v.eq_ignore_ascii_case(&cur) || (v.parse::<f64>().ok().is_some() && v.parse::<f64>().ok() == cur.parse::<f64>().ok())).map(|(l, _)| l.clone()).unwrap_or(cur.clone());
            ui.horizontal(|ui| {
                label(ui, g, &f.label, lw);
                ComboBox::from_id_salt(id).width(if cell { w - lw - 8.0 } else { ((w - lw) * 0.5).clamp(180.0, 300.0) }).selected_text(RichText::new(shown).font(g.body(21.0)).color(th.input_ink)).show_ui(ui, |ui| {
                    for (l, v) in &f.opts {
                        if ui.selectable_label(v.eq_ignore_ascii_case(&cur), RichText::new(l).font(g.body(20.0)).color(th.input_ink)).clicked() {
                            env.put(&f.path, Scalar::Text(v.clone()));
                        }
                    }
                });
            });
        }
        Kind::Slider => {
            let Some(raw) = env.get(&f.path).and_then(|s| s.num()) else { return unavailable(ui, g, f, lw) };
            let (min, max) = (if f.min.is_finite() { f.min } else { 0.0 }, if f.max.is_finite() { f.max } else { 100.0 });
            let mut v = (raw - f.off) / f.scale;
            let before = v;
            ui.horizontal(|ui| {
                label(ui, g, &f.label, lw);
                ui.spacing_mut().slider_width = if cell { (w - lw - 90.0).max(80.0) } else { 260.0 };
                ui.add(Slider::new(&mut v, min..=max).step_by(f.step.min(if f.dp > 0 { 10f64.powi(-(f.dp as i32)) } else { 1.0 })).fixed_decimals(f.dp));
            });
            if v != before {
                write_num(env, f, &f.path, v);
            }
        }
        Kind::Grid => {
            let (a, b) = f.range;
            ui.horizontal(|ui| {
                label(ui, g, &f.label, w - 330.0);
                for (txt, val) in [("All on", true), ("All off", false)] {
                    if button(ui, g, vec2(140.0, 34.0), txt, th.btn[if val { 2 } else { 0 }], th.btn_ink, None).clicked() {
                        for i in a..=b {
                            let p = f.path.replace('%', &i.to_string());
                            if env.get(&p).is_some() {
                                env.put(&p, Scalar::Bool(val));
                            }
                        }
                    }
                }
            });
            let cw = 210.0_f32;
            let cols = ((w + 8.0) / (cw + 8.0)).floor().max(1.0) as i32;
            let cw = (w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
            let mut i = a;
            while i <= b {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                    for _ in 0..cols {
                        if i > b {
                            break;
                        }
                        let p = f.path.replace('%', &i.to_string());
                        let name = f.names.get((i - a) as usize).cloned().unwrap_or_else(|| i.to_string());
                        match env.get(&p).and_then(|s| s.flag()) {
                            Some(on) => {
                                if chip(ui, g, &name, on, cw).clicked() {
                                    env.put(&p, Scalar::Bool(!on));
                                }
                            }
                            None => {
                                ui.add_enabled(false, Label::new(RichText::new(name).font(g.body(16.0))));
                            }
                        }
                        i += 1;
                    }
                });
            }
        }
        Kind::List => {
            let rows = env.doc.expand(&f.path);
            ui.horizontal(|ui| {
                label(ui, g, &f.label, (w - 40.0 - 230.0 * f.btns.len() as f32).max(120.0));
                for (bl, bv) in &f.btns {
                    let v: f64 = bv.parse().unwrap_or(0.0);
                    if button(ui, g, vec2(220.0, 34.0), if bl.is_empty() { bv } else { bl }, if th.preset == th.panel { th.btn[2] } else { th.preset }, if th.preset.r() > 200 { th.ink } else { th.btn_ink }, None).clicked() {
                        for r in &rows {
                            let vp = format!("{r}/{}", f.val_path);
                            if env.get(&vp).is_some() {
                                write_num(env, f, &vp, v);
                            }
                        }
                    }
                }
            });
            let mut shown = 0;
            for (ri, r) in rows.iter().enumerate() {
                let Some(name) = env.get(&format!("{r}/{}", f.name_path)).map(|s| s.text()) else { continue };
                let vp = format!("{r}/{}", f.val_path);
                shown += 1;
                ui.horizontal(|ui| {
                    let (lr, _) = ui.allocate_exact_size(vec2((w - 250.0).max(100.0), ROW_H - 4.0), Sense::hover());
                    ui.painter_at(lr).text(pos2(lr.min.x + 4.0, lr.center().y), Align2::LEFT_CENTER, pretty(&name), g.body(21.0), th.ink);
                    if let Some(raw) = env.get(&vp).and_then(|s| s.num()) {
                        let mut disp = (raw - f.off) / f.scale;
                        let before = disp;
                        if spin(ui, g, &mut disp, f.min, f.max, f.step, f.dp, &f.unit, 210.0, false, id.with(ri)) && disp != before {
                            write_num(env, f, &vp, disp);
                        }
                    }
                });
            }
            if shown == 0 {
                ui.label(RichText::new("(empty)").font(g.body(19.0)).color(th.ink_soft));
            }
        }
    }
}

fn unavailable(ui: &mut Ui, g: &Gfx, f: &Field, lw: f32) {
    ui.horizontal(|ui| {
        label(ui, g, &f.label, lw);
        let (r, _) = ui.allocate_exact_size(vec2(120.0, ROW_H - 4.0), Sense::hover());
        let p = ui.painter_at(r);
        let f = fit(&p, "not in save", g.body(16.0), r.width());
        p.text(r.left_center(), Align2::LEFT_CENTER, "not in save", f, g.th.ink_soft);
    });
}

impl App {
    pub fn frame(&mut self, ctx: &Context, _f: &mut eframe::Frame) {
        // glisser-déposer d'un fichier de sauvegarde
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|d| d.path.clone()).collect());
        if let Some(p) = dropped.first() {
            self.open_any(p);
        }
        match self.screen {
            Screen::Home => self.home_ui(ctx),
            Screen::Saves(g) => self.saves_ui(ctx, g),
            Screen::Editor => {
                if self.sess.is_none() {
                    self.screen = Screen::Home;
                } else {
                    self.editor_ui(ctx);
                }
            }
        }
    }
}

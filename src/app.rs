use crate::def::{self, Cat, Field, Game, Kind};
use crate::detect::{self, Found};
use crate::doc::{Doc, Scalar};
use crate::fonts;
use crate::fsutil::{latest_backup, write_safely};
use crate::plugin::{self, Plugin};
use crate::gfx::*;
use crate::pack::{self, FontSpec, Pack};
use crate::settings::{self, ActPos, Density, Settings, SysFont, TabsPos};
use crate::theme::{hex, shade, Theme};
use crate::updater::{self, Msg, Remote};
use crate::Launch;
use eframe::egui::*;
use std::cmp::Ordering;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

mod sync_ui;
use sync_ui::SyncUi;

const VERSION: &str = pack::APP_VERSION;

// ------------------------------------------------------------------ historique (Ctrl+Z)

struct Entry {
    /// (chemin, valeur à rétablir)
    changes: Vec<(String, Option<Scalar>)>,
    at: Instant,
}

/// Annuler / rétablir. Toutes les écritures d'une même image forment une seule étape ; les modifications
/// rapprochées du même champ (glisser une valeur) sont fusionnées pour qu'un Ctrl+Z revienne à la valeur d'avant le geste.
#[derive(Default)]
pub struct History {
    cur: Vec<(String, Option<Scalar>)>,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

fn same_paths(a: &[(String, Option<Scalar>)], b: &[(String, Option<Scalar>)]) -> bool {
    a.len() == b.len() && a.iter().all(|(p, _)| b.iter().any(|(q, _)| q == p))
}

impl History {
    fn record(&mut self, p: &str, old: Option<Scalar>) {
        if !self.cur.iter().any(|(q, _)| q == p) {
            self.cur.push((p.to_string(), old));
        }
    }

    fn commit(&mut self, now: Instant) {
        if self.cur.is_empty() {
            return;
        }
        let ch = std::mem::take(&mut self.cur);
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            if now.saturating_duration_since(last.at) < Duration::from_millis(700) && same_paths(&last.changes, &ch) {
                last.at = now;
                return;
            }
        }
        self.undo.push(Entry { changes: ch, at: now });
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
    }
}

pub struct Session {
    pub game: usize,
    pub path: PathBuf,
    pub doc: Box<dyn Doc>,
    pub dirty: bool,
    pub cat: usize,
    /// fichier compagnon (`mirror=` du jeu), recopié à chaque modification
    pub mirror: Option<(PathBuf, Box<dyn Doc>)>,
    pub hist: History,
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Home,
    Saves(usize),
    Editor,
    Settings,
    Sync,
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Load,
    Refresh,
    Restore,
    Save,
    Home,
    Settings,
    Sync,
}

/// État de la mise à jour des éditeurs de jeux.
enum Upd {
    Idle,
    Checking(Receiver<Result<Remote, String>>),
    /// nouvelle version disponible : on demande à l'utilisateur
    Offer(Remote),
    Busy { rx: Receiver<Msg>, done: usize, total: usize },
    Error(String),
    TooOld(String),
}

pub struct App {
    games: Vec<Game>,
    themes: Vec<Theme>,
    pack_themes: Vec<(String, Theme)>,
    pack_fonts: Vec<FontSpec>,
    pack_icons: Option<String>,
    pack_version: Option<String>,
    pack_updated: String,
    local_pack: Option<PathBuf>,
    /// code du pack qui comprend les sauvegardes (WebAssembly)
    codecs: Option<Rc<RefCell<Plugin>>>,
    sys_fonts: Vec<SysFont>,
    defined_fonts: Vec<String>,
    /// polices installées pendant cette image : egui ne les connaît qu'à l'image suivante
    fonts_pending: Option<Vec<String>>,
    fonts_live: bool,
    fonts_key: String,
    font_filter: String,
    icons: Icons,
    screen: Screen,
    settings_from: Screen,
    search: String,
    scans: Vec<Vec<Found>>,
    sess: Option<Session>,
    st: Settings,
    st_saved: Settings,
    toast: Option<(String, bool, Instant)>,
    manual: String,
    styled: String,
    pending: Option<Act>,
    open_arg: Option<(PathBuf, usize)>,
    upd: Upd,
    manual_check: bool,
    sync: SyncUi,
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
    /// fichier compagnon : (document, préfixe source, préfixe destination)
    mirror: Option<(&'a mut dyn Doc, String, String)>,
    rec: Option<&'a mut History>,
}

fn strip_path<'a>(p: &'a str, pre: &str) -> Option<&'a str> {
    if p == pre {
        Some("")
    } else {
        p.strip_prefix(pre).and_then(|r| r.strip_prefix('/'))
    }
}

impl Env<'_> {
    fn get(&self, p: &str) -> Option<Scalar> {
        rd(&*self.doc, self.st, p)
    }
    fn put(&mut self, p: &str, v: Scalar) {
        match p {
            "@scale" => self.st.scale = v.num().unwrap_or(1.0).clamp(0.7, 2.0),
            "@backup" => self.st.backup = v.flag().unwrap_or(true),
            _ => {
                if let Some(h) = self.rec.as_mut() {
                    h.record(p, self.doc.get(p));
                }
                match self.doc.set(p, v.clone()) {
                    Ok(()) => {
                        *self.dirty = true;
                        if let Some((md, src, dst)) = self.mirror.as_mut() {
                            if let Some(rest) = strip_path(p, src) {
                                let q = if rest.is_empty() { dst.clone() } else { format!("{dst}/{rest}") };
                                if md.get(&q).is_some() {
                                    let _ = md.set(&q, v);
                                }
                            }
                        }
                    }
                    Err(e) => *self.err = Some(e),
                }
            }
        }
    }
}

fn write_num(env: &mut Env, f: &Field, path: &str, disp: f64) {
    let raw = disp * f.scale + f.off;
    let orig = env.get(path).map(|s| s.text()).unwrap_or_default();
    let v = if !orig.contains('.') && !orig.contains('e') && !orig.contains('E') { Scalar::Int(raw.round() as i64) } else { Scalar::Float(raw) };
    env.put(path, v.clone());
    if path == f.path {
        apply_sync(env, f, &v, None);
    }
}

/// `sync=chemin` : la même valeur brute est écrite ailleurs ; `sync=chemin:#` : indice de l'option choisie d'une liste.
fn apply_sync(env: &mut Env, f: &Field, v: &Scalar, idx: Option<usize>) {
    for (p, by_index) in &f.sync {
        if *by_index {
            if let Some(i) = idx {
                env.put(p, Scalar::Int(i as i64));
            }
        } else {
            env.put(p, v.clone());
        }
    }
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
    pub fn new(cc: &eframe::CreationContext, launch: Launch) -> App {
        let st = Settings::load();
        let mut app = App {
            games: vec![],
            themes: vec![],
            pack_themes: vec![],
            pack_fonts: vec![],
            pack_icons: None,
            pack_version: None,
            pack_updated: String::new(),
            local_pack: launch.pack.clone(),
            codecs: None,
            sys_fonts: settings::system_fonts(),
            defined_fonts: vec!["pixelify".into()],
            fonts_pending: None,
            fonts_live: false,
            fonts_key: String::new(),
            font_filter: String::new(),
            icons: Icons::new(&cc.egui_ctx, None),
            screen: Screen::Home,
            settings_from: Screen::Home,
            search: String::new(),
            scans: vec![],
            sess: None,
            st_saved: st.clone(),
            st,
            toast: None,
            manual: String::new(),
            styled: String::new(),
            pending: None,
            open_arg: launch.open.map(|p| (p, launch.page)),
            upd: Upd::Idle,
            manual_check: false,
            sync: SyncUi::default(),
        };
        app.sync_fonts(&cc.egui_ctx);
        match launch.pack {
            // pack local (essais, développement d'un jeu) : pas de téléchargement ni de mise à jour
            Some(dir) => match pack::load(&dir) {
                Ok(p) => app.apply_pack(&cc.egui_ctx, p),
                Err(e) => app.say(e, false),
            },
            None => {
                if let Ok(p) = pack::load(&pack::pack_dir()) {
                    app.apply_pack(&cc.egui_ctx, p);
                }
                // au démarrage : y a-t-il de nouveaux éditeurs de jeux ?
                if app.pack_version.is_none() || app.st.auto_update {
                    app.start_check(&cc.egui_ctx, false);
                }
            }
        }
        if let Some((p, pg)) = app.open_arg.take() {
            app.open_any(&p);
            if let Some(s) = app.sess.as_mut() {
                s.cat = pg.min(app.games[s.game].cats.len().saturating_sub(1));
            }
        }
        app
    }

    // ------------------------------------------------------------------ pack et mises à jour

    fn open_doc(&self, codec: &str, bytes: &[u8]) -> Result<Box<dyn Doc>, String> {
        match &self.codecs {
            Some(p) => plugin::open(p, codec, bytes),
            None => Err("The game pack has no save codecs - update the game editors".into()),
        }
    }

    fn theme_named(&self, n: &str) -> Theme {
        self.pack_themes.iter().find(|(k, _)| k == n).map(|(_, t)| t.clone()).unwrap_or_else(|| Theme::builtin(if n == "home" { "home" } else { "wood" }))
    }

    fn apply_pack(&mut self, ctx: &Context, p: Pack) {
        let old_id = self.sess.as_ref().map(|s| self.games[s.game].id.clone());
        self.pack_version = Some(p.version.clone());
        self.pack_updated = p.updated;
        self.pack_themes = p.themes;
        self.pack_fonts = p.fonts;
        self.pack_icons = p.icons;
        self.codecs = None;
        if let Some(f) = &p.codecs {
            match std::fs::read(f).map_err(|e| format!("codecs : {e}")).and_then(|b| Plugin::load(&b)) {
                Ok(pl) => self.codecs = Some(Rc::new(RefCell::new(pl))),
                Err(e) => self.say(e, false),
            }
        }
        self.games = p.games;
        self.themes = self.games.iter().map(|g| self.theme_named(&g.theme)).collect();
        self.scans = self.games.iter().map(|g| detect::scan(&g.patterns)).collect();
        self.icons = Icons::new(ctx, self.pack_icons.as_deref());
        self.fonts_key.clear();
        self.sync_fonts(ctx);
        self.styled.clear();
        if let Some(id) = old_id {
            match self.games.iter().position(|g| g.id == id) {
                Some(i) => {
                    if let Some(s) = self.sess.as_mut() {
                        s.game = i;
                    }
                }
                None => {
                    self.sess = None;
                    self.screen = Screen::Home;
                }
            }
        }
        if matches!(self.screen, Screen::Saves(_)) {
            self.screen = if self.sess.is_some() { Screen::Editor } else { Screen::Home };
        }
    }

    fn start_check(&mut self, ctx: &Context, manual: bool) {
        if self.local_pack.is_some() {
            if manual {
                self.say("A local pack is in use (--pack): updates are disabled", false);
            }
            return;
        }
        if matches!(self.upd, Upd::Checking(_) | Upd::Busy { .. }) {
            return;
        }
        self.manual_check = manual;
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(updater::fetch_manifest(pack::REPO_RAW));
            c.request_repaint();
        });
        self.upd = Upd::Checking(rx);
    }

    fn start_install(&mut self, ctx: &Context, remote: Remote) {
        let (tx, rx) = channel();
        let c = ctx.clone();
        let total = remote.manifest.files.len();
        std::thread::spawn(move || {
            let r = updater::install(pack::REPO_RAW, &remote, &pack::pack_dir(), &tx).map(|_| remote.manifest.version.clone());
            let _ = tx.send(Msg::Done(r));
            c.request_repaint();
        });
        self.upd = Upd::Busy { rx, done: 0, total };
    }

    fn poll_update(&mut self, ctx: &Context) {
        match std::mem::replace(&mut self.upd, Upd::Idle) {
            Upd::Checking(rx) => match rx.try_recv() {
                Err(TryRecvError::Empty) => {
                    self.upd = Upd::Checking(rx);
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
                Ok(Err(e)) => {
                    // sans éditeurs installés on doit le dire ; sinon on reste discret (hors ligne = normal)
                    if self.pack_version.is_none() {
                        self.upd = Upd::Error(e);
                    } else if self.manual_check {
                        self.say(format!("Update check failed: {e}"), false);
                    }
                }
                Ok(Ok(remote)) => {
                    let newer = match &self.pack_version {
                        None => true,
                        Some(v) => pack::cmp_version(&remote.manifest.version, v) == Ordering::Greater,
                    };
                    if newer && pack::cmp_version(&remote.manifest.min_app, VERSION) == Ordering::Greater {
                        self.upd = Upd::TooOld(remote.manifest.min_app.clone());
                    } else if newer {
                        self.upd = Upd::Offer(remote);
                    } else if self.manual_check {
                        self.say(format!("Game editors are up to date (v{})", remote.manifest.version), true);
                    }
                }
            },
            Upd::Busy { rx, mut done, mut total } => {
                let mut fin: Option<Result<String, String>> = None;
                loop {
                    match rx.try_recv() {
                        Ok(Msg::Progress(a, b)) => {
                            done = a;
                            total = b;
                        }
                        Ok(Msg::Done(r)) => {
                            fin = Some(r);
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            fin = Some(Err("the download stopped unexpectedly".into()));
                            break;
                        }
                    }
                }
                match fin {
                    None => {
                        self.upd = Upd::Busy { rx, done, total };
                        ctx.request_repaint_after(Duration::from_millis(100));
                    }
                    Some(Ok(v)) => match pack::load(&pack::pack_dir()) {
                        Ok(p) => {
                            self.apply_pack(ctx, p);
                            self.say(format!("Game editors updated to v{v}"), true);
                        }
                        Err(e) => self.upd = Upd::Error(e),
                    },
                    Some(Err(e)) => self.upd = Upd::Error(e),
                }
            }
            other => self.upd = other,
        }
    }

    fn modal_open(&self) -> bool {
        self.pending.is_some() || self.sync.confirm_open() || !matches!(self.upd, Upd::Idle | Upd::Checking(_))
    }

    // ------------------------------------------------------------------ polices, thèmes, réglages

    fn sync_fonts(&mut self, ctx: &Context) {
        // egui n'applique `set_fonts` qu'à l'image suivante : utiliser tout de suite une nouvelle famille ferait planter
        if let Some(d) = self.fonts_pending.take() {
            self.defined_fonts = d;
            self.styled.clear();
        }
        let mut wanted: Vec<String> = [&self.st.title_font, &self.st.settings_font, &self.st.editor_font].iter().filter(|k| k.starts_with("sys:")).map(|k| k.to_string()).collect();
        wanted.sort();
        wanted.dedup();
        let key = format!("{}|{}", wanted.join(","), self.pack_version.as_deref().unwrap_or(""));
        if key != self.fonts_key {
            let d = fonts::install(ctx, &self.pack_fonts, &self.sys_fonts, &wanted);
            if self.fonts_live {
                self.fonts_pending = Some(d);
                ctx.request_repaint();
            } else {
                self.defined_fonts = d;
            }
            self.fonts_key = key;
            self.styled.clear();
        }
    }

    /// Une police absente (supprimée du pack ou du système) retombe sur Pixelify.
    fn font_key(&self, k: &str) -> String {
        if self.defined_fonts.iter().any(|d| d == k) { k.to_string() } else { "pixelify".to_string() }
    }

    fn title_theme(&self) -> Theme {
        self.st.themed(&self.theme_named(&self.st.title_theme))
    }

    fn game_theme(&self, gi: usize) -> Theme {
        self.st.themed(&self.themes[gi])
    }

    fn game_font(&self, gi: usize) -> String {
        if self.st.editor_font.is_empty() { self.font_key(&self.games[gi].font) } else { self.font_key(&self.st.editor_font) }
    }

    fn say(&mut self, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), ok, Instant::now()));
    }

    // ------------------------------------------------------------------ sauvegardes

    fn load(&mut self, game: usize, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("lecture impossible : {e}"))?;
        let g = &self.games[game];
        let doc = self.open_doc(&g.codec, &bytes).map_err(|e| format!("Not a readable {} save ({e})", g.name))?;
        if !matches_game(&*doc, g) {
            return Err(format!("This file does not look like a {} save", g.name));
        }
        // fichier compagnon (ex. Stardew : SaveGameInfo, lu par le menu « Charger »)
        let mirror = g.mirror.as_ref().and_then(|m| {
            let p = path.parent()?.join(&m.file);
            let b = std::fs::read(&p).ok()?;
            self.open_doc(&g.codec, &b).ok().map(|d| (p, d))
        });
        self.sess = Some(Session { game, path: path.to_path_buf(), doc, dirty: false, cat: 0, mirror, hist: History::default() });
        self.screen = Screen::Editor;
        Ok(())
    }

    fn open_any(&mut self, path: &Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.say(format!("Cannot read {} : {e}", path.display()), false),
        };
        for (i, g) in self.games.iter().enumerate() {
            if let Ok(d) = self.open_doc(&g.codec, &bytes) {
                if matches_game(&*d, g) {
                    return match self.load(i, path) {
                        Ok(()) => self.say(format!("Loaded {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), true),
                        Err(e) => self.say(e, false),
                    };
                }
            }
        }
        if self.games.is_empty() {
            self.say("No game editors installed yet - download them first", false);
        } else {
            self.say("No supported game recognises this file", false);
        }
    }

    fn do_save(&mut self) {
        let backup = self.st.backup;
        let Some(s) = self.sess.as_mut() else { return };
        let bytes = match s.doc.encode() {
            Ok(b) => b,
            Err(e) => return self.say(format!("Encode failed: {e}"), false),
        };
        let mirror_bytes = match s.mirror.as_ref().map(|(_, d)| d.encode()) {
            Some(Ok(b)) => Some(b),
            Some(Err(e)) => return self.say(format!("Encode failed: {e}"), false),
            None => None,
        };
        let mut res = write_safely(&s.path, &bytes, backup);
        if let (Ok(()), Some(b), Some((p, _))) = (&res, &mirror_bytes, &s.mirror) {
            res = write_safely(p, b, backup);
        }
        match res {
            Ok(()) => {
                s.dirty = false;
                let n = s.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.say(format!("Saved {n} ({})", human_size(bytes.len() as u64)), true);
            }
            Err(e) => self.say(e, false),
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
        let main = s.path.clone();
        let mirror = s.mirror.as_ref().map(|(p, _)| p.clone());
        let Some(b) = latest_backup(&main) else { return self.say("No backup found for this save", false) };
        if let Err(e) = std::fs::copy(&b, &main) {
            return self.say(format!("Restore failed: {e}"), false);
        }
        if let Some(mp) = mirror {
            if let Some(mb) = latest_backup(&mp) {
                let _ = std::fs::copy(mb, mp);
            }
        }
        self.reload();
        self.say("Last backup restored", true);
    }

    fn undo_redo(&mut self, redo: bool) {
        let Some(s) = self.sess.as_mut() else { return };
        let entry = if redo { s.hist.redo.pop() } else { s.hist.undo.pop() };
        let Some(entry) = entry else {
            return self.say(if redo { "Nothing to redo" } else { "Nothing to undo" }, false);
        };
        let rule = self.games[s.game].mirror.clone();
        let mut inv = vec![];
        let mut err = None;
        {
            let mut env = Env {
                doc: &mut *s.doc,
                st: &mut self.st,
                dirty: &mut s.dirty,
                err: &mut err,
                mirror: match (s.mirror.as_mut(), rule) {
                    (Some((_, d)), Some(r)) => Some((&mut **d, r.src, r.dst)),
                    _ => None,
                },
                rec: None,
            };
            for (p, v) in entry.changes.iter().rev() {
                inv.push((p.clone(), env.get(p)));
                if let Some(v) = v {
                    env.put(p, v.clone());
                }
            }
        }
        inv.reverse();
        let e = Entry { changes: inv, at: Instant::now() - Duration::from_secs(60) };
        let n = entry.changes.len();
        if redo {
            s.hist.undo.push(e);
        } else {
            s.hist.redo.push(e);
        }
        s.dirty = true;
        let (u, r) = (s.hist.undo.len(), s.hist.redo.len());
        match err {
            Some(e) => self.say(e, false),
            None => self.say(format!("{} {n} value{} ({u} to undo, {r} to redo)", if redo { "Redone:" } else { "Undone:" }, if n > 1 { "s" } else { "" }), true),
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
            Act::Sync => {
                if self.screen != Screen::Sync {
                    self.settings_from = self.screen;
                }
                self.screen = Screen::Sync;
            }
            Act::Settings => {
                if self.screen != Screen::Settings {
                    self.settings_from = self.screen;
                }
                self.screen = Screen::Settings;
            }
        }
    }

    /// Échap : retour à l'écran précédent.
    fn go_back(&mut self) {
        match self.screen {
            Screen::Settings => {
                self.screen = match self.settings_from {
                    Screen::Editor if self.sess.is_some() => Screen::Editor,
                    Screen::Saves(g) if g < self.games.len() => Screen::Saves(g),
                    _ => Screen::Home,
                }
            }
            Screen::Sync => self.screen = Screen::Home,
            Screen::Saves(_) => self.screen = if self.sess.is_some() { Screen::Editor } else { Screen::Home },
            Screen::Editor => {
                if self.sess.as_ref().map_or(false, |s| s.dirty) {
                    self.pending = Some(Act::Home);
                } else {
                    self.act(Act::Home);
                }
            }
            Screen::Home => {
                if !self.search.is_empty() {
                    self.search.clear();
                }
            }
        }
    }

    fn style_for(&mut self, ctx: &Context, key: &str, th: &Theme, font: &str) {
        // Android : la mise en page « bureau » (~760 pt de large) est réduite pour tenir dans l'écran
        let fit = if cfg!(target_os = "android") { (ctx.screen_rect().width() * ctx.zoom_factor() / 760.0).clamp(0.4, 1.0) } else { 1.0 };
        let k = format!("{key}|{font}|{:?}|{}|{fit:.2}", (th.bg, th.panel, th.accent, th.title, th.fs, th.round, th.caps, th.bw, th.ink), self.st.scale);
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
        let th = self.title_theme();
        let font = self.font_key(&self.st.title_font);
        self.style_for(ctx, "home", &th, &font);
        let mut open: Option<usize> = None;
        let mut rescan = false;
        let mut manual_open = false;
        let mut to_settings = false;
        let mut to_sync = false;
        let mut check = false;
        let q = self.search.trim().to_lowercase();
        let shown: Vec<usize> = (0..self.games.len()).filter(|&i| q.is_empty() || self.games[i].name.to_lowercase().contains(&q) || self.games[i].id.contains(&q) || self.games[i].short.to_lowercase().contains(&q)).collect();
        let card_fonts: Vec<String> = self.games.iter().map(|g| self.font_key(&g.font)).collect();
        let status = match (&self.upd, &self.pack_version) {
            (Upd::Checking(_), _) => "checking for game editor updates...".to_string(),
            (_, Some(v)) => format!("Game editors v{v}{}", if self.sync.running() { "  -  sharing saves over Tailscale" } else { "" }),
            (_, None) => "Game editors: not installed".to_string(),
        };
        let none_yet = self.games.is_empty();
        CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(28, 20))).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            let w = ui.available_width().min(820.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(w);
                let (r, _) = ui.allocate_exact_size(vec2(w, 74.0), Sense::hover());
                let p = ui.painter_at(r);
                text_shadow(&p, pos2(r.center().x, r.min.y + 30.0), Align2::CENTER_CENTER, &g.cap("SaveStudio"), g.head(46.0), th.title, th.title_shadow, 3.0);
                let sub = if none_yet { "No game editors installed yet." } else { "Type the name of a game. Its saves are found for you." };
                let sf = fit(&p, sub, g.body(18.0), w);
                p.text(pos2(r.center().x, r.max.y - 8.0), Align2::CENTER_CENTER, sub, sf, th.ink_soft);
                ui.add_space(6.0);
                let te = TextEdit::singleline(&mut self.search).hint_text(RichText::new("Game name...").color(th.ink_soft)).font(g.body(26.0)).text_color(th.input_ink).desired_width(w).margin(Margin::symmetric(14, 10));
                let resp = ui.add(te);
                if self.search.is_empty() && self.toast.is_none() && !none_yet {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && shown.len() == 1 {
                    open = Some(shown[0]);
                }
                ui.add_space(14.0);
                ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 130.0).show(ui, |ui| {
                    if none_yet {
                        ui.add_space(10.0);
                        let (r, _) = ui.allocate_exact_size(vec2(w, 120.0), Sense::hover());
                        boxed(&ui.painter_at(r.expand(2.0)), r, th.card, &th, th.panel_edge);
                        ui.painter().text(pos2(r.center().x, r.min.y + 34.0), Align2::CENTER_CENTER, "The game editors are downloaded from GitHub.", g.body(20.0), th.ink);
                        let br = Rect::from_center_size(pos2(r.center().x, r.max.y - 38.0), vec2(220.0, 44.0));
                        ui.scope_builder(UiBuilder::new().max_rect(br), |ui| {
                            if button(ui, &g, vec2(220.0, 44.0), "DOWNLOAD", th.btn[2], th.btn_ink, None).clicked() {
                                check = true;
                            }
                        });
                    }
                    for &i in &shown {
                        let gm = &self.games[i];
                        let t = &self.themes[i];
                        let gg = Gfx { th: t, icons: &self.icons, font: &card_fonts[i] };
                        let (r, resp) = ui.allocate_exact_size(vec2(w, 88.0), Sense::click());
                        let p = ui.painter_at(r.expand(2.0));
                        let fill = if resp.hovered() { shade(t.bg, 1.25) } else { t.bg };
                        boxed(&p, r, fill, t, t.panel_edge);
                        let icon = gm.cats.first().map(|c| c.icon.as_str()).unwrap_or("gear");
                        let ir = Rect::from_center_size(pos2(r.min.x + 52.0, r.center().y), vec2(48.0, 48.0));
                        gg.icons.paint(&p, icon, ir);
                        let store = gm.stores.first().map(|s| format!("{} {}", s.name, s.version)).unwrap_or_default();
                        let sw = if store.is_empty() { 0.0 } else { g.text_w(&p, &store, g.body(15.0)) + 12.0 };
                        let nm = gg.cap(&gm.name);
                        let nf = fit(&p, &nm, gg.head(30.0), r.width() - 96.0 - 60.0 - sw);
                        text_shadow(&p, pos2(r.min.x + 96.0, r.center().y - 12.0), Align2::LEFT_CENTER, &nm, nf, t.title, t.title_shadow, 2.0);
                        let n = self.scans[i].len();
                        let sub = if n == 0 { "no save found yet - drop a file or enter a path".to_string() } else { format!("{n} save{} found - latest {}", if n > 1 { "s" } else { "" }, ago(self.scans[i][0].modified)) };
                        let sf = fit(&p, &sub, g.body(17.0), r.width() - 96.0 - 60.0);
                        p.text(pos2(r.min.x + 96.0, r.center().y + 22.0), Align2::LEFT_CENTER, sub, sf, t.ink_soft);
                        if !store.is_empty() {
                            p.text(pos2(r.max.x - 56.0, r.center().y - 12.0), Align2::RIGHT_CENTER, &store, g.body(15.0), t.ink_soft);
                        }
                        p.text(pos2(r.max.x - 24.0, r.center().y), Align2::RIGHT_CENTER, ">", g.head(34.0), t.title);
                        if resp.hovered() {
                            ctx.set_cursor_icon(CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            open = Some(i);
                        }
                        ui.add_space(10.0);
                    }
                    if shown.is_empty() && !none_yet {
                        ui.add_space(20.0);
                        ui.label(RichText::new("No game with that name.").font(g.body(22.0)).color(th.ink_soft));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let te = TextEdit::singleline(&mut self.manual).hint_text(RichText::new("...or paste the path of a save file").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width((w - 505.0).max(120.0)).margin(Margin::symmetric(10, 8));
                    if ui.add(te).lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        manual_open = true;
                    }
                    if button(ui, &g, vec2(90.0, 36.0), "OPEN", th.btn[2], th.btn_ink, None).clicked() {
                        manual_open = true;
                    }
                    if button(ui, &g, vec2(100.0, 36.0), "RESCAN", th.btn[1], th.btn_ink, None).clicked() {
                        rescan = true;
                    }
                    if button(ui, &g, vec2(90.0, 36.0), "SYNC", th.btn[3], hex(0x2a1a06), None).clicked() {
                        to_sync = true;
                    }
                    if button(ui, &g, vec2(130.0, 36.0), "SETTINGS", th.btn[0], th.btn_ink, None).clicked() {
                        to_settings = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(status).font(g.body(15.0)).color(th.ink_soft));
            });
        });
        if rescan {
            for (i, g) in self.games.iter().enumerate() {
                self.scans[i] = detect::scan(&g.patterns);
            }
            self.say("Rescanned save folders", true);
        }
        if check {
            self.start_check(ctx, true);
        }
        if to_settings {
            self.act(Act::Settings);
        }
        if to_sync {
            self.act(Act::Sync);
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
        self.toast_ui(ctx, &th, &font);
    }

    fn saves_ui(&mut self, ctx: &Context, gi: usize) {
        let th = self.game_theme(gi);
        let font = self.game_font(gi);
        self.style_for(ctx, &format!("g{gi}"), &th, &font);
        let name = self.games[gi].name.clone();
        let short = self.games[gi].short.clone();
        let mut load: Option<PathBuf> = None;
        let mut back = false;
        TopBottomPanel::top("hdr").exact_height(76.0).frame(Frame::new().fill(th.bg)).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            let (r, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            let p = ui.painter_at(r);
            let t = g.cap(&format!("{short} Save Editor"));
            let f = fit(&p, &t, g.head(38.0), r.width() - 48.0);
            text_shadow(&p, pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, &t, f, th.title, th.title_shadow, 3.0);
        });
        TopBottomPanel::bottom("bar").exact_height(92.0).frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(24, 12))).show(ctx, |ui| {
            let g = Gfx { th: &th, icons: &self.icons, font: &font };
            ui.horizontal(|ui| {
                if back_button(ui, &g, vec2(150.0, 54.0), "BACK", th.btn[0], th.btn_ink).clicked() {
                    back = true;
                }
                let te = TextEdit::singleline(&mut self.manual).hint_text(RichText::new("or paste a file path").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width((ui.available_width() - 160.0).max(100.0)).margin(Margin::symmetric(10, 12));
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
            self.go_back();
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
        let th = self.game_theme(gi);
        let font = self.game_font(gi);
        self.style_for(ctx, &format!("g{gi}"), &th, &font);
        let (tabs_pos, act_pos, dens, force_cols) = (self.st.tabs, self.st.actions, self.st.density.factor(), self.st.cols);
        let mut act: Option<Act> = None;
        let mut err: Option<String> = None;
        {
            let App { games, icons, sess, st, .. } = self;
            let game = &games[gi];
            let s = sess.as_mut().unwrap();
            let g = Gfx { th: &th, icons, font: &font };
            let title = g.cap(&format!("{} Save Editor", game.short));

            // ---- en-tête : bouton RETOUR toujours visible (même pour Geometry Dash et Minecraft), titre, réglages
            TopBottomPanel::top("hdr").exact_height(78.0).frame(Frame::new().fill(th.bg)).show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0);
                    if back_button(ui, &g, vec2(130.0, 46.0), "BACK", th.btn[0], th.btn_ink).clicked() {
                        act = Some(Act::Home);
                    }
                    ui.add_space(6.0);
                    let rest = (ui.available_width() - 150.0 - 28.0).max(120.0);
                    let (tr, _) = ui.allocate_exact_size(vec2(rest, 70.0), Sense::hover());
                    let p = ui.painter_at(tr);
                    let ver = format!("v{VERSION}");
                    let vw = g.text_w(&p, &ver, g.body(21.0)) + 14.0;
                    let tf = fit(&p, &title, g.head(40.0), (tr.width() - vw).max(60.0));
                    let tw = g.text_w(&p, &title, tf.clone());
                    text_shadow(&p, pos2(tr.min.x, tr.center().y - 2.0), Align2::LEFT_CENTER, &title, tf, th.title, th.title_shadow, 3.0);
                    if tr.width() > tw + vw {
                        p.text(pos2(tr.min.x + tw + 14.0, tr.center().y + 6.0), Align2::LEFT_CENTER, ver, g.body(21.0), shade(th.title, 0.85));
                    }
                    if button(ui, &g, vec2(150.0, 46.0), "SETTINGS", th.btn[1], th.btn_ink, None).clicked() {
                        act = Some(Act::Settings);
                    }
                });
            });

            // ---- barre d'actions (en bas ou en haut selon les réglages)
            let bar = |ui: &mut Ui, act: &mut Option<Act>| action_bar(ui, &g, s.dirty, &s.path, act);
            match act_pos {
                ActPos::Bottom => {
                    TopBottomPanel::bottom("act").exact_height(144.0).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 24, right: 24, top: 8, bottom: 6 })).show(ctx, |ui| bar(ui, &mut act));
                }
                ActPos::Top => {
                    TopBottomPanel::top("act").exact_height(144.0).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 24, right: 24, top: 0, bottom: 6 })).show(ctx, |ui| bar(ui, &mut act));
                }
            }

            // ---- onglets (à gauche, à droite ou en haut)
            match tabs_pos {
                TabsPos::Left => {
                    SidePanel::left("tabs").exact_width(196.0).resizable(false).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 20, right: 6, top: 4, bottom: 8 })).show(ctx, |ui| tabs_vertical(ui, &g, &th, game, &mut s.cat));
                }
                TabsPos::Right => {
                    SidePanel::right("tabs").exact_width(196.0).resizable(false).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 6, right: 20, top: 4, bottom: 8 })).show(ctx, |ui| tabs_vertical(ui, &g, &th, game, &mut s.cat));
                }
                TabsPos::Top => {
                    TopBottomPanel::top("tabs").exact_height(84.0).frame(Frame::new().fill(th.bg).inner_margin(Margin { left: 24, right: 24, top: 2, bottom: 6 })).show(ctx, |ui| {
                        ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (i, c) in game.cats.iter().enumerate() {
                                    if tab(ui, &g, vec2(150.0, 66.0), &c.icon, &c.name, s.cat == i).clicked() {
                                        s.cat = i;
                                    }
                                }
                            });
                        });
                    });
                }
            }

            let (lm, rm) = match tabs_pos {
                TabsPos::Left => (6, 24),
                TabsPos::Right => (24, 6),
                TabsPos::Top => (24, 24),
            };
            CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin { left: lm, right: rm, top: 4, bottom: 4 })).show(ctx, |ui| {
                let r = ui.available_rect_before_wrap();
                boxed(ui.painter(), r, th.panel, &th, th.panel_edge);
                let inner = r.shrink(th.bw + 12.0);
                let cat: &Cat = &game.cats[s.cat.min(game.cats.len() - 1)];
                let rule = game.mirror.clone();
                let cat_idx = s.cat;
                let mut env = Env {
                    doc: &mut *s.doc,
                    st,
                    dirty: &mut s.dirty,
                    err: &mut err,
                    mirror: match (s.mirror.as_mut(), rule) {
                        (Some((_, d)), Some(r)) => Some((&mut **d, r.src, r.dst)),
                        _ => None,
                    },
                    rec: Some(&mut s.hist),
                };
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
                        if let Some(v) = version_line(game, &*env.doc) {
                            ui.label(RichText::new(v).font(g.body(16.0)).color(th.ink_soft));
                        }
                        ui.add_space(8.0 * dens);
                        for (si, sub) in cat.subs.iter().enumerate() {
                            let fill = if si % 2 == 0 { th.card } else { th.card_alt };
                            Frame::new().fill(fill).stroke(Stroke::new(th.bw, th.outline)).corner_radius(CornerRadius::same(th.round)).inner_margin(Margin::same((14.0 * dens).round() as i8)).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                let (tr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
                                let p = ui.painter_at(tr);
                                g.icons.paint(&p, &sub.icon, Rect::from_min_size(tr.min, vec2(36.0, 36.0)));
                                let sn = g.cap(&sub.name);
                                let sf = fit(&p, &sn, g.head(28.0), tr.width() - 46.0);
                                p.text(pos2(tr.min.x + 46.0, tr.center().y), Align2::LEFT_CENTER, sn, sf, th.ink);
                                ui.add_space(4.0 * dens);
                                sub_fields(ui, &g, sub, &mut env, Id::new(("sub", gi, cat_idx, si)), force_cols, dens);
                            });
                            ui.add_space(12.0 * dens);
                        }
                    });
                });
            });
            // une étape d'historique par image : Ctrl+Z annule tout ce qui vient d'être changé
            s.hist.commit(Instant::now());
        }
        if let Some(e) = err {
            self.say(e, false);
        }
        if let Some(a) = act {
            let dirty = self.sess.as_ref().map_or(false, |s| s.dirty);
            if (a == Act::Home || a == Act::Refresh) && dirty {
                self.pending = Some(a);
            } else {
                self.act(a);
            }
        }
        self.toast_ui(ctx, &th, &font);
    }

    /// Fenêtre « Abandonner les modifications ? » (Échap = continuer l'édition).
    fn confirm_ui(&mut self, ctx: &Context) {
        let Some(pa) = self.pending else { return };
        let gi = self.sess.as_ref().map(|s| s.game);
        let (th, font) = match gi {
            Some(i) => (self.game_theme(i), self.game_font(i)),
            None => (self.title_theme(), self.font_key(&self.st.title_font)),
        };
        let mut choice: Option<bool> = None;
        let g = Gfx { th: &th, icons: &self.icons, font: &font };
        let m = Modal::new(Id::new("confirm")).frame(Frame::new().fill(th.panel).stroke(Stroke::new(th.bw, th.outline)).inner_margin(Margin::same(18))).show(ctx, |ui| {
            ui.label(RichText::new("Discard unsaved changes?").font(g.head(26.0)).color(th.ink));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if button(ui, &g, vec2(150.0, 46.0), "DISCARD", th.btn[0], th.btn_ink, None).clicked() {
                    choice = Some(true);
                }
                if button(ui, &g, vec2(190.0, 46.0), "KEEP EDITING", th.btn[2], th.btn_ink, None).clicked() {
                    choice = Some(false);
                }
            });
        });
        if m.should_close() && choice.is_none() {
            choice = Some(false);
        }
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

    /// Fenêtres de la mise à jour des éditeurs de jeux.
    fn upd_ui(&mut self, ctx: &Context) {
        if matches!(self.upd, Upd::Idle | Upd::Checking(_)) {
            return;
        }
        let th = self.title_theme();
        let font = self.font_key(&self.st.title_font);
        let first = self.pack_version.is_none();
        // (titre, lignes, progression, bouton principal, bouton secondaire)
        let (title, lines, progress, primary, secondary): (&str, Vec<String>, Option<f32>, Option<&str>, Option<&str>) = match &self.upd {
            Upd::Offer(r) => {
                let n = r.manifest.files.len();
                if first {
                    ("Download the game editors?", vec!["SaveStudio is only the interface. The editor of each game is downloaded from GitHub, so games can be added or fixed without updating the app.".into(), format!("Version {} - {n} files, checked with SHA-256.", r.manifest.version)], None, Some("DOWNLOAD"), Some("LATER"))
                } else {
                    let cur = self.pack_version.clone().unwrap_or_default();
                    ("New game editors available", vec![format!("Version {} is available (you have {cur}).", r.manifest.version), "Download and install the update now?".into()], None, Some("UPDATE"), Some("LATER"))
                }
            }
            Upd::Busy { done, total, .. } => ("Downloading game editors...", vec![format!("{done} / {total} files")], Some(if *total == 0 { 0.0 } else { *done as f32 / *total as f32 }), None, None),
            Upd::Error(e) => ("Could not get the game editors", vec![e.clone()], None, Some("RETRY"), Some("CLOSE")),
            Upd::TooOld(min) => ("SaveStudio is too old", vec![format!("These game editors need SaveStudio {min} or newer (this is {VERSION})."), "Download the latest SaveStudio from the GitHub Releases page.".into()], None, None, Some("CLOSE")),
            _ => return,
        };
        let busy = progress.is_some();
        let g = Gfx { th: &th, icons: &self.icons, font: &font };
        let mut clicked: Option<bool> = None; // true = principal, false = secondaire
        let m = Modal::new(Id::new("update")).frame(Frame::new().fill(th.panel).stroke(Stroke::new(th.bw, th.outline)).inner_margin(Margin::same(20))).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(RichText::new(g.cap(title)).font(g.head(26.0)).color(th.ink));
            ui.add_space(8.0);
            for l in &lines {
                ui.label(RichText::new(l).font(g.body(18.0)).color(th.ink));
            }
            if let Some(p) = progress {
                ui.add_space(10.0);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().min(480.0), 26.0), Sense::hover());
                let pt = ui.painter();
                boxed(pt, r, th.input, &th, th.outline);
                let inner = r.shrink(th.bw + 2.0);
                pt.rect_filled(Rect::from_min_size(inner.min, vec2(inner.width() * p.clamp(0.0, 1.0), inner.height())), CornerRadius::ZERO, th.accent);
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if let Some(t) = primary {
                    if button(ui, &g, vec2(170.0, 46.0), t, th.btn[2], th.btn_ink, None).clicked() {
                        clicked = Some(true);
                    }
                }
                if let Some(t) = secondary {
                    if button(ui, &g, vec2(150.0, 46.0), t, th.btn[0], th.btn_ink, None).clicked() {
                        clicked = Some(false);
                    }
                }
            });
        });
        if !busy && m.should_close() && clicked.is_none() {
            clicked = Some(false); // Échap = plus tard / fermer
        }
        match clicked {
            Some(true) => match std::mem::replace(&mut self.upd, Upd::Idle) {
                Upd::Offer(r) => self.start_install(ctx, r),
                Upd::Error(_) => self.start_check(ctx, true),
                o => self.upd = o,
            },
            Some(false) => self.upd = Upd::Idle,
            None => {}
        }
    }

    fn settings_ui(&mut self, ctx: &Context) {
        let th = self.title_theme();
        let font = self.font_key(&self.st.settings_font);
        self.style_for(ctx, "settings", &th, &font);
        let mut back = false;
        let mut check = false;
        let mut reset = false;
        let themes: Vec<String> = std::iter::once("home".to_string()).chain(self.pack_themes.iter().map(|(n, _)| n.clone()).filter(|n| n != "home")).collect();
        let choices = fonts::choices(&self.pack_fonts, &self.sys_fonts);
        let n_sys = self.sys_fonts.len();
        let pack_line = match &self.pack_version {
            Some(v) => format!("Installed: v{v}{}", if self.pack_updated.is_empty() { String::new() } else { format!(" ({})", self.pack_updated) }),
            None => "Installed: none".to_string(),
        };
        let folder = if let Some(p) = &self.local_pack { format!("Local pack: {}", p.display()) } else { format!("Folder: {}", pack::pack_dir().display()) };
        let checking = matches!(self.upd, Upd::Checking(_));
        let codec_line = match &self.codecs {
            Some(c) => format!("Save formats understood: {}", c.borrow().codecs().join(", ")),
            None => "Save formats: none (the game pack has no codecs)".to_string(),
        };
        {
            let App { st, icons, pack_fonts, sys_fonts, font_filter, defined_fonts, .. } = self;
            let g = Gfx { th: &th, icons, font: &font };
            TopBottomPanel::top("hdr").exact_height(78.0).frame(Frame::new().fill(th.bg)).show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0);
                    if back_button(ui, &g, vec2(130.0, 46.0), "BACK", th.btn[0], th.btn_ink).clicked() {
                        back = true;
                    }
                    ui.add_space(10.0);
                    let (tr, _) = ui.allocate_exact_size(vec2(ui.available_width() - 20.0, 70.0), Sense::hover());
                    let p = ui.painter_at(tr);
                    let t = g.cap("Settings");
                    let f = fit(&p, &t, g.head(40.0), tr.width());
                    text_shadow(&p, pos2(tr.min.x, tr.center().y - 2.0), Align2::LEFT_CENTER, &t, f, th.title, th.title_shadow, 3.0);
                });
            });
            CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(24, 4))).show(ctx, |ui| {
                let r = ui.available_rect_before_wrap();
                boxed(ui.painter(), r, th.panel, &th, th.panel_edge);
                ui.scope_builder(UiBuilder::new().max_rect(r.shrink(th.bw + 12.0)), |ui| {
                    scroll_area(ui, &th, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        let lw = 250.0;

                        section(ui, &g, "Fonts", |ui| {
                            ui.label(RichText::new(format!("{} fonts available ({} from the game pack + built-in, {} installed on this computer)", choices.len(), pack_fonts.len().max(1), n_sys)).font(g.body(16.0)).color(th.ink_soft));
                            font_picker(ui, &g, "font_title", "Title screen", &mut st.title_font, None, &choices, sys_fonts, font_filter, defined_fonts, lw);
                            font_picker(ui, &g, "font_settings", "Settings screen", &mut st.settings_font, None, &choices, sys_fonts, font_filter, defined_fonts, lw);
                            font_picker(ui, &g, "font_editor", "Save editor", &mut st.editor_font, Some("Each game's own font"), &choices, sys_fonts, font_filter, defined_fonts, lw);
                        });

                        section(ui, &g, "Look", |ui| {
                            slider_row(ui, &g, "Interface size", lw, |ui| ui.add(Slider::new(&mut st.scale, 0.7..=2.0).step_by(0.05).fixed_decimals(2)));
                            slider_row(ui, &g, "Text size", lw, |ui| ui.add(Slider::new(&mut st.text_scale, 0.7..=1.6).step_by(0.05).fixed_decimals(2)));
                            ui.horizontal(|ui| {
                                label(ui, &g, "Title screen theme", lw);
                                ComboBox::from_id_salt("title_theme").width(220.0).selected_text(RichText::new(pretty(&st.title_theme)).font(g.body(21.0)).color(th.input_ink)).show_ui(ui, |ui| {
                                    for t in &themes {
                                        if ui.selectable_label(*t == st.title_theme, RichText::new(pretty(t)).font(g.body(20.0)).color(th.input_ink)).clicked() {
                                            st.title_theme = t.clone();
                                        }
                                    }
                                });
                            });
                            choice_row(ui, &g, "Letter case", lw, &mut st.caps, &[("Theme", 0u8), ("UPPERCASE", 1), ("Normal", 2)]);
                            choice_row(ui, &g, "Corners", lw, &mut st.corners, &[("Theme", 0u8), ("Square", 1), ("Rounded", 2)]);
                            color_row(ui, &g, "Background", lw, &mut st.ov_bg, hex(0x20232b), "ov_bg");
                            color_row(ui, &g, "Panels", lw, &mut st.ov_panel, hex(0x2a2f3b), "ov_panel");
                            color_row(ui, &g, "Accent", lw, &mut st.ov_accent, hex(0x56ba3c), "ov_accent");
                            color_row(ui, &g, "Title color", lw, &mut st.ov_title, hex(0xf6c64a), "ov_title");
                            ui.label(RichText::new("Colors apply on top of every game's theme. Switch a color off to go back to the game's own.").font(g.body(15.0)).color(th.ink_soft));
                        });

                        section(ui, &g, "Save editor layout", |ui| {
                            choice_row(ui, &g, "Category tabs", lw, &mut st.tabs, &[("Left", TabsPos::Left), ("Right", TabsPos::Right), ("Top", TabsPos::Top)]);
                            choice_row(ui, &g, "Action buttons", lw, &mut st.actions, &[("Bottom", ActPos::Bottom), ("Top", ActPos::Top)]);
                            choice_row(ui, &g, "Spacing", lw, &mut st.density, &[("Compact", Density::Compact), ("Normal", Density::Normal), ("Roomy", Density::Roomy)]);
                            choice_row(ui, &g, "Columns per card", lw, &mut st.cols, &[("Game default", 0u8), ("1", 1), ("2", 2), ("3", 3)]);
                        });

                        section(ui, &g, "Saves", |ui| {
                            ui.horizontal(|ui| {
                                label(ui, &g, "Backup before writing", lw);
                                toggle(ui, &g, &mut st.backup, Id::new("set_backup"));
                            });
                        });

                        section(ui, &g, "Game editors", |ui| {
                            ui.label(RichText::new(&pack_line).font(g.body(19.0)).color(th.ink));
                            ui.label(RichText::new(&codec_line).font(g.body(15.0)).color(th.ink_soft));
                            ui.label(RichText::new(&folder).font(g.body(14.0)).color(th.ink_soft));
                            ui.horizontal(|ui| {
                                label(ui, &g, "Check at startup", lw);
                                toggle(ui, &g, &mut st.auto_update, Id::new("set_auto"));
                            });
                            if button(ui, &g, vec2(300.0, 40.0), if checking { "CHECKING..." } else { "CHECK FOR UPDATES" }, th.btn[1], th.btn_ink, None).clicked() {
                                check = true;
                            }
                        });

                        section(ui, &g, "Keyboard", |ui| {
                            for (k, d) in [("Esc", "go back to the previous screen"), ("Ctrl+Z", "undo the last change you made"), ("Ctrl+Y / Ctrl+Shift+Z", "redo")] {
                                ui.label(RichText::new(format!("{k}  -  {d}")).font(g.body(18.0)).color(th.ink));
                            }
                        });

                        if button(ui, &g, vec2(300.0, 40.0), "RESET ALL SETTINGS", th.btn[0], th.btn_ink, None).clicked() {
                            reset = true;
                        }
                        ui.add_space(8.0);
                    });
                });
            });
        }
        if reset {
            let keep = (self.st.sync_enabled, self.st.sync_allow_write, self.st.sync_token.clone(), self.st.sync_port, self.st.sync_ip.clone(), self.st.sync_peer.clone());
            self.st = Settings::default();
            (self.st.sync_enabled, self.st.sync_allow_write, self.st.sync_token, self.st.sync_port, self.st.sync_ip, self.st.sync_peer) = keep;
            self.styled.clear();
            self.say("Settings reset to defaults", true);
        }
        if check {
            self.start_check(ctx, true);
        }
        if back {
            self.go_back();
        }
        self.toast_ui(ctx, &th, &font);
    }
}

// --------------------------------------------------------------------- morceaux d'interface partagés

fn version_line(game: &Game, doc: &dyn Doc) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    if !game.ver.is_empty() {
        if let Some(v) = doc.get(&game.ver) {
            let t = v.text();
            if !t.trim().is_empty() {
                parts.push(format!("Save written by game v{}", t.trim()));
            }
        }
    }
    if !game.stores.is_empty() {
        parts.push(game.stores.iter().map(|s| format!("{} {}", s.name, s.version)).collect::<Vec<_>>().join(" - "));
    }
    if parts.is_empty() { None } else { Some(parts.join("  |  ")) }
}

fn tabs_vertical(ui: &mut Ui, g: &Gfx, th: &Theme, game: &Game, cat: &mut usize) {
    ui.spacing_mut().item_spacing.y = 10.0;
    scroll_area(ui, th, |ui| {
        for (i, c) in game.cats.iter().enumerate() {
            if tab(ui, g, vec2(ui.available_width().min(166.0), 98.0), &c.icon, &c.name, *cat == i).clicked() {
                *cat = i;
            }
        }
    });
}

fn action_bar(ui: &mut Ui, g: &Gfx, dirty: bool, path: &Path, act: &mut Option<Act>) {
    let th = g.th;
    let gap = 14.0;
    let w = (ui.available_width() - gap * 3.0) / 4.0;
    let mut ui_h = ui.new_child(UiBuilder::new().max_rect(ui.available_rect_before_wrap()).layout(Layout::left_to_right(Align::TOP)));
    ui_h.spacing_mut().item_spacing.x = gap;
    let btns: [(&str, &str, &str, Color32, Act); 4] = [
        ("folder", "Load save", "Load another...", th.btn[0], Act::Load),
        ("refresh", "Refresh save", "Reload from disk", th.btn[1], Act::Refresh),
        ("restore", "Backups", "Restore last backup", th.btn[2], Act::Restore),
        ("disk", "Save changes", if dirty { "Unsaved changes" } else { "Write to disk" }, th.btn[3], Act::Save),
    ];
    for (ic, big, small, col, a) in btns {
        let ink = if a == Act::Save && th.btn[3].g() > 200 { hex(0x2a1a06) } else { th.btn_ink };
        if action(&mut ui_h, g, vec2(w, 100.0), ic, big, small, col, ink, true).clicked() {
            *act = Some(a);
        }
    }
    let r = ui.max_rect();
    let comps: Vec<String> = path.components().rev().take(3).map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let short_path = format!(".../{}", comps.into_iter().rev().collect::<Vec<_>>().join("/"));
    let ft = format!("SaveStudio {VERSION}{}", if dirty { " *" } else { "" });
    let rw = g.text_w(ui.painter(), &ft, g.body(15.0)) + 8.0;
    let pl = ui.painter().with_clip_rect(Rect::from_min_max(r.min, pos2(r.max.x - rw, r.max.y)));
    pl.text(pos2(r.min.x, r.max.y - 2.0), Align2::LEFT_BOTTOM, short_path, g.body(15.0), th.ink_soft);
    ui.painter().text(pos2(r.max.x, r.max.y - 2.0), Align2::RIGHT_BOTTOM, ft, g.body(15.0), th.ink_soft);
}

// --------------------------------------------------------------------- éléments de l'écran Settings

fn section(ui: &mut Ui, g: &Gfx, title: &str, add: impl FnOnce(&mut Ui)) {
    let th = g.th;
    Frame::new().fill(th.card).stroke(Stroke::new(th.bw, th.outline)).corner_radius(CornerRadius::same(th.round)).inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(g.cap(title)).font(g.head(26.0)).color(th.ink));
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.y = 8.0;
        add(ui);
    });
    ui.add_space(12.0);
}

fn slider_row(ui: &mut Ui, g: &Gfx, text: &str, lw: f32, add: impl FnOnce(&mut Ui) -> Response) {
    ui.horizontal(|ui| {
        label(ui, g, text, lw);
        ui.spacing_mut().slider_width = 240.0;
        add(ui);
    });
}

fn choice_row<T: PartialEq + Copy>(ui: &mut Ui, g: &Gfx, text: &str, lw: f32, cur: &mut T, opts: &[(&str, T)]) {
    ui.horizontal_wrapped(|ui| {
        label(ui, g, text, lw);
        for (t, v) in opts {
            let w = (g.text_w(ui.painter(), t, g.body(18.0)) + 34.0).max(70.0);
            if chip(ui, g, t, *cur == *v, w).clicked() {
                *cur = *v;
            }
        }
    });
}

fn color_row(ui: &mut Ui, g: &Gfx, text: &str, lw: f32, val: &mut Option<Color32>, default: Color32, id: &str) {
    ui.horizontal(|ui| {
        label(ui, g, text, lw);
        let mut on = val.is_some();
        if toggle(ui, g, &mut on, Id::new(id)) {
            *val = if on { Some(default) } else { None };
        }
        match val {
            Some(c) => {
                ui.color_edit_button_srgba(c);
            }
            None => {
                ui.label(RichText::new("game theme").font(g.body(16.0)).color(g.th.ink_soft));
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn font_picker(ui: &mut Ui, g: &Gfx, id: &str, text: &str, cur: &mut String, default_label: Option<&str>, choices: &[fonts::Choice], sys: &[SysFont], filter: &mut String, defined: &[String], lw: f32) {
    let th = g.th;
    let shown = if cur.is_empty() { default_label.unwrap_or("").to_string() } else { fonts::display_name(cur, sys) };
    ui.horizontal(|ui| {
        label(ui, g, text, lw);
        ComboBox::from_id_salt(id).width(300.0).selected_text(RichText::new(shown).font(g.body(21.0)).color(th.input_ink)).show_ui(ui, |ui| {
            ui.add(TextEdit::singleline(filter).hint_text("search a font...").desired_width(280.0));
            let q = filter.to_lowercase();
            ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                if let Some(d) = default_label {
                    if ui.selectable_label(cur.is_empty(), RichText::new(d).font(g.body(20.0)).color(th.input_ink)).clicked() {
                        cur.clear();
                    }
                }
                for c in choices.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q)) {
                    let label = if c.system { c.name.clone() } else { format!("{}  (game pack)", c.name) };
                    if ui.selectable_label(*cur == c.key, RichText::new(label).font(g.body(20.0)).color(th.input_ink)).clicked() {
                        *cur = c.key.clone();
                    }
                }
            });
        });
    });
    // aperçu dans la police choisie (si elle est déjà chargée)
    if !cur.is_empty() && defined.iter().any(|d| d == cur) {
        let f = FontId::new(24.0, FontFamily::Name(format!("b:{cur}").into()));
        ui.horizontal(|ui| {
            ui.add_space(lw + 8.0);
            ui.label(RichText::new("The quick brown fox - 0123456789").font(f).color(th.ink));
        });
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

fn sub_fields(ui: &mut Ui, g: &Gfx, sub: &def::Sub, env: &mut Env, id: Id, force_cols: u8, dens: f32) {
    let total = ui.available_width();
    let cols = if force_cols > 0 { force_cols as usize } else { sub.cols.max(1) };
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
        ui.add_space(4.0 * dens);
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
                    apply_sync(env, f, &Scalar::Bool(on), None);
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
                env.put(&f.path, Scalar::Text(s.clone()));
                apply_sync(env, f, &Scalar::Text(s), None);
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
                            let idx = f.opts.iter().position(|(_, x)| x == v);
                            apply_sync(env, f, &Scalar::Text(v.clone()), idx);
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
        self.sync_fonts(ctx);
        self.fonts_live = true;
        self.poll_update(ctx);
        self.sync_tick(ctx);
        let modal = self.modal_open();

        // glisser-déposer d'un fichier de sauvegarde
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|d| d.path.clone()).collect());
        if let (Some(p), false) = (dropped.first(), modal) {
            self.open_any(p);
        }

        // Échap : écran précédent (les fenêtres et listes déroulantes gèrent Échap elles-mêmes)
        if !modal && ctx.input(|i| i.key_pressed(Key::Escape)) && !ctx.memory(|m| m.any_popup_open()) {
            self.go_back();
        }
        // Ctrl+Z / Ctrl+Y : annuler / rétablir (pas pendant la saisie dans un champ : il a son propre Ctrl+Z)
        if self.screen == Screen::Editor && !modal && !ctx.wants_keyboard_input() {
            let redo = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y));
            let undo = !redo && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
            if redo || undo {
                self.undo_redo(redo);
            }
        }

        match self.screen {
            Screen::Home => self.home_ui(ctx),
            Screen::Saves(g) if g < self.games.len() => self.saves_ui(ctx, g),
            Screen::Saves(_) => self.screen = Screen::Home,
            Screen::Settings => self.settings_ui(ctx),
            Screen::Sync => self.sync_ui(ctx),
            Screen::Editor => {
                if self.sess.is_none() {
                    self.screen = Screen::Home;
                } else {
                    self.editor_ui(ctx);
                }
            }
        }
        self.confirm_ui(ctx);
        self.upd_ui(ctx);

        // les réglages sont enregistrés dès qu'on lâche la souris (pas à chaque image d'un glissement)
        if self.st != self.st_saved && !ctx.input(|i| i.pointer.any_down()) {
            self.st.save();
            self.st_saved = self.st.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<Scalar> {
        Some(Scalar::Text(v.into()))
    }

    #[test]
    fn history_merges_a_drag_into_one_step() {
        let mut h = History::default();
        let t0 = Instant::now();
        h.record("money", s("100"));
        h.commit(t0);
        h.record("money", s("150"));
        h.commit(t0 + Duration::from_millis(50));
        h.record("money", s("200"));
        h.commit(t0 + Duration::from_millis(120));
        assert_eq!(h.undo.len(), 1);
        // revient à la valeur d'avant le geste
        assert_eq!(h.undo[0].changes[0].1, s("100"));
    }

    #[test]
    fn history_keeps_separate_edits_apart() {
        let mut h = History::default();
        let t0 = Instant::now();
        h.record("a", s("1"));
        h.commit(t0);
        h.record("a", s("2"));
        h.commit(t0 + Duration::from_secs(5));
        h.record("b", s("9"));
        h.commit(t0 + Duration::from_secs(5) + Duration::from_millis(10));
        assert_eq!(h.undo.len(), 3);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut h = History::default();
        h.redo.push(Entry { changes: vec![("x".into(), s("1"))], at: Instant::now() });
        h.record("a", s("1"));
        h.commit(Instant::now());
        assert!(h.redo.is_empty());
    }

    #[test]
    fn mirror_prefix_mapping() {
        assert_eq!(strip_path("SaveGame/player/money", "SaveGame/player"), Some("money"));
        assert_eq!(strip_path("SaveGame/player", "SaveGame/player"), Some(""));
        assert_eq!(strip_path("SaveGame/playerX/a", "SaveGame/player"), None);
        assert_eq!(strip_path("SaveGame/year", "SaveGame/player"), None);
    }
}

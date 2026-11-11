//! Écran « Sync » : partage de sauvegardes entre ordinateurs à travers Tailscale (protocole dans `sync.rs`).
use super::*;
use crate::sync::{self, GameLoc, Row, State};
use crate::tailscale::{self, Status};
use std::net::{IpAddr, SocketAddr};

type Listing = Result<(sync::Ping, Vec<Row>), String>;

#[derive(Default)]
pub struct SyncUi {
    server: Option<sync::Server>,
    server_err: Option<String>,
    ts: Option<Result<Status, String>>,
    ts_rx: Option<Receiver<Result<Status, String>>>,
    ts_asked: bool,
    rows: Vec<Row>,
    remote_name: String,
    remote_write: bool,
    connected: bool,
    list_rx: Option<Receiver<Listing>>,
    op_rx: Option<Receiver<Result<String, String>>>,
    status: Option<(String, bool)>,
    /// (ligne, true = récupérer ici / false = envoyer là-bas) en attente de confirmation
    confirm: Option<(usize, bool)>,
    seen_writes: usize,
    games_sig: Option<String>,
}

enum SAct {
    RefreshTs,
    Connect,
    Op(usize, bool),
    NewToken,
    CopyToken,
    UsePeer(String),
}

impl SyncUi {
    pub fn confirm_open(&self) -> bool {
        self.confirm.is_some()
    }
    pub fn running(&self) -> bool {
        self.server.is_some()
    }
}

fn parse_ip(s: &str) -> Option<IpAddr> {
    s.trim().parse().ok()
}

impl App {
    pub(super) fn game_locs(&self) -> Vec<GameLoc> {
        self.games.iter().map(|g| GameLoc { id: g.id.clone(), name: g.name.clone(), patterns: g.patterns.clone(), mirror: g.mirror.as_ref().map(|m| m.file.clone()) }).collect()
    }

    fn ask_tailscale(&mut self, ctx: &Context) {
        if self.sync.ts_rx.is_some() {
            return;
        }
        self.sync.ts_asked = true;
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(tailscale::status());
            c.request_repaint();
        });
        self.sync.ts_rx = Some(rx);
    }

    /// Adresse sur laquelle écouter : celle saisie à la main, sinon celle que Tailscale donne à cet ordinateur.
    fn my_sync_ip(&self) -> Option<IpAddr> {
        if !self.st.sync_ip.trim().is_empty() {
            return parse_ip(&self.st.sync_ip);
        }
        match &self.sync.ts {
            Some(Ok(s)) => s.self_ip.as_deref().and_then(parse_ip),
            _ => None,
        }
    }

    fn connect(&mut self, ctx: &Context) {
        if self.sync.list_rx.is_some() || self.st.sync_peer.trim().is_empty() {
            return;
        }
        let (peer, token, games) = (self.st.sync_peer.clone(), self.st.sync_token.clone(), self.game_locs());
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r: Listing = sync::ping(&peer, &token).and_then(|p| {
                let remote = sync::remote_list(&peer, &token)?;
                Ok((p, sync::compare(&games, sync::local_items(&games), remote)))
            });
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.sync.list_rx = Some(rx);
        self.sync.status = Some(("Connecting...".into(), true));
    }

    fn run_op(&mut self, ctx: &Context, idx: usize, pull: bool) {
        if self.sync.op_rx.is_some() {
            return;
        }
        let Some(row) = self.sync.rows.get(idx).cloned() else { return };
        let (peer, token, games) = (self.st.sync_peer.clone(), self.st.sync_token.clone(), self.game_locs());
        let (tx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = if pull { sync::pull(&peer, &token, &games, &row).map(|n| format!("Received {} ({n} file{})", row.rel, if n > 1 { "s" } else { "" })) } else { sync::push(&peer, &token, &row).map(|n| format!("Sent {} ({n} file{})", row.rel, if n > 1 { "s" } else { "" })) };
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.sync.op_rx = Some(rx);
        self.sync.status = Some((if pull { "Receiving..." } else { "Sending..." }.into(), true));
    }

    /// À chaque image : réponses des tâches de fond, démarrage/arrêt du serveur de partage, mots de passe.
    pub(super) fn sync_tick(&mut self, ctx: &Context) {
        if let Some(rx) = self.sync.ts_rx.take() {
            match rx.try_recv() {
                Ok(r) => self.sync.ts = Some(r),
                Err(TryRecvError::Empty) => {
                    self.sync.ts_rx = Some(rx);
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        if let Some(rx) = self.sync.list_rx.take() {
            match rx.try_recv() {
                Ok(Ok((p, rows))) => {
                    self.sync.rows = rows;
                    self.sync.connected = true;
                    self.sync.remote_name = if p.host.is_empty() { self.st.sync_peer.clone() } else { p.host.clone() };
                    self.sync.remote_write = p.write;
                    self.sync.status = Some((format!("Connected to {} (SaveStudio {})", self.sync.remote_name, p.version), true));
                }
                Ok(Err(e)) => {
                    self.sync.connected = false;
                    self.sync.rows.clear();
                    self.sync.status = Some((e, false));
                }
                Err(TryRecvError::Empty) => {
                    self.sync.list_rx = Some(rx);
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        if let Some(rx) = self.sync.op_rx.take() {
            match rx.try_recv() {
                Ok(r) => {
                    for (i, g) in self.games.iter().enumerate() {
                        self.scans[i] = detect::scan(&g.patterns);
                    }
                    match r {
                        Ok(m) => {
                            self.say(m, true);
                            self.sync.status = None;
                            self.connect(ctx); // rafraîchit le tableau
                        }
                        Err(e) => {
                            self.sync.status = Some((e.clone(), false));
                            self.say(e, false);
                        }
                    }
                }
                Err(TryRecvError::Empty) => {
                    self.sync.op_rx = Some(rx);
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }

        // serveur de partage
        let need_ts = self.st.sync_enabled || self.screen == Screen::Sync;
        if need_ts && !self.sync.ts_asked {
            self.ask_tailscale(ctx);
        }
        if (self.st.sync_enabled || self.screen == Screen::Sync) && self.st.sync_token.is_empty() {
            self.st.sync_token = sync::new_token();
        }
        if !self.st.sync_enabled {
            self.sync.server = None;
            self.sync.server_err = None;
        } else {
            let want = self.my_sync_ip().map(|ip| SocketAddr::new(ip, self.st.sync_port));
            if let (Some(s), Some(w)) = (&self.sync.server, want) {
                if s.addr != w {
                    self.sync.server = None; // adresse ou port changés : on redémarre
                }
            }
            if self.sync.server.is_none() {
                match want {
                    Some(a) if tailscale::is_tailnet_ip(a.ip()) => {
                        let cfg = sync::ServerCfg { token: self.st.sync_token.clone(), allow_write: self.st.sync_allow_write, games: self.game_locs() };
                        match sync::start(a, cfg, false) {
                            Ok(s) => {
                                self.sync.seen_writes = 0;
                                self.sync.games_sig = self.pack_version.clone();
                                self.sync.server = Some(s);
                                self.sync.server_err = None;
                            }
                            Err(e) => self.sync.server_err = Some(e),
                        }
                    }
                    Some(_) => self.sync.server_err = Some("That is not a Tailscale address (they start with 100.64 to 100.127)".into()),
                    None => {
                        self.sync.server_err = match &self.sync.ts {
                            Some(Err(e)) => Some(format!("{e}. Enter this computer's Tailscale address below.")),
                            Some(Ok(s)) if s.self_ip.is_none() => Some("Tailscale is installed but not connected (sign in to Tailscale first)".into()),
                            _ => None,
                        }
                    }
                }
            }
            if let Some(s) = &self.sync.server {
                {
                    let mut c = s.cfg.lock().unwrap_or_else(|p| p.into_inner());
                    c.token = self.st.sync_token.clone();
                    c.allow_write = self.st.sync_allow_write;
                    if self.sync.games_sig != self.pack_version {
                        c.games = self.games.iter().map(|g| GameLoc { id: g.id.clone(), name: g.name.clone(), patterns: g.patterns.clone(), mirror: g.mirror.as_ref().map(|m| m.file.clone()) }).collect();
                    }
                }
                self.sync.games_sig = self.pack_version.clone();
                let w = s.writes.load(std::sync::atomic::Ordering::Relaxed);
                if w != self.sync.seen_writes {
                    let n = w.saturating_sub(self.sync.seen_writes);
                    self.sync.seen_writes = w;
                    for (i, g) in self.games.iter().enumerate() {
                        self.scans[i] = detect::scan(&g.patterns);
                    }
                    self.say(format!("A paired computer replaced {n} of your save file{}. Reload a save if it was open.", if n > 1 { "s" } else { "" }), true);
                }
                ctx.request_repaint_after(Duration::from_secs(2));
            }
        }
    }

    pub(super) fn sync_ui(&mut self, ctx: &Context) {
        let th = self.title_theme();
        let font = self.font_key(&self.st.settings_font);
        self.style_for(ctx, "sync", &th, &font);
        let mut back = false;
        let mut act: Option<SAct> = None;
        let busy = self.sync.op_rx.is_some() || self.sync.list_rx.is_some();
        let my_ip = self.my_sync_ip();
        {
            let App { st, icons, sync, .. } = self;
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
                    let t = g.cap("Sync with Tailscale");
                    let f = fit(&p, &t, g.head(38.0), tr.width());
                    text_shadow(&p, pos2(tr.min.x, tr.center().y - 2.0), Align2::LEFT_CENTER, &t, f, th.title, th.title_shadow, 3.0);
                });
            });
            CentralPanel::default().frame(Frame::new().fill(th.bg).inner_margin(Margin::symmetric(24, 4))).show(ctx, |ui| {
                let r = ui.available_rect_before_wrap();
                boxed(ui.painter(), r, th.panel, &th, th.panel_edge);
                ui.scope_builder(UiBuilder::new().max_rect(r.shrink(th.bw + 12.0)), |ui| {
                    scroll_area(ui, &th, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        let lw = 230.0;
                        ui.label(RichText::new("Keep the same saves on all your computers. Both need Tailscale, SaveStudio and the same shared password. Nothing goes through the internet: it travels inside your private Tailscale network.").font(g.body(16.0)).color(th.ink_soft));
                        ui.add_space(8.0);

                        section(ui, &g, "This computer", |ui| {
                            let line = match (&sync.ts, my_ip) {
                                (_, Some(ip)) => format!("Tailscale address: {ip}"),
                                (None, None) => "Looking for Tailscale...".to_string(),
                                (Some(Err(e)), None) => e.clone(),
                                (Some(Ok(_)), None) => "Tailscale is not connected".to_string(),
                            };
                            ui.label(RichText::new(line).font(g.body(19.0)).color(th.ink));
                            ui.horizontal(|ui| {
                                label(ui, &g, "Let my other computers use my saves", lw + 90.0);
                                toggle(ui, &g, &mut st.sync_enabled, Id::new("sync_enabled"));
                            });
                            if st.sync_enabled {
                                match (&sync.server, &sync.server_err) {
                                    (Some(s), _) => ui.label(RichText::new(format!("Sharing on {}", s.addr)).font(g.body(17.0)).color(th.accent)),
                                    (None, Some(e)) => ui.label(RichText::new(e).font(g.body(17.0)).color(hex(0xc2392b))),
                                    _ => ui.label(RichText::new("Starting...").font(g.body(17.0)).color(th.ink_soft)),
                                };
                            }
                            ui.horizontal(|ui| {
                                label(ui, &g, "Allow them to replace my saves", lw + 90.0);
                                toggle(ui, &g, &mut st.sync_allow_write, Id::new("sync_write"));
                            });
                            ui.label(RichText::new("Off = other computers can only copy your saves. A backup is always made before anything is replaced.").font(g.body(14.0)).color(th.ink_soft));
                            ui.horizontal(|ui| {
                                label(ui, &g, "Shared password", lw);
                                ui.add(TextEdit::singleline(&mut st.sync_token).font(g.body(18.0)).text_color(th.input_ink).desired_width(300.0).margin(Margin::symmetric(8, 6)));
                                if button(ui, &g, vec2(70.0, 34.0), "NEW", th.btn[0], th.btn_ink, None).clicked() {
                                    act = Some(SAct::NewToken);
                                }
                                if button(ui, &g, vec2(80.0, 34.0), "COPY", th.btn[1], th.btn_ink, None).clicked() {
                                    act = Some(SAct::CopyToken);
                                }
                            });
                            ui.horizontal(|ui| {
                                label(ui, &g, "Tailscale address (if not found)", lw + 90.0);
                                ui.add(TextEdit::singleline(&mut st.sync_ip).hint_text(RichText::new("automatic").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width(160.0).margin(Margin::symmetric(8, 6)));
                            });
                        });

                        section(ui, &g, "The other computer", |ui| {
                            ui.horizontal(|ui| {
                                label(ui, &g, "Address or name", lw);
                                ui.add(TextEdit::singleline(&mut st.sync_peer).hint_text(RichText::new("100.x.y.z").color(th.ink_soft)).font(g.body(18.0)).text_color(th.input_ink).desired_width(220.0).margin(Margin::symmetric(8, 6)));
                                let label_btn = if busy { "..." } else { "CONNECT" };
                                if button(ui, &g, vec2(130.0, 34.0), label_btn, th.btn[2], th.btn_ink, None).clicked() && !busy {
                                    act = Some(SAct::Connect);
                                }
                            });
                            if let Some(Ok(ts)) = &sync.ts {
                                if !ts.peers.is_empty() {
                                    ui.label(RichText::new("Your Tailscale devices (click one):").font(g.body(16.0)).color(th.ink_soft));
                                    ui.horizontal_wrapped(|ui| {
                                        for p in &ts.peers {
                                            let t = format!("{} {}  {}", if p.online { "●" } else { "○" }, p.name, p.ip);
                                            let w = g.text_w(ui.painter(), &t, g.body(17.0)) + 30.0;
                                            if chip(ui, &g, &t, st.sync_peer.trim() == p.ip, w).clicked() {
                                                act = Some(SAct::UsePeer(p.ip.clone()));
                                            }
                                        }
                                    });
                                }
                            }
                            if button(ui, &g, vec2(250.0, 32.0), "REFRESH DEVICE LIST", th.btn[1], th.btn_ink, None).clicked() {
                                act = Some(SAct::RefreshTs);
                            }
                            if let Some((m, ok)) = &sync.status {
                                ui.label(RichText::new(m).font(g.body(17.0)).color(if *ok { th.ink } else { hex(0xc2392b) }));
                            }
                        });

                        if sync.connected {
                            section(ui, &g, &format!("Saves ({})", sync.rows.len()), |ui| {
                                if sync.rows.is_empty() {
                                    ui.label(RichText::new("No save found on either computer.").font(g.body(18.0)).color(th.ink_soft));
                                }
                                for (i, row) in sync.rows.iter().enumerate() {
                                    let (rr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 66.0), Sense::hover());
                                    let p = ui.painter_at(rr.expand(2.0));
                                    boxed(&p, rr, th.card_alt, &th, th.outline);
                                    let pc = p.with_clip_rect(rr.shrink(4.0));
                                    let state = row.state();
                                    let (txt, col) = match state {
                                        State::Same => ("In sync".to_string(), th.accent),
                                        State::LocalNewer => (format!("This computer is newer ({})", ago(row.local.as_ref().map(|(l, _)| std::time::UNIX_EPOCH + Duration::from_secs(l.files[0].mtime)))), th.ink),
                                        State::RemoteNewer => (format!("The other computer is newer ({})", ago(row.remote.as_ref().map(|r| std::time::UNIX_EPOCH + Duration::from_secs(r.files[0].mtime)))), th.ink),
                                        State::Differ => ("Different content, same date".to_string(), th.ink),
                                        State::OnlyLocal => ("Only on this computer".to_string(), th.ink_soft),
                                        State::OnlyRemote => ("Only on the other computer".to_string(), th.ink_soft),
                                    };
                                    pc.text(pos2(rr.min.x + 14.0, rr.min.y + 20.0), Align2::LEFT_CENTER, g.cap(&row.game_name), g.head(20.0), th.ink);
                                    pc.text(pos2(rr.min.x + 14.0, rr.max.y - 18.0), Align2::LEFT_CENTER, &row.rel, g.body(14.0), th.ink_soft);
                                    pc.text(pos2(rr.center().x - 40.0, rr.center().y), Align2::LEFT_CENTER, txt, g.body(16.0), col);
                                    let can_pull = row.remote.is_some() && state != State::Same;
                                    let can_push = row.local.is_some() && state != State::Same && sync.remote_write;
                                    let bw = 84.0;
                                    for (k, (name, on, fill, pull)) in [("PULL", can_pull, th.btn[2], true), ("PUSH", can_push, th.btn[1], false)].into_iter().enumerate() {
                                        let br = Rect::from_min_size(pos2(rr.max.x - 10.0 - bw * (2 - k) as f32 - 8.0 * (1 - k) as f32, rr.center().y - 18.0), vec2(bw, 36.0));
                                        ui.scope_builder(UiBuilder::new().max_rect(br), |ui| {
                                            let fill = if on { fill } else { shade(th.card, 0.9) };
                                            let ink = if on { th.btn_ink } else { th.ink_soft };
                                            if button(ui, &g, vec2(bw, 36.0), name, fill, ink, None).clicked() && on {
                                                act = Some(SAct::Op(i, pull));
                                            }
                                        });
                                    }
                                }
                                ui.label(RichText::new(if sync.remote_write { "PULL = copy here from the other computer.  PUSH = replace it there with this one." } else { "PULL = copy here from the other computer.  PUSH needs \"Allow them to replace my saves\" turned on over there." }).font(g.body(14.0)).color(th.ink_soft));
                            });
                        }
                    });
                });
            });
        }

        // confirmation avant de remplacer quelque chose qui existe déjà
        let mut decision: Option<bool> = None;
        if let Some((i, pull)) = self.sync.confirm {
            if let Some(row) = self.sync.rows.get(i) {
                let g = Gfx { th: &th, icons: &self.icons, font: &font };
                let (what, from, to) = if pull { (&row.rel, self.sync.remote_name.clone(), "this computer".to_string()) } else { (&row.rel, "this computer".to_string(), self.sync.remote_name.clone()) };
                let m = Modal::new(Id::new("sync_confirm")).frame(Frame::new().fill(th.panel).stroke(Stroke::new(th.bw, th.outline)).inner_margin(Margin::same(18))).show(ctx, |ui| {
                    ui.set_max_width(480.0);
                    ui.label(RichText::new(g.cap("Replace this save?")).font(g.head(26.0)).color(th.ink));
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("{what}\nfrom {from} to {to}.\nThe current version is backed up first.")).font(g.body(18.0)).color(th.ink));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if button(ui, &g, vec2(150.0, 46.0), "REPLACE", th.btn[0], th.btn_ink, None).clicked() {
                            decision = Some(true);
                        }
                        if button(ui, &g, vec2(150.0, 46.0), "CANCEL", th.btn[2], th.btn_ink, None).clicked() {
                            decision = Some(false);
                        }
                    });
                });
                if m.should_close() && decision.is_none() {
                    decision = Some(false);
                }
            } else {
                decision = Some(false);
            }
        }
        if let Some(d) = decision {
            let c = self.sync.confirm.take();
            if let (true, Some((i, pull))) = (d, c) {
                self.run_op(ctx, i, pull);
            }
        }

        match act {
            Some(SAct::RefreshTs) => {
                self.sync.ts = None;
                self.ask_tailscale(ctx);
            }
            Some(SAct::Connect) => {
                if self.st.sync_token.is_empty() {
                    self.st.sync_token = sync::new_token();
                }
                self.connect(ctx);
            }
            Some(SAct::NewToken) => self.st.sync_token = sync::new_token(),
            Some(SAct::CopyToken) => {
                ctx.copy_text(self.st.sync_token.clone());
                self.say("Password copied - paste it in SaveStudio on your other computer", true);
            }
            Some(SAct::UsePeer(ip)) => self.st.sync_peer = ip,
            Some(SAct::Op(i, pull)) => {
                let needs_confirm = self.sync.rows.get(i).map_or(false, |r| if pull { r.local.is_some() } else { r.remote.is_some() });
                if needs_confirm {
                    self.sync.confirm = Some((i, pull));
                } else {
                    self.run_op(ctx, i, pull);
                }
            }
            None => {}
        }
        if back {
            self.go_back();
        }
        self.toast_ui(ctx, &th, &font);
    }
}

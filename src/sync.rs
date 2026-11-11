//! Partage de sauvegardes entre ordinateurs, à travers Tailscale.
//!
//! * Un petit serveur HTTP intégré (désactivé par défaut) écoute **uniquement** sur l'adresse Tailscale de la machine
//!   et n'accepte que des connexions venant du réseau Tailscale. Le trafic est chiffré par Tailscale (WireGuard).
//! * Chaque requête doit porter le mot de passe partagé (`Authorization: Bearer …`).
//! * Une sauvegarde est identifiée par `(jeu, chemin relatif)`, le chemin relatif étant calculé sous le dossier fixe du
//!   motif de détection du jeu : « Ferme_123/Ferme_123 », « 0.celeste »… Il est donc le même sur Windows, Linux et Android.
//! * Le serveur n'écrit que si l'utilisateur l'a autorisé, seulement un fichier qui suit la forme d'un emplacement de
//!   sauvegarde du jeu (voir `detect::locate`), et fait toujours une copie de sécurité avant de remplacer.
use crate::detect;
use crate::fsutil::write_safely;
use crate::tailscale::is_tailnet_ip;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub const DEFAULT_PORT: u16 = 47653;
const MAX_BODY: usize = 64 << 20;

/// Ce qu'il faut savoir d'un jeu pour retrouver ses sauvegardes.
#[derive(Clone, Debug)]
pub struct GameLoc {
    pub id: String,
    pub name: String,
    pub patterns: Vec<String>,
    /// fichier compagnon (ex. SaveGameInfo), transféré avec la sauvegarde
    pub mirror: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo {
    pub rel: String,
    pub size: u64,
    pub mtime: u64,
    pub sha: String,
}

/// Une sauvegarde : `files[0]` est le fichier principal, le reste ses compagnons.
#[derive(Clone, Debug)]
pub struct ItemInfo {
    pub game: String,
    pub rel: String,
    pub files: Vec<FileInfo>,
}

pub fn sha_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn new_token() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        // ne devrait jamais arriver ; repli sur l'horloge plutôt que de planter
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
        b.copy_from_slice(&t.to_le_bytes());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn secs(t: Option<SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

fn file_info(rel: String, path: &PathBuf) -> Option<FileInfo> {
    let bytes = std::fs::read(path).ok()?;
    let mtime = secs(std::fs::metadata(path).ok()?.modified().ok());
    Some(FileInfo { rel, size: bytes.len() as u64, mtime, sha: sha_hex(&bytes) })
}

/// Sauvegardes de cet ordinateur (avec leurs chemins réels).
pub fn local_items(games: &[GameLoc]) -> Vec<(ItemInfo, Vec<PathBuf>)> {
    let mut out = vec![];
    for g in games {
        for f in detect::scan(&g.patterns) {
            if f.rel.is_empty() {
                continue;
            }
            let Some(main) = file_info(f.rel.clone(), &f.path) else { continue };
            let mut files = vec![main];
            let mut paths = vec![f.path.clone()];
            if let Some(m) = &g.mirror {
                if let Some(p) = f.path.parent().map(|d| d.join(m)).filter(|p| p.is_file()) {
                    let rel = match f.rel.rsplit_once('/') {
                        Some((d, _)) => format!("{d}/{m}"),
                        None => m.clone(),
                    };
                    if let Some(i) = file_info(rel, &p) {
                        files.push(i);
                        paths.push(p);
                    }
                }
            }
            out.push((ItemInfo { game: g.id.clone(), rel: f.rel.clone(), files }, paths));
        }
    }
    out
}

fn item_json(i: &ItemInfo) -> Value {
    json!({"game": i.game, "rel": i.rel, "files": i.files.iter().map(|f| json!({"rel": f.rel, "size": f.size, "mtime": f.mtime, "sha256": f.sha})).collect::<Vec<_>>()})
}

fn item_from_json(v: &Value) -> Option<ItemInfo> {
    let files = v.get("files")?.as_array()?.iter().filter_map(|f| Some(FileInfo { rel: f.get("rel")?.as_str()?.to_string(), size: f.get("size")?.as_u64()?, mtime: f.get("mtime")?.as_u64()?, sha: f.get("sha256")?.as_str()?.to_string() })).collect::<Vec<_>>();
    if files.is_empty() {
        return None;
    }
    Some(ItemInfo { game: v.get("game")?.as_str()?.to_string(), rel: v.get("rel")?.as_str()?.to_string(), files })
}

// ------------------------------------------------------------------ serveur

pub struct ServerCfg {
    pub token: String,
    pub allow_write: bool,
    pub games: Vec<GameLoc>,
}

pub struct Server {
    stop: Arc<AtomicBool>,
    pub cfg: Arc<Mutex<ServerCfg>>,
    /// nombre de fichiers écrits par des pairs depuis le démarrage
    pub writes: Arc<AtomicUsize>,
    pub addr: SocketAddr,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// `allow_loopback` : uniquement pour les tests (en usage normal, seules les adresses Tailscale sont acceptées).
pub fn start(addr: SocketAddr, cfg: ServerCfg, allow_loopback: bool) -> Result<Server, String> {
    let l = TcpListener::bind(addr).map_err(|e| format!("Cannot listen on {addr}: {e}"))?;
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let cfg = Arc::new(Mutex::new(cfg));
    let writes = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let (st, c, w) = (stop.clone(), cfg.clone(), writes.clone());
    std::thread::spawn(move || {
        while !st.load(Ordering::Relaxed) {
            match l.accept() {
                Ok((s, peer)) => {
                    let ok = is_tailnet_ip(peer.ip()) || (allow_loopback && peer.ip().is_loopback());
                    if !ok || active.load(Ordering::Relaxed) >= 8 {
                        continue; // connexion fermée en la laissant tomber
                    }
                    active.fetch_add(1, Ordering::Relaxed);
                    let (c, w, a) = (c.clone(), w.clone(), active.clone());
                    std::thread::spawn(move || {
                        let _ = s.set_nonblocking(false);
                        handle(s, &c, &w);
                        a.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    });
    Ok(Server { stop, cfg, writes, addr })
}

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn pct_encode(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || b"-._~/".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

fn read_request(s: &TcpStream) -> Result<Req, String> {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    (&mut r).take(8192).read_line(&mut line).map_err(|e| e.to_string())?;
    let mut it = line.split_whitespace();
    let (method, target) = (it.next().ok_or("empty request")?.to_string(), it.next().ok_or("bad request line")?.to_string());
    let (path, q) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = q.split('&').filter(|x| !x.is_empty()).filter_map(|kv| kv.split_once('=')).map(|(k, v)| (pct_decode(k), pct_decode(v))).collect();
    let mut headers = HashMap::new();
    for _ in 0..40 {
        let mut h = String::new();
        (&mut r).take(8192).read_line(&mut h).map_err(|e| e.to_string())?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let len: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    if len > MAX_BODY {
        return Err("body too large".into());
    }
    let mut body = vec![];
    if len > 0 {
        (&mut r).take(len as u64).read_to_end(&mut body).map_err(|e| e.to_string())?;
        if body.len() != len {
            return Err("truncated body".into());
        }
    }
    Ok(Req { method, path: path.to_string(), query, headers, body })
}

fn respond(mut s: &TcpStream, code: u16, ctype: &str, body: &[u8]) {
    let text = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {code} {text}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
    let _ = s.flush();
}

fn err(s: &TcpStream, code: u16, msg: &str) {
    respond(s, code, "application/json", json!({"error": msg}).to_string().as_bytes());
}

fn same_secret(a: &str, b: &str) -> bool {
    let (x, y) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    x.iter().zip(y.iter()).fold(0u8, |acc, (p, q)| acc | (p ^ q)) == 0
}

fn handle(s: TcpStream, cfg: &Arc<Mutex<ServerCfg>>, writes: &AtomicUsize) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
    let req = match read_request(&s) {
        Ok(r) => r,
        Err(e) => return err(&s, 400, &e),
    };
    let (token, allow_write, games) = {
        let c = cfg.lock().unwrap_or_else(|p| p.into_inner());
        (c.token.clone(), c.allow_write, c.games.clone())
    };
    let given = req.headers.get("authorization").and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if token.is_empty() || !same_secret(given, &token) {
        std::thread::sleep(Duration::from_millis(400)); // freine les essais répétés
        return err(&s, 401, "wrong password");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/ping") => {
            let host = std::env::var("HOSTNAME").or_else(|_| std::env::var("COMPUTERNAME")).unwrap_or_default();
            respond(&s, 200, "application/json", json!({"app": "savestudio", "version": crate::pack::APP_VERSION, "host": host, "write": allow_write}).to_string().as_bytes());
        }
        ("GET", "/v1/list") => {
            let items: Vec<Value> = local_items(&games).iter().map(|(i, _)| item_json(i)).collect();
            respond(&s, 200, "application/json", json!({"items": items}).to_string().as_bytes());
        }
        ("GET", "/v1/file") => {
            let (Some(game), Some(rel)) = (req.query.get("game"), req.query.get("rel")) else { return err(&s, 400, "game and rel required") };
            let found = local_items(&games).into_iter().filter(|(i, _)| &i.game == game).find_map(|(i, paths)| i.files.iter().position(|f| &f.rel == rel).map(|k| paths[k].clone()));
            match found.and_then(|p| std::fs::read(p).ok()) {
                Some(b) => respond(&s, 200, "application/octet-stream", &b),
                None => err(&s, 404, "no such save"),
            }
        }
        ("PUT", "/v1/file") => {
            if !allow_write {
                return err(&s, 403, "this computer does not accept changes to its saves (turn on \"Allow replacing\" in its Sync screen)");
            }
            let (Some(game), Some(rel)) = (req.query.get("game"), req.query.get("rel")) else { return err(&s, 400, "game and rel required") };
            let Some(g) = games.iter().find(|g| &g.id == game) else { return err(&s, 404, "unknown game") };
            let Some(want) = req.headers.get("x-sha256") else { return err(&s, 400, "X-Sha256 required") };
            if !want.eq_ignore_ascii_case(&sha_hex(&req.body)) {
                return err(&s, 400, "checksum mismatch");
            }
            // fichier déjà connu, sinon emplacement conforme à la forme des sauvegardes du jeu
            let existing = local_items(&games).into_iter().filter(|(i, _)| &i.game == game).find_map(|(i, paths)| i.files.iter().position(|f| &f.rel == rel).map(|k| paths[k].clone()));
            let Some(target) = existing.or_else(|| detect::locate(&g.patterns, rel, g.mirror.as_deref())) else { return err(&s, 400, "that path does not look like a save of this game") };
            match write_safely(&target, &req.body, true) {
                Ok(()) => {
                    writes.fetch_add(1, Ordering::Relaxed);
                    respond(&s, 200, "application/json", b"{\"ok\":true}");
                }
                Err(e) => err(&s, 500, &e),
            }
        }
        _ => err(&s, 404, "unknown request"),
    }
}

// ------------------------------------------------------------------ client

fn base(peer: &str) -> String {
    let p = peer.trim().trim_start_matches("http://").trim_end_matches('/');
    let colons = p.matches(':').count();
    let hp = if colons == 0 {
        format!("{p}:{DEFAULT_PORT}")
    } else if colons == 1 || p.starts_with('[') {
        p.to_string()
    } else {
        format!("[{p}]:{DEFAULT_PORT}") // IPv6 nu
    };
    format!("http://{hp}")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout(Duration::from_secs(60)).build()
}

fn explain(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => "Wrong shared password".to_string(),
        ureq::Error::Status(c, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("error").and_then(|m| m.as_str()).map(|m| m.to_string()));
            msg.unwrap_or_else(|| format!("HTTP {c}"))
        }
        e => format!("Cannot reach the other computer ({e}). Is SaveStudio open there with sharing turned on?"),
    }
}

fn get(peer: &str, token: &str, path: &str) -> Result<Vec<u8>, String> {
    let r = agent().get(&format!("{}{path}", base(peer))).set("Authorization", &format!("Bearer {token}")).call().map_err(explain)?;
    let mut b = vec![];
    r.into_reader().take(MAX_BODY as u64).read_to_end(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

#[derive(Debug)]
pub struct Ping {
    pub host: String,
    pub write: bool,
    pub version: String,
}

pub fn ping(peer: &str, token: &str) -> Result<Ping, String> {
    let v: Value = serde_json::from_slice(&get(peer, token, "/v1/ping")?).map_err(|e| e.to_string())?;
    if v.get("app").and_then(|x| x.as_str()) != Some("savestudio") {
        return Err("That is not a SaveStudio sharing server".into());
    }
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Ok(Ping { host: s("host"), write: v.get("write").and_then(|x| x.as_bool()).unwrap_or(false), version: s("version") })
}

pub fn remote_list(peer: &str, token: &str) -> Result<Vec<ItemInfo>, String> {
    let v: Value = serde_json::from_slice(&get(peer, token, "/v1/list")?).map_err(|e| e.to_string())?;
    Ok(v.get("items").and_then(|i| i.as_array()).map(|a| a.iter().filter_map(item_from_json).collect()).unwrap_or_default())
}

fn download(peer: &str, token: &str, game: &str, rel: &str) -> Result<Vec<u8>, String> {
    get(peer, token, &format!("/v1/file?game={}&rel={}", pct_encode(game), pct_encode(rel)))
}

fn upload(peer: &str, token: &str, game: &str, rel: &str, bytes: &[u8]) -> Result<(), String> {
    agent()
        .put(&format!("{}/v1/file?game={}&rel={}", base(peer), pct_encode(game), pct_encode(rel)))
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-Sha256", &sha_hex(bytes))
        .send_bytes(bytes)
        .map(|_| ())
        .map_err(explain)
}

// ------------------------------------------------------------------ comparaison et transferts

#[derive(Clone, Debug)]
pub struct Row {
    pub game: String,
    pub game_name: String,
    pub rel: String,
    pub local: Option<(ItemInfo, Vec<PathBuf>)>,
    pub remote: Option<ItemInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Same,
    LocalNewer,
    RemoteNewer,
    /// contenus différents mais même date
    Differ,
    OnlyLocal,
    OnlyRemote,
}

impl Row {
    pub fn state(&self) -> State {
        match (&self.local, &self.remote) {
            (Some(_), None) => State::OnlyLocal,
            (None, Some(_)) => State::OnlyRemote,
            (Some((l, _)), Some(r)) => {
                let same = l.files.len() == r.files.len() && l.files.iter().all(|a| r.files.iter().any(|b| a.rel == b.rel && a.sha == b.sha));
                if same {
                    State::Same
                } else if l.files[0].mtime > r.files[0].mtime + 1 {
                    State::LocalNewer
                } else if r.files[0].mtime > l.files[0].mtime + 1 {
                    State::RemoteNewer
                } else {
                    State::Differ
                }
            }
            (None, None) => State::Same,
        }
    }
}

pub fn compare(games: &[GameLoc], local: Vec<(ItemInfo, Vec<PathBuf>)>, remote: Vec<ItemInfo>) -> Vec<Row> {
    let name = |id: &str| games.iter().find(|g| g.id == id).map(|g| g.name.clone()).unwrap_or_else(|| id.to_string());
    let mut rows: Vec<Row> = vec![];
    for (i, p) in local {
        rows.push(Row { game_name: name(&i.game), game: i.game.clone(), rel: i.rel.clone(), local: Some((i, p)), remote: None });
    }
    for r in remote {
        // un jeu inconnu de cet ordinateur n'a aucun emplacement : on ne peut rien en faire ici
        if !games.iter().any(|g| g.id == r.game) {
            continue;
        }
        match rows.iter_mut().find(|x| x.game == r.game && x.rel == r.rel) {
            Some(x) => x.remote = Some(r),
            None => rows.push(Row { game_name: name(&r.game), game: r.game.clone(), rel: r.rel.clone(), local: None, remote: Some(r) }),
        }
    }
    rows.sort_by(|a, b| a.game_name.cmp(&b.game_name).then(a.rel.cmp(&b.rel)));
    rows
}

/// Remplace (ou crée) la sauvegarde d'ici par celle de l'autre ordinateur. Retourne le nombre de fichiers écrits.
pub fn pull(peer: &str, token: &str, games: &[GameLoc], row: &Row) -> Result<usize, String> {
    let r = row.remote.as_ref().ok_or("nothing to pull")?;
    let g = games.iter().find(|g| g.id == row.game).ok_or("unknown game")?;
    let mut n = 0;
    for f in &r.files {
        let bytes = download(peer, token, &row.game, &f.rel)?;
        if sha_hex(&bytes) != f.sha {
            return Err(format!("{}: the download was corrupted, nothing was replaced", f.rel));
        }
        let existing = row.local.as_ref().and_then(|(i, p)| i.files.iter().position(|x| x.rel == f.rel).map(|k| p[k].clone()));
        let target = existing.or_else(|| detect::locate(&g.patterns, &f.rel, g.mirror.as_deref())).ok_or_else(|| format!("No matching save folder for {} on this computer (start {} once here first)", f.rel, g.name))?;
        write_safely(&target, &bytes, true)?;
        n += 1;
    }
    Ok(n)
}

/// Envoie la sauvegarde d'ici à l'autre ordinateur (qui doit l'autoriser).
pub fn push(peer: &str, token: &str, row: &Row) -> Result<usize, String> {
    let (info, paths) = row.local.as_ref().ok_or("nothing to push")?;
    let mut n = 0;
    for (f, p) in info.files.iter().zip(paths) {
        let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
        upload(peer, token, &row.game, &f.rel, &bytes)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup(name: &str) -> (PathBuf, Vec<GameLoc>) {
        let r = std::env::temp_dir().join(format!("ss-sync-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&r);
        fs::create_dir_all(r.join("Saves/Farm_1")).unwrap();
        fs::write(r.join("Saves/Farm_1/Farm_1"), "main-data").unwrap();
        fs::write(r.join("Saves/Farm_1/SaveGameInfo"), "info-data").unwrap();
        let root = r.to_string_lossy().replace('\\', "/");
        (r, vec![GameLoc { id: "sdv".into(), name: "Stardew".into(), patterns: vec![format!("{root}/Saves/*/{{d}}")], mirror: Some("SaveGameInfo".into()) }])
    }

    fn serve(games: &[GameLoc], write: bool) -> Server {
        start("127.0.0.1:0".parse().unwrap(), ServerCfg { token: "secret".into(), allow_write: write, games: games.to_vec() }, true).unwrap()
    }

    #[test]
    fn list_download_with_companion_and_wrong_password() {
        let (r, games) = setup("a");
        let s = serve(&games, false);
        let peer = s.addr.to_string();
        let p = ping(&peer, "secret").unwrap();
        assert!(!p.write);
        assert_eq!(ping(&peer, "nope").unwrap_err(), "Wrong shared password");
        let list = remote_list(&peer, "secret").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].rel, "Farm_1/Farm_1");
        assert_eq!(list[0].files.iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), vec!["Farm_1/Farm_1", "Farm_1/SaveGameInfo"]);
        assert_eq!(download(&peer, "secret", "sdv", "Farm_1/SaveGameInfo").unwrap(), b"info-data");
        assert!(download(&peer, "secret", "sdv", "../../etc/passwd").is_err());
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn writes_are_refused_unless_allowed_and_checked() {
        let (r, games) = setup("b");
        let ro = serve(&games, false);
        let e = upload(&ro.addr.to_string(), "secret", "sdv", "Farm_1/Farm_1", b"x").unwrap_err();
        assert!(e.contains("does not accept"), "{e}");
        assert_eq!(fs::read(r.join("Saves/Farm_1/Farm_1")).unwrap(), b"main-data");

        let rw = serve(&games, true);
        let peer = rw.addr.to_string();
        upload(&peer, "secret", "sdv", "Farm_1/Farm_1", b"NEW").unwrap();
        assert_eq!(fs::read(r.join("Saves/Farm_1/Farm_1")).unwrap(), b"NEW");
        // une copie de sécurité de l'ancienne version a été faite
        assert!(fs::read_dir(r.join("Saves/Farm_1/savestudio-backups")).unwrap().count() >= 1);
        // une nouvelle ferme est créée au bon endroit, mais pas n'importe où
        upload(&peer, "secret", "sdv", "Farm_2/Farm_2", b"second").unwrap();
        assert_eq!(fs::read(r.join("Saves/Farm_2/Farm_2")).unwrap(), b"second");
        assert!(upload(&peer, "secret", "sdv", "Farm_2/evil.exe", b"x").unwrap_err().contains("does not look like"));
        assert!(upload(&peer, "secret", "sdv", "../evil/evil", b"x").is_err());
        assert!(upload(&peer, "secret", "unknowngame", "a/a", b"x").is_err());
        assert_eq!(rw.writes.load(Ordering::Relaxed), 2);
        let _ = fs::remove_dir_all(r);
    }

    #[test]
    fn pull_and_push_between_two_machines() {
        let (ra, ga) = setup("m1");
        // machine B : dossier vide, mais le dossier fixe existe
        let rb = std::env::temp_dir().join(format!("ss-sync-m2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&rb);
        fs::create_dir_all(rb.join("Saves")).unwrap();
        let rootb = rb.to_string_lossy().replace('\\', "/");
        let gb = vec![GameLoc { patterns: vec![format!("{rootb}/Saves/*/{{d}}")], ..ga[0].clone() }];

        let srv_a = serve(&ga, true);
        let peer_a = srv_a.addr.to_string();
        let rows = compare(&gb, local_items(&gb), remote_list(&peer_a, "secret").unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state(), State::OnlyRemote);
        assert_eq!(pull(&peer_a, "secret", &gb, &rows[0]).unwrap(), 2);
        assert_eq!(fs::read(rb.join("Saves/Farm_1/Farm_1")).unwrap(), b"main-data");
        assert_eq!(fs::read(rb.join("Saves/Farm_1/SaveGameInfo")).unwrap(), b"info-data");

        // B modifie sa copie, la renvoie à A
        fs::write(rb.join("Saves/Farm_1/Farm_1"), "edited-on-B").unwrap();
        let rows = compare(&gb, local_items(&gb), remote_list(&peer_a, "secret").unwrap());
        assert_ne!(rows[0].state(), State::Same);
        assert_eq!(push(&peer_a, "secret", &rows[0]).unwrap(), 2);
        assert_eq!(fs::read(ra.join("Saves/Farm_1/Farm_1")).unwrap(), b"edited-on-B");
        let rows = compare(&gb, local_items(&gb), remote_list(&peer_a, "secret").unwrap());
        assert_eq!(rows[0].state(), State::Same);
        let _ = fs::remove_dir_all(ra);
        let _ = fs::remove_dir_all(rb);
    }

    #[test]
    fn peer_address_forms() {
        assert_eq!(base("100.64.0.2"), "http://100.64.0.2:47653");
        assert_eq!(base("100.64.0.2:9000"), "http://100.64.0.2:9000");
        assert_eq!(base("http://laptop:1/"), "http://laptop:1");
        assert_eq!(base("fd7a:115c:a1e0::1"), "http://[fd7a:115c:a1e0::1]:47653");
    }

    #[test]
    fn percent_encoding_roundtrip() {
        let s = "Farm 1/é&=?";
        assert_eq!(pct_decode(&pct_encode(s)), s);
    }

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}

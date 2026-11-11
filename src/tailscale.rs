//! Lecture de l'état de Tailscale via sa ligne de commande (`tailscale status --json`) : adresse de cet ordinateur
//! et liste des autres machines du réseau. SaveStudio ne parle jamais aux serveurs de Tailscale : il utilise
//! simplement le réseau privé chiffré que Tailscale a déjà mis en place entre vos appareils.
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct Peer {
    pub name: String,
    pub ip: String,
    pub online: bool,
    pub os: String,
}

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct Status {
    pub backend: String,
    pub self_name: String,
    pub self_ip: Option<String>,
    pub peers: Vec<Peer>,
}

/// Adresses Tailscale : 100.64.0.0/10 (IPv4, CGNAT) ou fd7a:115c:a1e0::/48 (IPv6).
pub fn is_tailnet_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.octets()[0] == 100 && (v.octets()[1] & 0xC0) == 64,
        IpAddr::V6(v) => {
            let s = v.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

fn first_v4(v: &Value) -> Option<String> {
    v.as_array()?.iter().filter_map(|x| x.as_str()).find(|s| s.parse::<Ipv4Addr>().is_ok()).map(|s| s.to_string())
}

pub fn parse_status(json: &str) -> Result<Status, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("tailscale: unreadable status ({e})"))?;
    let s = |x: &Value, k: &str| x.get(k).and_then(|y| y.as_str()).unwrap_or("").to_string();
    let me = v.get("Self").cloned().unwrap_or(Value::Null);
    let mut st = Status { backend: s(&v, "BackendState"), self_name: s(&me, "HostName"), self_ip: me.get("TailscaleIPs").and_then(first_v4), peers: vec![] };
    if let Some(o) = v.get("Peer").and_then(|p| p.as_object()) {
        for (_, p) in o {
            if let Some(ip) = p.get("TailscaleIPs").and_then(first_v4) {
                st.peers.push(Peer { name: s(p, "HostName"), ip, online: p.get("Online").and_then(|x| x.as_bool()).unwrap_or(false), os: s(p, "OS") });
            }
        }
    }
    st.peers.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(st)
}

fn candidates() -> Vec<&'static str> {
    if cfg!(target_os = "windows") {
        vec!["tailscale", r"C:\Program Files\Tailscale\tailscale.exe", r"C:\Program Files (x86)\Tailscale\tailscale.exe"]
    } else {
        vec!["tailscale", "/usr/bin/tailscale", "/usr/local/bin/tailscale", "/usr/sbin/tailscale"]
    }
}

fn run(args: &[&str]) -> Result<String, String> {
    let mut last = String::from("Tailscale was not found on this computer");
    for c in candidates() {
        let mut cmd = Command::new(c);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // pas de fenêtre console
        }
        match cmd.output() {
            Ok(o) if o.status.success() => return Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
            Ok(o) => last = format!("tailscale: {}", String::from_utf8_lossy(&o.stderr).trim()),
            Err(_) => {}
        }
    }
    Err(last)
}

/// Interroge Tailscale (peut prendre un instant : à appeler hors du fil de l'interface).
pub fn status() -> Result<Status, String> {
    parse_status(&run(&["status", "--json"])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailnet_ranges() {
        for ok in ["100.64.0.1", "100.100.5.5", "100.127.255.254", "fd7a:115c:a1e0::1"] {
            assert!(is_tailnet_ip(ok.parse().unwrap()), "{ok}");
        }
        for no in ["100.63.0.1", "100.128.0.1", "192.168.1.5", "10.0.0.1", "127.0.0.1", "8.8.8.8", "fe80::1"] {
            assert!(!is_tailnet_ip(no.parse().unwrap()), "{no}");
        }
    }

    #[test]
    fn parses_status_json() {
        let j = r#"{"BackendState":"Running","Self":{"HostName":"desktop","TailscaleIPs":["fd7a:115c:a1e0::1","100.64.0.5"],"Online":true},
          "Peer":{"nodekey:a":{"HostName":"laptop","TailscaleIPs":["100.64.0.9"],"Online":true,"OS":"windows"},
                  "nodekey:b":{"HostName":"phone","TailscaleIPs":["100.64.0.7"],"Online":false,"OS":"android"}}}"#;
        let s = parse_status(j).unwrap();
        assert_eq!((s.backend.as_str(), s.self_name.as_str(), s.self_ip.as_deref()), ("Running", "desktop", Some("100.64.0.5")));
        assert_eq!(s.peers.len(), 2);
        assert_eq!(s.peers[0].name, "laptop"); // en ligne d'abord
        assert!(!s.peers[1].online);
        assert!(parse_status("nope").is_err());
    }
}

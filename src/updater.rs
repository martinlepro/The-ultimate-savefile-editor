//! Téléchargement du pack de jeux (en tâche de fond) : manifeste, fichiers, vérification SHA-256, installation atomique.
use crate::pack::{self, Manifest};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub struct Remote {
    pub manifest: Manifest,
    pub bytes: Vec<u8>,
}

pub enum Msg {
    Progress(usize, usize),
    Done(Result<String, String>),
}

/// Racines de confiance : celles de Mozilla (webpki-roots) + celles du système quand elles existent.
fn tls_config() -> std::sync::Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    #[cfg(not(target_os = "android"))]
    for c in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(c);
    }
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    std::sync::Arc::new(
        rustls::ClientConfig::builder_with_provider(provider).with_safe_default_protocol_versions().expect("protocoles TLS").with_root_certificates(roots).with_no_client_auth(),
    )
}

pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .tls_config(tls_config())
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .user_agent(&format!("SaveStudio/{}", pack::APP_VERSION))
        .build();
    let resp = agent.get(url).call().map_err(|e| match e {
        ureq::Error::Status(c, _) => format!("{url} : HTTP {c}"),
        e => format!("connexion impossible ({e})"),
    })?;
    let mut buf = vec![];
    resp.into_reader().take(32 << 20).read_to_end(&mut buf).map_err(|e| format!("lecture interrompue : {e}"))?;
    Ok(buf)
}

pub fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn fetch_manifest(base: &str) -> Result<Remote, String> {
    let bytes = http_get(&format!("{base}/manifest.json"))?;
    let manifest = pack::parse_manifest(&bytes)?;
    Ok(Remote { manifest, bytes })
}

/// Les fichiers d'emplacements des autres plateformes ne servent à rien ici.
fn wanted(path: &str) -> bool {
    match path.split_once('/') {
        Some((d @ ("windows" | "linux" | "android"), f)) if f.ends_with("locations.svl") => d == pack::platform(),
        _ => true,
    }
}

/// Télécharge tous les fichiers dans `dest.new`, vérifie chaque empreinte, puis remplace `dest` d'un coup.
/// En cas d'erreur, le pack déjà installé n'est pas touché.
pub fn install(base: &str, remote: &Remote, dest: &Path, tx: &Sender<Msg>) -> Result<(), String> {
    let tmp: PathBuf = dest.with_extension("new");
    let old: PathBuf = dest.with_extension("old");
    let _ = std::fs::remove_dir_all(&tmp);
    let res = (|| -> Result<(), String> {
        std::fs::create_dir_all(&tmp).map_err(|e| format!("création de {} impossible : {e}", tmp.display()))?;
        let list: Vec<(&String, &String)> = remote.manifest.files.iter().filter(|(p, _)| wanted(p)).collect();
        for (i, (rel, hash)) in list.iter().enumerate() {
            let _ = tx.send(Msg::Progress(i, list.len()));
            let data = http_get(&format!("{base}/{rel}"))?;
            if &sha256_hex(&data) != *hash {
                return Err(format!("empreinte incorrecte pour {rel} : fichier refusé"));
            }
            let out = tmp.join(rel);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            std::fs::write(&out, data).map_err(|e| format!("écriture de {rel} : {e}"))?;
        }
        std::fs::write(tmp.join("manifest.json"), &remote.bytes).map_err(|e| e.to_string())?;
        // le pack doit se relire correctement avant de remplacer l'ancien
        pack::load(&tmp).map(|_| ())?;
        let _ = tx.send(Msg::Progress(list.len(), list.len()));
        Ok(())
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&old);
    if dest.exists() {
        std::fs::rename(dest, &old).map_err(|e| format!("remplacement impossible : {e}"))?;
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::rename(&old, dest); // on remet l'ancien pack
        return Err(format!("installation impossible : {e}"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Version du pack installé (None si aucun).
#[cfg(test)]
pub fn installed_version(dir: &Path) -> Option<String> {
    let b = std::fs::read(dir.join("manifest.json")).ok()?;
    pack::parse_manifest(&b).ok().map(|m| m.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn other_platform_locations_are_skipped() {
        assert!(wanted("fonts/x.ttf"));
        assert!(wanted(&format!("{}/locations.svl", pack::platform())));
        let other = if pack::platform() == "windows" { "linux" } else { "windows" };
        assert!(!wanted(&format!("{other}/locations.svl")));
    }

    /// Essai réseau réel : `cargo test --release -- --ignored real_download`
    #[test]
    #[ignore]
    fn real_download() {
        let dir = std::env::temp_dir().join(format!("savestudio-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let base = std::env::var("SAVESTUDIO_TEST_BASE").unwrap_or_else(|_| pack::REPO_RAW.to_string());
        let r = fetch_manifest(&base).expect("manifeste");
        let (tx, _rx) = std::sync::mpsc::channel();
        install(&base, &r, &dir.join("pack"), &tx).expect("installation");
        let p = pack::load(&dir.join("pack")).expect("lecture");
        assert!(p.games.len() >= 5, "jeux : {}", p.games.len());
        assert_eq!(installed_version(&dir.join("pack")).unwrap(), r.manifest.version);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

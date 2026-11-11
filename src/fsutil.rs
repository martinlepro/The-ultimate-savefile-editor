//! Écriture sûre des sauvegardes : copie de sécurité puis fichier temporaire + renommage.
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn backup_dir(path: &Path) -> PathBuf {
    path.parent().unwrap_or(Path::new(".")).join("savestudio-backups")
}

pub fn write_safely(path: &Path, bytes: &[u8], backup: bool) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("Cannot create {}: {e}", d.display()))?;
    }
    if backup && path.exists() {
        let dir = backup_dir(path);
        let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(path, dir.join(format!("{name}.{stamp}.bak"))).map(|_| ())).map_err(|e| format!("Backup failed, nothing written: {e}"))?;
    }
    let tmp = path.with_extension("savestudio-tmp");
    std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| format!("Write failed: {e}"))
}

pub fn latest_backup(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mut best: Option<(u64, PathBuf)> = None;
    for e in std::fs::read_dir(backup_dir(path)).ok()?.flatten() {
        let f = e.file_name().to_string_lossy().into_owned();
        if let Some(ts) = f.strip_prefix(&format!("{name}.")).and_then(|r| r.strip_suffix(".bak")).and_then(|t| t.parse::<u64>().ok()) {
            if best.as_ref().map_or(true, |(b, _)| ts > *b) {
                best = Some((ts, e.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

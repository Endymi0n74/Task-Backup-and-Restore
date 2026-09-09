//! Journalisation fichier simple pour le CLI : un fichier horodate par jour
//! dans `%LOCALAPPDATA%\tsbak\logs\` (le meme dossier que l'interface
//! graphique Task backup and restore), avec retention de 14 jours.
//!
//! La journalisation est *best-effort* : elle ne fait jamais echouer le
//! programme (les erreurs d'ecriture sont ignorees silencieusement).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

/// Pointeur global vers le journal ouvert (None tant que `init_logging`
/// n'a pas ete appele, ou si l'ouverture a echoue).
static LOG: OnceLock<Mutex<Option<LogFile>>> = OnceLock::new();

struct LogFile {
    writer: fs::File,
    day: String,
}

/// Repertoire des journaux : variable d'environnement `TSBAK_LOG_DIR` si
/// definie (tests), sinon `%LOCALAPPDATA%\tsbak\logs`, sinon un fallback dans
/// le dossier temporaire (plateformes sans LOCALAPPDATA).
pub fn log_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TSBAK_LOG_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("tsbak-logs"));
    base.join("tsbak").join("logs")
}

/// Chemin du fichier de journal du jour courant.
pub fn log_file_path() -> PathBuf {
    log_dir().join(format!(
        "tsbak-{}.log",
        Local::now().format("%Y-%m-%d")
    ))
}

/// Initialise la journalisation : cree le repertoire, purge les journaux de
/// plus de 14 jours et ouvre le fichier du jour en mode ajout. Sans effet si
/// deja initialise. Les erreurs sont ignorees (journalisation best-effort).
pub fn init_logging() {
    let dir = log_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    purge_older_than(&dir, 14);
    let day = Local::now().format("%Y-%m-%d").to_string();
    let writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path())
        .ok();
    if let Some(writer) = writer {
        let _ = LOG.set(Mutex::new(Some(LogFile { writer, day })));
    }
}

/// Purge les fichiers `tsbak-YYYY-MM-DD.log` plus vieux que `keep_days` jours.
fn purge_older_than(dir: &Path, keep_days: i64) {
    let cutoff = Local::now().date_naive() - chrono::Duration::days(keep_days);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(stem) = name
            .strip_prefix("tsbak-")
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        if let Ok(date) = chrono::NaiveDate::parse_from_str(stem, "%Y-%m-%d") {
            if date < cutoff {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn write(level: &str, msg: &str) {
    let Some(guard) = LOG.get() else {
        return;
    };
    let Ok(mut slot) = guard.lock() else {
        return;
    };
    let Some(log) = slot.as_mut() else {
        return;
    };
    // Rotation a minuit : reouvre le fichier du nouveau jour si besoin.
    let now = Local::now();
    let day = now.format("%Y-%m-%d").to_string();
    if log.day != day {
        if let Ok(writer) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path())
        {
            log.writer = writer;
            log.day = day;
        }
    }
    let line = format!("[{}] {} {}\n", now.format("%H:%M:%S"), level, msg);
    let _ = log.writer.write_all(line.as_bytes());
    let _ = log.writer.flush();
}

/// Journalise une ligne de niveau INFO.
pub fn info(msg: impl AsRef<str>) {
    write("INFO", msg.as_ref());
}

/// Journalise une ligne de niveau WARN.
pub fn warn(msg: impl AsRef<str>) {
    write("WARN", msg.as_ref());
}

/// Journalise une ligne de niveau ERROR.
pub fn error(msg: impl AsRef<str>) {
    write("ERROR", msg.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ecrit_et_alterne_les_journaux() {
        let dir = tempdir().unwrap();
        // Dirige la journalisation vers le dossier de test.
        std::env::set_var("TSBAK_LOG_DIR", dir.path());
        init_logging();
        info("demarrage du test");
        error("erreur simulee");

        let path = log_file_path();
        assert!(path.exists(), "le fichier journal doit exister");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("INFO demarrage du test"));
        assert!(content.contains("ERROR erreur simulee"));

        // Purge : un vieux journal doit disparaitre a la prochaine init.
        let old = dir.path().join("tsbak-2020-01-01.log");
        fs::write(&old, "ancien").unwrap();
        assert!(old.exists());
        init_logging();
        assert!(!old.exists(), "les journaux de plus de 14 jours sont purges");

        std::env::remove_var("TSBAK_LOG_DIR");
    }
}
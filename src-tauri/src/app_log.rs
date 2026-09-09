//! Journalisation de Task backup and restore : un fichier horodaté par jour dans
//! `%LOCALAPPDATA%\tsbak\logs\` (rotation journalière, rétention 14 jours),
//! plus un anneau en mémoire (1000 lignes) servi à l'interface en temps réel.
//!
//! Règle de sécurité : cette API ne manipule **jamais** de mot de passe.
//! Aucune fonction n'accepte de secret — les appels de logs ne contiennent
//! que des chemins de tâches, noms d'utilisateurs et compteurs. Les mots de
//! passe restent dans l'état backend de l'application et ne transitent par
//! aucun chemin de journalisation (cf. `commands.rs`).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

/// Une ligne de journal prête à être affichée par l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    /// Numéro de séquence croissant, sert de curseur au rafraîchissement incrémental.
    pub seq: u64,
    /// Horodatage local, ex: "2026-09-08 19:04:24".
    pub timestamp: String,
    /// Niveau : INFO, WARN ou ERROR.
    pub level: String,
    /// Message déjà formaté, sans secret.
    pub message: String,
}

/// Taille maximale de l'anneau en mémoire.
const RING_CAPACITY: usize = 1000;

/// Rétention des fichiers de logs en jours.
const RETENTION_DAYS: u64 = 14;

/// Le journal partagé de l'application (détenu dans l'état Tauri).
pub struct AppLog {
    inner: Mutex<Inner>,
}

struct Inner {
    dir: PathBuf,
    day: String,
    file: Option<File>,
    ring: Vec<LogLine>,
    next_seq: u64,
}

/// Dossier de logs par défaut : `%LOCALAPPDATA%\tsbak\logs`.
/// Repli sur `./logs` (dossier courant) si la variable est absente.
pub fn default_log_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("tsbak").join("logs"))
        .unwrap_or_else(|_| PathBuf::from("logs"))
}

/// Supprime les fichiers `tsbak-*.log` plus vieux que `days` jours.
fn purge_old_logs(dir: &PathBuf, days: u64) {
    let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(stamp) = name.strip_prefix("tsbak-").and_then(|n| n.strip_suffix(".log")) {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(stamp, "%Y-%m-%d") {
                    let expires = cutoff.date_naive();
                    if date < expires {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

impl AppLog {
    /// Initialise le journal dans `default_log_dir()` et écrit la ligne d'ouverture.
    pub fn init() -> Self {
        Self::with_dir(default_log_dir())
    }

    /// Initialise le journal dans un dossier précis (utilisé par les tests).
    pub fn with_dir(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let log = AppLog {
            inner: Mutex::new(Inner {
                dir,
                day: String::new(),
                file: None,
                ring: Vec::new(),
                next_seq: 0,
            }),
        };
        log.info("Journal démarré");
        log
    }

    /// Dossier où sont écrits les fichiers de logs.
    pub fn dir(&self) -> PathBuf {
        self.inner.lock().unwrap().dir.clone()
    }

    /// Journalise un message informatif.
    pub fn info(&self, message: &str) {
        self.write("INFO", message);
    }

    /// Journalise un avertissement.
    pub fn warn(&self, message: &str) {
        self.write("WARN", message);
    }

    /// Journalise une erreur.
    pub fn error(&self, message: &str) {
        self.write("ERROR", message);
    }

    /// Lignes de séquence supérieure ou égale à `after_seq` (curseur
    /// inclusif : l'appelant passe `dernière séquence connue + 1`, ou 0 pour
    /// tout récupérer depuis le début), avec la dernière séquence connue.
    pub fn lines_after(&self, after_seq: u64) -> (Vec<LogLine>, u64) {
        let inner = self.inner.lock().unwrap();
        let lines: Vec<LogLine> = inner
            .ring
            .iter()
            .filter(|l| l.seq >= after_seq)
            .cloned()
            .collect();
        let last = inner.next_seq.saturating_sub(1);
        (lines, last)
    }

    fn write(&self, level: &str, message: &str) {
        let now = chrono::Local::now();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let day = now.format("%Y-%m-%d").to_string();
        let line = format!("{timestamp} [{level}] {message}");

        let mut inner = self.inner.lock().unwrap();

        // Rotation journalière : on ouvre le fichier du jour si besoin et on
        // purge les fichiers trop anciens à cette occasion.
        if inner.file.is_none() || inner.day != day {
            let path = inner.dir.join(format!("tsbak-{day}.log"));
            inner.file = OpenOptions::new().create(true).append(true).open(path).ok();
            purge_old_logs(&inner.dir, RETENTION_DAYS);
        }
        inner.day = day;
        if let Some(f) = inner.file.as_mut() {
            let _ = writeln!(f, "{line}");
        }

        // Anneau en mémoire, borné à RING_CAPACITY lignes.
        let seq = inner.next_seq;
        if inner.ring.len() == RING_CAPACITY {
            inner.ring.remove(0);
        }
        inner.ring.push(LogLine {
            seq,
            timestamp,
            level: level.to_string(),
            message: message.to_string(),
        });
        inner.next_seq += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_to_file_and_ring() {
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(dir.path().to_path_buf());
        log.info("premier");
        log.warn("avertissement");
        log.error("problème");

        let (lines, last) = log.lines_after(0);
        // La ligne d'ouverture « Journal démarré » (séquence 0) est incluse.
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].level, "INFO");
        assert_eq!(lines[0].message, "Journal démarré");
        assert_eq!(lines[1].message, "premier");
        assert_eq!(last, 3);

        // Curseur inclusif : rien après la dernière séquence connue.
        let (empty, last2) = log.lines_after(last + 1);
        assert!(empty.is_empty());
        assert_eq!(last2, last);

        // Le fichier du jour contient les mêmes lignes.
        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(files.len(), 1);
        let content = fs::read_to_string(&files[0]).unwrap();
        assert!(content.contains("[INFO] premier"));
        assert!(content.contains("[ERROR] problème"));
    }

    #[test]
    fn incremental_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(dir.path().to_path_buf());
        // Tout depuis le début (séquence 0 incluse).
        let (all, last0) = log.lines_after(0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message, "Journal démarré");
        log.info("après le démarrage");
        // Curseur = dernière séquence connue + 1.
        let (lines, last) = log.lines_after(last0 + 1);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "après le démarrage");
        assert_eq!(last, last0 + 1);
    }

    #[test]
    fn ring_capacity_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(dir.path().to_path_buf());
        for i in 0..(RING_CAPACITY + 50) {
            log.info(&format!("ligne {i}"));
        }
        let (lines, _) = log.lines_after(0);
        assert!(lines.len() <= RING_CAPACITY, "l'anneau doit être borné");
        // Les lignes les plus récentes sont conservées.
        assert!(lines.last().unwrap().message.contains("ligne 5000") || {
            let n = RING_CAPACITY + 49;
            lines.last().unwrap().message == format!("ligne {n}")
        });
    }

    #[test]
    fn no_secret_api_surface() {
        // Garde-fou de conception : le type de journal n'expose aucune
        // fonction acceptant un mot de passe, et aucune fonction ne dérive
        // Debug sur des secrets (rien à vérifier à l'exécution ici, ce test
        // documente la propriété pour les futurs lecteurs).
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(dir.path().to_path_buf());
        log.info("toute ligne est déjà formatée sans secret");
        assert!(log.dir().exists());
    }

    #[test]
    fn purge_removes_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("tsbak-2020-01-01.log");
        let fresh = dir.path().join("tsbak-2099-01-01.log");
        fs::write(&old, "ancien").unwrap();
        fs::write(&fresh, "récent").unwrap();
        purge_old_logs(&dir.path().to_path_buf(), RETENTION_DAYS);
        assert!(!old.exists(), "le fichier trop ancien doit être supprimé");
        assert!(fresh.exists(), "le fichier récent doit être conservé");
    }
}
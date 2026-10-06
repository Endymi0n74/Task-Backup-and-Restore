//! Import en **processus enfant élevé** (sidecar/helper), pour éviter le
//! redémarrage complet de l'interface lors d'un import nécessitant les
//! droits administrateur.
//!
//! Deux moitiés, dans le même exécutable :
//! - **Mode helper** (`--helper-import` : détecté dans `main.rs` avant le
//!   démarrage de Tauri) : exécute l'import réel sans fenêtre, journalise
//!   dans le même dossier de logs (`%LOCALAPPDATA%\tsbak\logs\`), écrit un
//!   fichier de **résultat JSON** puis supprime le fichier de réponses.
//! - **Côté interface** ([`run_elevated_import`]) : écrit le fichier de
//!   réponses temporaire (décisions + mots de passe en mémoire), lance le
//!   même exe avec le verbe `runas` via `ShellExecuteExW` (invite UAC),
//!   attend la fin du processus, lit le rapport et nettoie les fichiers
//!   temporaires.
//!
//! Sécurité : le fichier de réponses contient brièvement les mots de passe
//! sur disque (dans `%TEMP%\task-backup-restore`, ACL utilisateur uniquement) — c'est
//! la seule façon de transférer des secrets à un processus élevé séparé. Il
//! est supprimé par le helper **et** par l'interface, dans tous les chemins
//! (y compris les erreurs), et **aucun** mot de passe n'est jamais journalisé
//! ni écrit ailleurs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use tsbak::answers::AnswerFile;
use tsbak::import::{build_plan, execute_plan, load_and_verify, ImportOptions};
use tsbak::password::PasswordResolver;
use tsbak::scheduler::TaskSchedulerApi;
use tsbak::wizard::NullInteractor;

use crate::app_log::AppLog;
use crate::commands::{self, ImportDecisions, ReportView};

/// Argument qui bascule l'exe en mode helper tête nue (sans Tauri).
pub const HELPER_FLAG: &str = "--helper-import";

/// Dossier de travail temporaire (fichiers de réponses et de résultat).
fn temp_work_dir() -> PathBuf {
    std::env::temp_dir().join("task-backup-restore")
}

/// Suffixe unique (pid + horodatage nanoseconde) pour les fichiers temporaires.
fn rand_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

/// Âge au-delà duquel un fichier de travail est considéré comme orphelin :
/// une invite UAC ne reste pas en attente plus d'une heure (au-delà,
/// l'import est considéré comme abandonné et le secret est purgé).
const WORK_FILE_MAX_AGE: Duration = Duration::from_secs(3600);

/// Purge les fichiers de travail orphelins (plus vieux d'une heure) :
/// appelée à l'ouverture de l'interface et avant chaque import élevé.
/// Retourne le nombre de fichiers supprimés.
pub(crate) fn purge_stale_work_files() -> usize {
    purge_work_files_in(&temp_work_dir(), Some(WORK_FILE_MAX_AGE))
}

/// Purge **tous** les fichiers de travail du dossier temporaire (fichier de
/// réponses contenant brièvement les mots de passe, fichiers de résultat) :
/// appelée à la fermeture de l'interface, où aucun secret ne doit survivre.
/// Retourne le nombre de fichiers supprimés.
pub(crate) fn purge_work_files() -> usize {
    purge_work_files_in(&temp_work_dir(), None)
}

/// Supprime les fichiers `answers-*` / `result-*` de `dir` — les plus vieux
/// que `max_age` seulement (tous si `None`). Les autres fichiers sont
/// laissés intacts. Le dossier inexistant n'est pas une erreur.
fn purge_work_files_in(dir: &Path, max_age: Option<Duration>) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("answers-") && !name.starts_with("result-") {
            continue;
        }
        if let Some(age) = max_age {
            let older = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|mtime| now.duration_since(mtime).ok())
                .map(|elapsed| elapsed > age)
                .unwrap_or(false);
            if !older {
                continue;
            }
        }
        if fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

// ---------------------------------------------------------------------------
// Mode helper (tête nue)
// ---------------------------------------------------------------------------

/// Résultat écrit par le helper dans le fichier `--result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperResult {
    /// Succès complet (rapport produit, aucune erreur fatale).
    pub ok: bool,
    /// Message d'erreur fatal, si le rapport n'a pas pu être produit.
    pub error: Option<String>,
    /// Rapport d'import, si l'import a été exécuté (même partiellement bloqué).
    pub report: Option<ReportView>,
}

/// Point d'entrée headless : lit les arguments, exécute l'import réel et
/// écrit le fichier de résultat. Retourne le code de sortie du processus.
pub fn run_helper_args(args: &[String]) -> i32 {
    let log = AppLog::init();
    log.info("Helper d'import démarré (processus administrateur)");

    let get = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let (dir, answers_path, result_path) = match (get("--dir"), get("--answers"), get("--result")) {
        (Some(d), Some(a), Some(r)) => (d, a, r),
        _ => {
            log.error("Helper : arguments manquants (--dir, --answers, --result)");
            return 2;
        }
    };
    let dry_run = args.iter().any(|a| a == "--dry-run");
    // Dossier cible optionnel du planificateur (--folder), transmis tel quel.
    let target_folder = get("--folder");

    // Charge le fichier de réponses (contenant les mots de passe), puis le
    // supprime immédiatement : il ne doit jamais rester sur disque.
    let answers = match AnswerFile::load(Path::new(&answers_path)) {
        Ok(a) => {
            let _ = std::fs::remove_file(&answers_path);
            a
        }
        Err(e) => {
            let _ = std::fs::remove_file(&answers_path);
            log.error(&format!("Helper : fichier de réponses invalide : {e}"));
            return 2;
        }
    };

    let result = match commands::make_scheduler() {
        Ok(scheduler) => {
            let outcome =
                helper_import_with(&log, scheduler.as_ref(), Path::new(&dir), &answers, dry_run, target_folder.as_deref());
            match outcome {
                Ok(report) => HelperResult { ok: true, error: None, report: Some(report) },
                Err(e) => HelperResult { ok: false, error: Some(e), report: None },
            }
        }
        Err(e) => HelperResult { ok: false, error: Some(e), report: None },
    };

    let write_ok = write_json(Path::new(&result_path), &result).is_ok();
    let code = if result.ok && write_ok { 0 } else { 2 };
    log.info(&format!("Helper terminé (code {code})"));
    code
}

/// Cœur du helper, testable avec un scheduler simulé : charge l'archive,
/// construit le plan avec les décisions/mots de passe du fichier de
/// réponses, exécute (ou simule) et journalise. Aucun secret n'est loggé.
pub(crate) fn helper_import_with(
    log: &AppLog,
    scheduler: &dyn TaskSchedulerApi,
    dir: &Path,
    answers: &AnswerFile,
    dry_run: bool,
    target_folder: Option<&str>,
) -> Result<ReportView, String> {
    let (manifest, xmls) = load_and_verify(dir).map_err(|e| e.to_string())?;

    let resolver = PasswordResolver::new(None, Some(answers.passwords.clone()), false);
    let options = ImportOptions {
        conflict_policy: None,
        skip_password_tasks: false,
        user_map: answers.user_map.clone(),
        answers: Some(answers),
        target_folder: target_folder.map(str::to_string),
    };

    let plan = build_plan(scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor)
        .map_err(|e| e.to_string())?;

    let verb = if dry_run { "Simulation d'import (helper)" } else { "Import (helper)" };
    log.info(&format!("{verb} depuis {} ({} tâche(s))", dir.display(), plan.len()));

    let report = execute_plan(scheduler, &plan, dry_run).map_err(|e| e.to_string())?;

    // Étapes puis synthèse : même journalisation que la commande
    // `import_execute` (partagée dans `commands`), aucun secret dedans.
    let view = commands::report_view(&report, dry_run);
    commands::log_import_report(log, verb, &view);
    Ok(view)
}

// ---------------------------------------------------------------------------
// Côté interface : lancement du processus enfant élevé
// ---------------------------------------------------------------------------

/// Lance l'import réel dans un processus enfant **élevé** (invite UAC) et
/// attend son rapport. L'interface reste ouverte : seul le processus helper
/// est élevé. Les mots de passe transitent par un fichier de réponses
/// temporaire, supprimé dans tous les cas.
pub fn run_elevated_import(
    dir: &str,
    decisions: &ImportDecisions,
    passwords: &HashMap<String, String>,
    target_folder: Option<&str>,
) -> Result<ReportView, String> {
    // Fichiers de travail orphelins d'une session précédente (interface
    // fermée avant la fin de l'UAC, plantage…) : nettoyés avant chaque
    // import — un fichier de réponses (secret) ne doit jamais s'accumuler.
    purge_stale_work_files();

    let work = temp_work_dir();
    std::fs::create_dir_all(&work)
        .map_err(|e| format!("impossible de créer {} : {e}", work.display()))?;
    let suffix = rand_suffix();
    let answers_path = work.join(format!("answers-{suffix}.json"));
    let result_path = work.join(format!("result-{suffix}.json"));

    // Nettoyage systématique : le fichier de réponses contient les mots de
    // passe et ne doit jamais rester sur disque, même en cas d'erreur.
    let cleanup = |answers_path: &Path, result_path: &Path| {
        let _ = std::fs::remove_file(answers_path);
        let _ = std::fs::remove_file(result_path);
    };

    // Décisions de l'interface + mots de passe mémorisés en mémoire (le
    // fichier de réponses est la seule écriture transitoire d'un secret).
    let mut answers = commands::build_answers(Some(decisions));
    answers.passwords = passwords.clone();
    if let Err(e) = write_json(&answers_path, &answers) {
        cleanup(&answers_path, &result_path);
        return Err(format!("impossible d'écrire le fichier de réponses : {e}"));
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let folder_arg = target_folder
        .map(|f| format!(" --folder {}", quote_arg(f)))
        .unwrap_or_default();
    let params = format!(
        "{HELPER_FLAG} --dir {} --answers {} --result {}{folder_arg}",
        quote_arg(dir),
        quote_arg(&answers_path.to_string_lossy()),
        quote_arg(&result_path.to_string_lossy())
    );

    let spawn = spawn_elevated_and_wait(&exe, &params);
    if let Err(e) = spawn {
        cleanup(&answers_path, &result_path);
        return Err(e);
    }

    // Le helper a écrit son résultat (ou a échoué avant) : on le relit.
    let raw = match std::fs::read_to_string(&result_path) {
        Ok(r) => r,
        Err(_) => {
            cleanup(&answers_path, &result_path);
            return Err(
                "le processus administrateur s'est terminé sans produire de résultat (élévation annulée ?)".to_string(),
            );
        }
    };
    let result: HelperResult = serde_json::from_str(&raw).map_err(|e| {
        cleanup(&answers_path, &result_path);
        format!("résultat du processus administrateur illisible : {e}")
    })?;

    cleanup(&answers_path, &result_path);

    if !result.ok {
        return Err(result.error.unwrap_or_else(|| "échec du processus administrateur".to_string()));
    }
    result
        .report
        .ok_or_else(|| "le processus administrateur n'a pas produit de rapport".to_string())
}

/// Échappe un argument pour la ligne de commande Windows (les guillemets
/// dans les chemins sont pathologiques et retirés).
fn quote_arg(s: &str) -> String {
    format!("\"{}\"", s.replace('"', ""))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, bytes)
}

/// Lance `exe params` avec le verbe `runas` (invite UAC) et attend la fin du
/// processus. En cas d'annulation de l'UAC, `ShellExecuteExW` échoue (5).
#[cfg(windows)]
fn spawn_elevated_and_wait(exe: &Path, params: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let to_wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let exe_u16 = to_wide(&exe.to_string_lossy());
    let params_u16 = to_wide(params);
    let verb_u16 = to_wide("runas");

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb_u16.as_ptr()),
        lpFile: PCWSTR(exe_u16.as_ptr()),
        lpParameters: PCWSTR(params_u16.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut sei).map_err(|e| {
            if e.code().0 == 5 {
                "élévation refusée ou annulée (UAC)".to_string()
            } else {
                format!("la demande d'élévation a échoué : {e}")
            }
        })?;

        let handle = sei.hProcess;
        if handle.is_invalid() {
            return Err("le processus administrateur n'a pas été créé".to_string());
        }
        WaitForSingleObject(handle, INFINITE);
        let _ = CloseHandle(handle);
    }
    Ok(())
}

#[cfg(not(windows))]
fn spawn_elevated_and_wait(_exe: &Path, _params: &str) -> Result<(), String> {
    Err("le processus enfant élevé nécessite Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_log::AppLog;
    use std::collections::HashMap;
    use tsbak::answers::ConflictDecision;
    use tsbak::export::{export, PatternFilter};
    use tsbak::model::LogonType;
    use tsbak::scheduler::mock::{MockScheduler, MockTask};

    fn source_scheduler() -> MockScheduler {
        MockScheduler::new(true)
            .with_task(
                "\\Backup\\Nightly",
                MockTask {
                    xml: "<Task xmlns=\"x\"><Actions><Exec><Command>a.exe</Command></Exec></Actions></Task>"
                        .to_string(),
                    user_id: Some("NT AUTHORITY\\SYSTEM".to_string()),
                    logon_type: LogonType::ServiceAccount,
                },
            )
            .with_task(
                "\\Backup\\Weekly",
                MockTask {
                    xml: "<Task xmlns=\"x\"><Actions><Exec><Command>b.exe</Command></Exec></Actions></Task>"
                        .to_string(),
                    user_id: Some("DOMAIN\\alice".to_string()),
                    logon_type: LogonType::Password,
                },
            )
    }

    fn make_archive(dir: &Path) {
        let source = source_scheduler();
        let filter = PatternFilter::new(vec![], vec![]);
        export(&source, dir, true, &filter, "SOURCE-PC").expect("export simulé");
    }

    #[test]
    fn helper_import_writes_and_logs_without_secret() {
        let log_dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(log_dir.path().to_path_buf());
        let archive = tempfile::tempdir().unwrap();
        make_archive(archive.path());

        let mut answers = AnswerFile::default();
        answers.passwords.insert("DOMAIN\\alice".to_string(), "s3cret".to_string());

        let target = MockScheduler::new(true);
        let report = helper_import_with(&log, &target, archive.path(), &answers, false, None).unwrap();
        assert_eq!(report.created.len(), 2);
        assert_eq!(report.exit_code, 0);
        assert_eq!(target.registration_count(), 2, "l'import réel doit écrire");

        // Le fichier de logs ne doit jamais contenir le mot de passe.
        let files: Vec<_> = std::fs::read_dir(log_dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        let content = files.iter().map(|f| std::fs::read_to_string(f).unwrap()).collect::<Vec<_>>().join("\n");
        assert!(!content.contains("s3cret"), "le mot de passe ne doit jamais être journalisé");
        assert!(content.contains("Import (helper)"));
    }

    #[test]
    fn helper_restores_under_target_folder() {
        let log_dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(log_dir.path().to_path_buf());
        let archive = tempfile::tempdir().unwrap();
        make_archive(archive.path());

        let mut answers = AnswerFile::default();
        answers.passwords.insert("DOMAIN\\alice".to_string(), "s3cret".to_string());

        let target = MockScheduler::new(true);
        let report = helper_import_with(&log, &target, archive.path(), &answers, false, Some("\\Restore-2026"))
            .unwrap();
        assert_eq!(report.created.len(), 2);
        // La structure source est preservée sous le dossier cible.
        assert!(target.task_exists("\\Restore-2026\\Backup\\Nightly").unwrap());
        assert!(target.task_exists("\\Restore-2026\\Backup\\Weekly").unwrap());
        assert!(!target.task_exists("\\Backup\\Nightly").unwrap(), "rien ne doit être écrit au chemin source");
    }

    #[test]
    fn helper_dry_run_never_writes() {
        let log_dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(log_dir.path().to_path_buf());
        let archive = tempfile::tempdir().unwrap();
        make_archive(archive.path());

        let answers = AnswerFile::default();
        let target = MockScheduler::new(true);
        let report = helper_import_with(&log, &target, archive.path(), &answers, true, None).unwrap();
        assert_eq!(report.created.len(), 1, "la tâche SYSTEM passe sans mot de passe");
        assert_eq!(report.blocked.len(), 1, "la tâche avec mot de passe reste bloquée");
        assert_eq!(target.registration_count(), 0, "le dry-run ne doit rien écrire");
    }

    #[test]
    fn helper_blocks_without_password() {
        let log_dir = tempfile::tempdir().unwrap();
        let log = AppLog::with_dir(log_dir.path().to_path_buf());
        let archive = tempfile::tempdir().unwrap();
        make_archive(archive.path());

        let answers = AnswerFile::default();
        let target = MockScheduler::new(true);
        let report = helper_import_with(&log, &target, archive.path(), &answers, false, None).unwrap();
        assert_eq!(report.created.len(), 1);
        assert_eq!(report.blocked.len(), 1);
        assert_eq!(report.exit_code, 1, "tâche bloquée => code 1, pas 2");
    }

    #[test]
    fn answer_file_roundtrip_carries_passwords() {
        let mut decisions = ImportDecisions::default();
        decisions.conflicts.insert("\\A".to_string(), "overwrite".to_string());
        decisions.user_map.insert("OLDPC\\bob".to_string(), "NEWPC\\bob".to_string());
        decisions.skip_tasks.push("\\SkipMe".to_string());
        let mut passwords = HashMap::new();
        passwords.insert("NEWPC\\bob".to_string(), "m0t de passe".to_string());

        let mut answers = commands::build_answers(Some(&decisions));
        answers.passwords = passwords;
        assert_eq!(answers.passwords.get("NEWPC\\bob").map(String::as_str), Some("m0t de passe"));
        assert!(matches!(
            answers.conflict_decisions.get("\\A"),
            Some(ConflictDecision::Overwrite)
        ));
        assert_eq!(answers.user_map.get("OLDPC\\bob").map(String::as_str), Some("NEWPC\\bob"));
        assert!(answers.skip_tasks.contains(&"\\SkipMe".to_string()));

        // Sérialisation → rechargement (roundtrip avec AnswerFile::load).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("answers.json");
        write_json(&path, &answers).unwrap();
        let loaded = AnswerFile::load(&path).unwrap();
        assert_eq!(loaded.passwords.get("NEWPC\\bob").map(String::as_str), Some("m0t de passe"));
    }

    #[test]
    fn helper_result_serializes_and_parses() {
        let result = HelperResult {
            ok: true,
            error: None,
            report: Some(ReportView {
                dry_run: false,
                created: vec!["\\A".to_string()],
                updated: vec![],
                skipped: vec![],
                blocked: vec![],
                failed: vec![],
                exit_code: 0,
            }),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result.json");
        write_json(&path, &result).unwrap();
        let parsed: HelperResult = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.report.unwrap().created, vec!["\\A".to_string()]);
    }

    #[test]
    fn quote_arg_handles_paths() {
        assert_eq!(quote_arg(r"C:\Users\a b\dump"), "\"C:\\Users\\a b\\dump\"");
        assert_eq!(quote_arg("x\"y"), "\"xy\"");
    }

    #[test]
    fn purge_work_files_retire_reponses_et_resultats() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("answers-99-1.json"), "secret").unwrap();
        fs::write(dir.path().join("result-99-1.json"), "{}").unwrap();
        // Un fichier sans rapport avec l'import élevé n'est pas touché.
        fs::write(dir.path().join("notes.txt"), "à conserver").unwrap();

        let removed = purge_work_files_in(dir.path(), None);
        assert_eq!(removed, 2, "réponses + résultat doivent être purgés");
        assert!(dir.path().join("notes.txt").exists(), "les autres fichiers restent");
        // Idempotent : rien de plus à supprimer.
        assert_eq!(purge_work_files_in(dir.path(), None), 0);
    }

    #[test]
    fn purge_work_files_respect_lage_impose() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("answers-frais.json"), "secret").unwrap();

        // Fichier récent : conservé avec une limite d'âge (une élévation en
        // attente d'UAC ne doit pas perdre son fichier de réponses).
        let removed = purge_work_files_in(dir.path(), Some(WORK_FILE_MAX_AGE));
        assert_eq!(removed, 0, "un fichier frais ne doit pas être purgé");
        assert!(dir.path().join("answers-frais.json").exists());

        // Dossier inexistant : pas une erreur, rien à supprimer.
        assert_eq!(purge_work_files_in(&dir.path().join("absent"), None), 0);
    }
}
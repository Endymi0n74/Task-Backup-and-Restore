//! Commandes Tauri exposées au frontend (français, interface statique).
//!
//! Flux d'import en deux phases, réutilisant la classification unique de
//! `tsbak` (`build_plan` puis `execute_plan`) :
//! 1. `build_plan` (sans décisions) → liste les tâches à résoudre ;
//! 2. l'utilisateur règle conflits/mappings/mots de passe dans l'interface ;
//! 3. `import_execute` reconstruit le plan avec les décisions puis l'exécute.
//!
//! Sécurité : les mots de passe ne quittent jamais le processus Rust. Ils
//! sont saisis dans un champ masqué, transmis à `import_set_password`,
//! stockés dans l'état backend, et **jamais journalisés ni sérialisés** dans
//! les vues renvoyées à l'interface.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use tsbak::answers::{AnswerFile, ConflictDecision};
use tsbak::export::{export, PatternFilter};
use tsbak::import::{build_plan, execute_plan, load_and_verify, ImportOptions, PlanItem};
use tsbak::model::ImportAction;
use tsbak::password::PasswordResolver;
use tsbak::scheduler::TaskSchedulerApi;
use tsbak::wizard::NullInteractor;

use crate::app_log::AppLog;
use crate::archive;
use crate::helper;
use crate::AppState;

/// État partagé passé à chaque commande (journal + mots de passe en mémoire).
pub type AppStateRef<'a> = State<'a, AppState>;

pub(crate) fn make_scheduler() -> Result<Box<dyn TaskSchedulerApi>, String> {
    #[cfg(windows)]
    {
        tsbak::scheduler::windows_impl::WindowsScheduler::connect()
            .map(|s| Box::new(s) as Box<dyn TaskSchedulerApi>)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        Err("l'accès au planificateur de tâches Windows nécessite Windows".to_string())
    }
}

fn local_host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

// ---------------------------------------------------------------------------
// Vues sérialisables (sans secret)
// ---------------------------------------------------------------------------

/// Une tâche planifiée telle qu'affichée dans l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub path: String,
    pub user_id: Option<String>,
    pub logon_type: String,
}

/// Résumé d'un export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummaryView {
    pub exported: Vec<String>,
    pub skipped_by_filter: usize,
    /// Chemin du .zip créé, si un export en archive a été demandé.
    pub zip_path: Option<String>,
    /// L'archive .zip est chiffrée (AES-256).
    pub zip_encrypted: bool,
}

/// Résumé de validation d'une archive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSummary {
    pub valid: bool,
    pub task_count: usize,
    pub exported_at: Option<String>,
    pub source_host: Option<String>,
    pub error: Option<String>,
}

/// Ligne de plan d'import, sans aucun secret.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemView {
    /// Chemin d'origine dans l'archive (clé des décisions : sauts, conflits).
    pub path: String,
    /// Chemin effectif sur la cible (préfixé par le dossier cible, si défini).
    pub target_path: String,
    /// Libellé lisible de l'action (ex: "CREATE").
    pub action_label: String,
    /// Identifiant machine de l'action, pilote l'affichage des contrôles.
    pub action_kind: String,
    /// Utilisateur cible résolu, si applicable.
    pub target_user: Option<String>,
    /// Utilisateur source (tâches "utilisateur non mappé").
    pub source_user: Option<String>,
    /// Un mot de passe sera requis pour cette tâche lors de l'écriture.
    pub needs_password: bool,
}

/// Décisions de l'utilisateur pour l'import (conflits, mappings, sauts).
/// Les mots de passe ne transitent **pas** ici (voir `import_set_password`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDecisions {
    /// Chemin de tâche -> "overwrite" | "skip".
    #[serde(default)]
    pub conflicts: HashMap<String, String>,
    /// Utilisateur source -> utilisateur cible.
    #[serde(default)]
    pub user_map: HashMap<String, String>,
    /// Tâches à sauter explicitement.
    #[serde(default)]
    pub skip_tasks: Vec<String>,
}

/// Rapport d'exécution d'un import (ou d'une simulation). Sérialisable dans
/// les deux sens : le rapport est aussi produit par le processus helper
/// (`--helper-import`) et relu par l'interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub dry_run: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
    pub blocked: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
    pub exit_code: i32,
}

/// Page de logs pour le rafraîchissement incrémental de l'interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsPage {
    pub lines: Vec<crate::app_log::LogLine>,
    pub last_seq: u64,
}

fn logon_type_label(t: &str) -> String {
    match t {
        "None" => "Aucune",
        "Password" => "Mot de passe",
        "InteractiveTokenOrPassword" => "Jeton interactif ou mot de passe",
        "InteractiveToken" => "Jeton interactif",
        "Group" => "Groupe",
        "ServiceAccount" => "Compte de service",
        "S4U" => "S4U",
        other => other,
    }
    .to_string()
}

fn plan_item_view(item: &PlanItem) -> PlanItemView {
    let (action_kind, target_user, source_user, needs_password) = match &item.action {
        ImportAction::Create => ("create", item.target_user.clone(), None, item.record.requires_password()),
        ImportAction::Update => ("update", item.target_user.clone(), None, item.record.requires_password()),
        ImportAction::SkipIdentical => ("skip_identical", None, None, false),
        ImportAction::SkippedByChoice => ("skipped", None, None, false),
        ImportAction::Conflict => ("conflict", item.target_user.clone(), None, false),
        ImportAction::PasswordRequired { user } => ("password_required", Some(user.clone()), None, true),
        ImportAction::UserUnmapped { source_user } => ("user_unmapped", None, Some(source_user.clone()), false),
    };
    PlanItemView {
        path: item.record.path.clone(),
        target_path: item.target_path.clone(),
        action_label: item.action.label().to_string(),
        action_kind: action_kind.to_string(),
        target_user,
        source_user,
        needs_password,
    }
}

fn report_view(report: &tsbak::model::ExecutionReport, dry_run: bool) -> ReportView {
    ReportView {
        dry_run,
        created: report.created.clone(),
        updated: report.updated.clone(),
        skipped: report.skipped.clone(),
        blocked: report.blocked.clone(),
        failed: report.failed.clone(),
        exit_code: report.exit_code(),
    }
}

// ---------------------------------------------------------------------------
// Commandes
// ---------------------------------------------------------------------------

/// Liste les tâches planifiées (récursif si `recursive`).
#[tauri::command]
pub async fn list_tasks(
    state: AppStateRef<'_>,
    recursive: bool,
) -> Result<Vec<TaskView>, String> {
    let scheduler = make_scheduler()?;
    let handles = scheduler.list_tasks(recursive).map_err(|e| e.to_string())?;
    let mut views = Vec::with_capacity(handles.len());
    for handle in &handles {
        let auth = scheduler.get_task_auth_info(&handle.path).unwrap_or(tsbak::scheduler::TaskAuthInfo {
            user_id: None,
            logon_type: tsbak::model::LogonType::None,
        });
        views.push(TaskView {
            path: handle.path.clone(),
            user_id: auth.user_id,
            logon_type: logon_type_label(&format!("{:?}", auth.logon_type)),
        });
    }
    state
        .log
        .info(&format!("Liste des tâches planifiées : {} trouvée(s)", views.len()));
    Ok(views)
}

/// Exporte les tâches sélectionnées (filtres include/exclude) vers `dir`.
///
/// Si `zip_name` est fourni, un `.zip` est créé dans `dir` et **c'est le
/// seul contenu déposé dans le dossier** : les XML et le `manifest.json`
/// sont d'abord écrits dans un dossier temporaire puis compressés (le
/// dossier temporaire est supprimé, succès ou échec). Chiffré AES-256 si
/// `zip_password` est non vide, Deflate seul sinon. Le mot de passe n'est
/// jamais journalisé.
#[tauri::command]
pub async fn export_tasks(
    state: AppStateRef<'_>,
    dir: String,
    include: Vec<String>,
    exclude: Vec<String>,
    zip_name: Option<String>,
    zip_password: Option<String>,
) -> Result<ExportSummaryView, String> {
    export_tasks_impl(&state.log, &dir, include, exclude, zip_name, zip_password)
}

/// Logique d'export (partagée entre la commande et les tests e2e réels).
fn export_tasks_impl(
    log: &AppLog,
    dir: &str,
    include: Vec<String>,
    exclude: Vec<String>,
    zip_name: Option<String>,
    zip_password: Option<String>,
) -> Result<ExportSummaryView, String> {
    let scheduler = make_scheduler()?;
    export_with(
        scheduler.as_ref(),
        log,
        dir,
        include,
        exclude,
        zip_name,
        zip_password,
    )
}

/// Export sur un planificateur donné (commun à la commande, à l'interface et
/// aux tests unitaires `MockScheduler`, sans accès COM).
///
/// En mode archive, les fichiers d'export sont écrits dans un dossier
/// temporaire (`%TEMP%\tsbak-export\…`) **puis** compressés vers `dir` : le
/// dossier de destination ne reçoit que le `.zip`, jamais les XML ni le
/// `manifest.json` à côté de l'archive. Le dossier temporaire est supprimé
/// dans tous les cas (succès ou échec).
fn export_with(
    scheduler: &dyn TaskSchedulerApi,
    log: &AppLog,
    dir: &str,
    include: Vec<String>,
    exclude: Vec<String>,
    zip_name: Option<String>,
    zip_password: Option<String>,
) -> Result<ExportSummaryView, String> {
    // Nom d'archive validé AVANT toute écriture : un nom invalide ne doit
    // rien laisser sur disque.
    let safe_zip = match &zip_name {
        Some(name) => Some(sanitize_zip_name(name)?),
        None => None,
    };

    // Mode archive : la destination est préparée d'abord (échec rapide si
    // elle est inaccessible), puis l'export part dans un dossier temporaire.
    let staging = match &safe_zip {
        Some(_) => {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("impossible de créer {dir} : {e}"))?;
            Some(new_staging_dir()?)
        }
        None => None,
    };
    let out_dir: &Path = match &staging {
        Some(s) => s.as_path(),
        None => Path::new(dir),
    };
    if let Some(s) = &staging {
        log.info(&format!(
            "Export : écriture temporaire dans {}",
            s.display()
        ));
    }

    let outcome = write_export(
        scheduler,
        log,
        out_dir,
        Path::new(dir),
        &PatternFilter::new(include, exclude),
        safe_zip.as_deref(),
        zip_password.as_deref(),
    );

    // Le dossier temporaire est supprimé dans tous les cas ; s'il reste sur
    // disque (plantage entre-temps), la purge à 7 jours le rattrapera.
    if let Some(s) = &staging {
        if let Err(e) = std::fs::remove_dir_all(s) {
            log.warn(&format!(
                "nettoyage du dossier temporaire {} impossible : {e}",
                s.display()
            ));
        }
    }
    outcome
}

/// Écrit les fichiers d'export (XML bruts + `manifest.json`) dans `out_dir`,
/// puis, si `zip_name` est fourni, l'archive `.zip` **dans `dest_dir`**.
/// En mode archive `out_dir` est le dossier temporaire : `dest_dir` ne
/// reçoit donc que l'archive.
fn write_export(
    scheduler: &dyn TaskSchedulerApi,
    log: &AppLog,
    out_dir: &Path,
    dest_dir: &Path,
    filter: &PatternFilter,
    zip_name: Option<&str>,
    zip_password: Option<&str>,
) -> Result<ExportSummaryView, String> {
    let summary = export(scheduler, out_dir, true, filter, &local_host_name())
        .map_err(|e| e.to_string())?;
    for path in &summary.exported {
        log.info(&format!("Export : {path}"));
    }
    log.info(&format!(
        "Export terminé : {} tâche(s) vers {}, {} ignorée(s) par les filtres",
        summary.exported.len(),
        dest_dir.display(),
        summary.skipped_by_filter.len()
    ));

    // Archive .zip optionnelle, construite depuis le dossier d'export qui
    // vient d'être écrit : manifest.json et empreintes sont embarqués tels
    // quels (aucun recalcul).
    let mut zip_path: Option<String> = None;
    let mut zip_encrypted = false;
    if let Some(name) = zip_name {
        let zip_full = dest_dir.join(name);
        let password = zip_password.filter(|p| !p.is_empty());
        if let Err(e) = archive::zip_dir(log, out_dir, &zip_full, password) {
            // Pas d'archive partielle en cas d'échec.
            let _ = std::fs::remove_file(&zip_full);
            return Err(e);
        }
        zip_encrypted = password.is_some();
        zip_path = Some(zip_full.to_string_lossy().to_string());
    }

    Ok(ExportSummaryView {
        exported: summary.exported,
        skipped_by_filter: summary.skipped_by_filter.len(),
        zip_path,
        zip_encrypted,
    })
}

/// Dossier temporaire unique d'un export en archive
/// (`%TEMP%\tsbak-export\<pid>-<nanos>`), supprimé en fin d'export. Les
/// dossiers laissés par un plantage (> 7 jours) sont purgés à chaque appel.
fn new_staging_dir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join("tsbak-export");
    purge_old_temp_dirs(&base);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("dossier temporaire d'export ({}) : {e}", dir.display()))?;
    Ok(dir)
}

/// Nom de fichier .zip sûr : pas de séparateurs, pas de remontée, extension
/// .zip imposée.
fn sanitize_zip_name(name: &str) -> Result<String, String> {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        return Err("nom d'archive invalide".to_string());
    }
    let with_ext = if trimmed.to_lowercase().ends_with(".zip") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.zip")
    };
    if with_ext.contains('/') || with_ext.contains('\\') || with_ext.contains("..") {
        return Err(format!("nom d'archive non sûr : '{with_ext}'"));
    }
    Ok(with_ext)
}

/// Vérifie l'intégrité d'une archive d'export (manifeste + empreintes SHA-256 + XML).
#[tauri::command]
pub async fn validate_archive(state: AppStateRef<'_>, dir: String) -> Result<ManifestSummary, String> {
    validate_archive_impl(&state.log, &dir)
}

/// Logique de validation (partagée entre la commande et les tests e2e réels).
fn validate_archive_impl(log: &AppLog, dir: &str) -> Result<ManifestSummary, String> {
    match load_and_verify(Path::new(dir)) {
        Ok((manifest, _)) => {
            log.info(&format!(
                "Archive valide : {} tâche(s), source '{}'",
                manifest.tasks.len(),
                manifest.source_host
            ));
            Ok(ManifestSummary {
                valid: true,
                task_count: manifest.tasks.len(),
                exported_at: Some(manifest.exported_at.clone()),
                source_host: Some(manifest.source_host.clone()),
                error: None,
            })
        }
        Err(e) => {
            log.warn(&format!("Archive invalide ({dir}) : {e}"));
            Ok(ManifestSummary {
                valid: false,
                task_count: 0,
                exported_at: None,
                source_host: None,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Construit le plan d'import d'une archive. Sans décisions, toutes les
/// tâches nécessitant une résolution apparaissent « bloquées ».
#[tauri::command]
pub async fn import_build_plan(
    state: AppStateRef<'_>,
    dir: String,
    decisions: Option<ImportDecisions>,
    target_folder: Option<String>,
) -> Result<Vec<PlanItemView>, String> {
    import_build_plan_impl(&state.log, &state.passwords, &dir, decisions, target_folder.as_deref())
}

/// Logique de construction du plan (partagée entre la commande et les tests
/// e2e réels).
fn import_build_plan_impl(
    log: &AppLog,
    passwords: &Mutex<HashMap<String, String>>,
    dir: &str,
    decisions: Option<ImportDecisions>,
    target_folder: Option<&str>,
) -> Result<Vec<PlanItemView>, String> {
    let (manifest, xmls) = load_and_verify(Path::new(dir)).map_err(|e| e.to_string())?;
    let scheduler = make_scheduler()?;

    let answers = build_answers(&decisions);
    let pw = passwords.lock().unwrap().clone();
    let resolver = PasswordResolver::new(None, Some(pw), false);
    let options = ImportOptions {
        conflict_policy: None,
        skip_password_tasks: false,
        user_map: HashMap::new(),
        answers: Some(&answers),
        target_folder: target_folder.map(str::to_string),
    };

    let plan = build_plan(
        scheduler.as_ref(),
        &manifest,
        &xmls,
        &options,
        &resolver,
        &NullInteractor,
    )
    .map_err(|e| e.to_string())?;

    let views: Vec<PlanItemView> = plan.iter().map(plan_item_view).collect();
    let blocked = views
        .iter()
        .filter(|v| v.action_kind == "conflict" || v.action_kind == "password_required" || v.action_kind == "user_unmapped")
        .count();
    log.info(&format!(
        "Plan d'import établi : {} tâche(s), {} restant(e)(s) à résoudre",
        views.len(),
        blocked
    ));
    Ok(views)
}

/// Mémorise un mot de passe en mémoire (jamais journalisé, jamais renvoyé).
#[tauri::command]
pub async fn import_set_password(
    state: AppStateRef<'_>,
    user: String,
    password: String,
) -> Result<(), String> {
    let mut map = state.passwords.lock().unwrap();
    if password.is_empty() {
        map.remove(&user.to_lowercase());
    } else {
        map.insert(user.to_lowercase(), password);
    }
    drop(map);
    state
        .log
        .info(&format!("Mot de passe mémorisé en mémoire pour '{user}' (jamais journalisé)"));
    Ok(())
}

/// Vide les mots de passe mémorisés (bouton « Effacer les mots de passe »).
#[tauri::command]
pub async fn import_clear_passwords(state: AppStateRef<'_>) -> Result<(), String> {
    state.passwords.lock().unwrap().clear();
    state.log.info("Mots de passe mémorisés effacés");
    Ok(())
}

/// Exécute (ou simule) l'import avec les décisions de l'utilisateur.
///
/// Import réel **sans** privilèges administrateur : au lieu de redémarrer
/// l'interface, l'import est délégué à un processus enfant élevé
/// (`--helper-import`, invite UAC) qui écrit les tâches puis renvoie son
/// rapport. La simulation (dry-run) et l'import déjà élevé restent en
/// processus (lecture seule, aucune élévation nécessaire).
#[tauri::command]
pub async fn import_execute(
    state: AppStateRef<'_>,
    dir: String,
    decisions: ImportDecisions,
    dry_run: bool,
    target_folder: Option<String>,
) -> Result<ReportView, String> {
    import_execute_impl(&state.log, &state.passwords, &dir, decisions, dry_run, true, target_folder.as_deref())
        .await
}

/// Logique d'import (partagée entre la commande et les tests e2e réels).
/// `use_helper` : autorise la délégation au processus enfant élevé pour un
/// import réel non élevé (l'interface passe `true` ; les tests e2e passent
/// `false` pour rester déterministes sur des tâches identiques, sans écriture).
async fn import_execute_impl(
    log: &AppLog,
    passwords: &Mutex<HashMap<String, String>>,
    dir: &str,
    decisions: ImportDecisions,
    dry_run: bool,
    use_helper: bool,
    target_folder: Option<&str>,
) -> Result<ReportView, String> {
    if use_helper && !dry_run && !current_is_elevated() {
        let pw = passwords.lock().unwrap().clone();
        let dir_owned = dir.to_string();
        let folder_owned = target_folder.map(str::to_string);
        log.info("Import : demande d'élévation (UAC) — processus administrateur temporaire");
        let report = tauri::async_runtime::spawn_blocking(move || {
            helper::run_elevated_import(&dir_owned, &decisions, &pw, folder_owned.as_deref())
        })
        .await
        .map_err(|e| format!("tâche d'élévation interrompue : {e}"))??;
        log.info(&format!(
            "Import (admin) terminé : {} créée(s), {} mise(s) à jour, {} ignorée(s), {} bloquée(s), {} échec(s)",
            report.created.len(),
            report.updated.len(),
            report.skipped.len(),
            report.blocked.len(),
            report.failed.len()
        ));
        return Ok(report);
    }

    let (manifest, xmls) = load_and_verify(Path::new(dir)).map_err(|e| e.to_string())?;
    let scheduler = make_scheduler()?;

    let answers = build_answers(&Some(decisions));
    let pw = passwords.lock().unwrap().clone();
    let resolver = PasswordResolver::new(None, Some(pw), false);
    let options = ImportOptions {
        conflict_policy: None,
        skip_password_tasks: false,
        user_map: HashMap::new(),
        answers: Some(&answers),
        target_folder: target_folder.map(str::to_string),
    };

    let plan = build_plan(
        scheduler.as_ref(),
        &manifest,
        &xmls,
        &options,
        &resolver,
        &NullInteractor,
    )
    .map_err(|e| e.to_string())?;

    let verb = if dry_run { "Simulation d'import" } else { "Import" };
    log.info(&format!("{verb} depuis {dir} ({} tâche(s))", plan.len()));

    let report = execute_plan(scheduler.as_ref(), &plan, dry_run).map_err(|e| e.to_string())?;

    for path in &report.created {
        log.info(&format!("Import : créée {path}"));
    }
    for path in &report.updated {
        log.info(&format!("Import : mise à jour {path}"));
    }
    for path in &report.skipped {
        log.info(&format!("Import : ignorée {path}"));
    }
    for (path, reason) in &report.blocked {
        log.warn(&format!("Import : bloquée {path} ({reason})"));
    }
    for (path, reason) in &report.failed {
        log.error(&format!("Import : échec {path} ({reason})"));
    }
    log.info(&format!(
        "{verb} terminé : {} créée(s), {} mise(s) à jour, {} ignorée(s), {} bloquée(s), {} échec(s)",
        report.created.len(),
        report.updated.len(),
        report.skipped.len(),
        report.blocked.len(),
        report.failed.len()
    ));

    Ok(report_view(&report, dry_run))
}

/// Dernières lignes du journal (rafraîchissement incrémental via `after_seq`).
#[tauri::command]
pub async fn get_logs(state: AppStateRef<'_>, after_seq: u64) -> Result<LogsPage, String> {
    let (lines, last_seq) = state.log.lines_after(after_seq);
    Ok(LogsPage { lines, last_seq })
}

/// Dossier où sont écrits les fichiers de logs.
#[tauri::command]
pub async fn log_dir(state: AppStateRef<'_>) -> Result<String, String> {
    Ok(state.log.dir().to_string_lossy().to_string())
}

/// Ouvre le dossier des logs dans l'Explorateur Windows.
#[tauri::command]
pub async fn open_log_folder(state: AppStateRef<'_>) -> Result<(), String> {
    let dir = state.log.dir();
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("impossible d'ouvrir le dossier des logs ({}): {e}", dir.display()))
}

/// L'application tourne-t-elle avec des droits administrateur ?
#[tauri::command]
pub async fn is_elevated() -> bool {
    current_is_elevated()
}

/// Élévation du processus courant (Windows uniquement, sinon faux).
fn current_is_elevated() -> bool {
    #[cfg(windows)]
    {
        tsbak::scheduler::windows_impl::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}


/// Ouvre une boîte de dialogue de sélection de dossier (interface native).
#[tauri::command]
pub async fn pick_folder(title: String) -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new().set_title(&title).pick_folder();
    Ok(picked.map(|p| p.to_string_lossy().to_string()))
}

/// Ouvre une boîte de dialogue de sélection de fichier .zip.
#[tauri::command]
pub async fn pick_zip_file(title: String) -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .set_title(&title)
        .add_filter("Archive d'export tsbak (.zip)", &["zip"])
        .pick_file();
    Ok(picked.map(|p| p.to_string_lossy().to_string()))
}

/// Détection automatique des .zip : extrait l'archive vers un dossier
/// temporaire unique (`%TEMP%\tsbak-extract\...`) après validation complète,
/// puis renvoie le dossier réel à utiliser pour vérifier/planifier/importer.
/// Les anciens dossiers d'extraction (> 7 jours) sont purgés à chaque appel.
#[tauri::command]
pub async fn extract_zip_auto(
    state: AppStateRef<'_>,
    zip_path: String,
    password: Option<String>,
) -> Result<String, String> {
    let base = std::env::temp_dir().join("tsbak-extract");
    purge_old_temp_dirs(&base);

    let name = Path::new(&zip_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "archive".to_string());
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dest = base.join(format!("{safe}-{}-{nanos}", std::process::id()));

    let count = archive::unzip_verified(&state.log, Path::new(&zip_path), &dest, password.as_deref())
        .map_err(|e| {
            state.log.warn(&format!("Extraction automatique du .zip refusée ({zip_path}) : {e}"));
            e
        })?;
    state.log.info(&format!(
        "Archive .zip détectée : extraction automatique de {count} entrée(s) vers {}",
        dest.display()
    ));
    Ok(dest.to_string_lossy().to_string())
}

/// Supprime les dossiers temporaires de travail (`tsbak-extract`,
/// `tsbak-export`) plus vieux de 7 jours : reliquats d'un plantage.
fn purge_old_temp_dirs(base: &std::path::Path) {
    use std::time::{Duration, SystemTime};
    let cutoff = SystemTime::now() - Duration::from_secs(7 * 24 * 3600);
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() && meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }
}

/// Extrait une archive .zip (éventuellement chiffrée) vers `dest_dir` après
/// **validation complète** (manifeste + empreintes SHA-256). La destination
/// n'est créée que si l'archive est saine.
#[tauri::command]
pub async fn extract_zip_archive(
    state: AppStateRef<'_>,
    zip_path: String,
    dest_dir: String,
    password: Option<String>,
) -> Result<usize, String> {
    let log: &AppLog = &state.log;
    archive::unzip_verified(log, Path::new(&zip_path), Path::new(&dest_dir), password.as_deref())
        .map_err(|e| {
            log.warn(&format!("Extraction ZIP refusée ({zip_path}) : {e}"));
            e
        })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convertit les décisions de l'interface en fichier de réponses `AnswerFile`
/// (format déjà supporté par la logique d'import de `tsbak`).
fn build_answers(decisions: &Option<ImportDecisions>) -> AnswerFile {
    let mut answers = AnswerFile::default();
    if let Some(d) = decisions {
        answers.user_map = d.user_map.clone();
        answers.skip_tasks = d.skip_tasks.clone();
        for (path, decision) in &d.conflicts {
            let policy = match decision.as_str() {
                "overwrite" => ConflictDecision::Overwrite,
                _ => ConflictDecision::Skip,
            };
            answers.conflict_decisions.insert(path.clone(), policy);
        }
    }
    answers
}

// ---------------------------------------------------------------------------
// Tests unitaires de l'export en mode archive (MockScheduler, sans COM)
// ---------------------------------------------------------------------------

/// Verifie le decompte de l'export en mode archive : le dossier de
/// destination ne recoit QUE le `.zip` (les XML et le `manifest.json` sont
/// d'abord ecrits dans un dossier temporaire, puis supprimes), l'export
/// simple continue d'ecrire directement dans la destination, et un nom
/// d'archive invalide n'ecrit rien du tout.
#[cfg(test)]
mod export_archive_tests {
    use super::*;
    use crate::app_log::AppLog;
    use std::collections::BTreeSet;
    use tempfile::tempdir;
    use tsbak::model::LogonType;
    use tsbak::scheduler::mock::{MockScheduler, MockTask};

    /// Planificateur simule avec deux taches (aucun acces COM).
    fn sample_scheduler() -> MockScheduler {
        MockScheduler::new(true)
            .with_task(
                "\\Backup\\Nightly",
                MockTask {
                    xml: "<Task>nightly</Task>".to_string(),
                    user_id: Some("SYSTEM".to_string()),
                    logon_type: LogonType::ServiceAccount,
                },
            )
            .with_task(
                "\\Autre\\Nettoyage",
                MockTask {
                    xml: "<Task>nettoyage</Task>".to_string(),
                    user_id: None,
                    logon_type: LogonType::None,
                },
            )
    }

    /// Liste triee des entrees (fichiers/dossiers) d'un repertoire.
    fn entry_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("lecture de {} : {e}", dir.display()))
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Ensemble des dossiers de staging presents dans `%TEMP%\tsbak-export`
    /// (vide si ce dossier n'existe pas encore).
    fn staging_names() -> BTreeSet<String> {
        std::fs::read_dir(std::env::temp_dir().join("tsbak-export"))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Mode archive : la destination ne contient que le `.zip`, qui embarque
    /// les deux XML et le manifeste ; le dossier temporaire est supprime.
    #[test]
    fn export_archive_ne_depose_que_le_zip() {
        let dest = tempdir().expect("dossier de destination");
        let logs = tempdir().expect("dossier de logs");
        let log = AppLog::with_dir(logs.path().join("logs"));
        let scheduler = sample_scheduler();
        let staging_before = staging_names();

        let summary = export_with(
            &scheduler,
            &log,
            dest.path().to_str().expect("chemin utf-8"),
            Vec::new(),
            Vec::new(),
            Some("sauvegarde.zip".to_string()),
            None,
        )
        .expect("export en archive");

        // La destination ne recoit QUE le .zip, jamais les XML ni le manifeste.
        assert_eq!(
            entry_names(dest.path()),
            vec!["sauvegarde.zip".to_string()],
            "la destination ne doit recevoir que le .zip"
        );
        let zip_full = dest.path().join("sauvegarde.zip");
        assert!(zip_full.is_file(), "le .zip doit exister dans la destination");
        assert_eq!(summary.zip_path.as_deref(), zip_full.to_str());
        assert!(!summary.zip_encrypted, "aucun mot de passe = pas de chiffrement");
        assert_eq!(summary.exported.len(), 2);
        assert_eq!(summary.skipped_by_filter, 0);

        // Le dossier temporaire de staging a bien ete supprime.
        let after = staging_names();
        let leftovers: Vec<String> = after.difference(&staging_before).cloned().collect();
        assert!(
            leftovers.is_empty(),
            "dossier temporaire d'export non supprime : {leftovers:?}"
        );

        // L'archive embarque bien les 2 XML + manifest.json (extraction
        // validee par les empreintes SHA-256 du manifeste).
        let extract_dir = logs.path().join("extrait");
        let count = archive::unzip_verified(
            &log,
            &zip_full,
            &extract_dir,
            None,
        )
        .expect("extraction de l'archive");
        assert_eq!(count, 3, "2 XML + manifest.json attendus dans le .zip");
    }

    /// Mode simple (sans archive) : XML + manifest.json ecrits directement
    /// dans la destination, aucun .zip.
    #[test]
    fn export_sans_archive_ecrit_dans_la_destination() {
        let dest = tempdir().expect("dossier de destination");
        let logs = tempdir().expect("dossier de logs");
        let log = AppLog::with_dir(logs.path().join("logs"));
        let scheduler = sample_scheduler();

        let summary = export_with(
            &scheduler,
            &log,
            dest.path().to_str().expect("chemin utf-8"),
            Vec::new(),
            Vec::new(),
            None,
            None,
        )
        .expect("export simple");

        let names = entry_names(dest.path());
        assert!(names.contains(&"manifest.json".to_string()), "manifest.json absent : {names:?}");
        assert_eq!(names.iter().filter(|n| n.ends_with(".xml")).count(), 2, "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with(".zip")), "aucun .zip attendu : {names:?}");
        assert!(summary.zip_path.is_none());
        assert!(!summary.zip_encrypted);
        assert_eq!(summary.exported.len(), 2);
    }

    /// Un nom d'archive invalide (« .. » interdit) echoue avant toute
    /// ecriture : ni .zip, ni fichier d'export, ni dossier temporaire.
    #[test]
    fn nom_archive_invalide_ne_cree_rien() {
        let dest = tempdir().expect("dossier de destination");
        let logs = tempdir().expect("dossier de logs");
        let log = AppLog::with_dir(logs.path().join("logs"));
        let scheduler = sample_scheduler();
        let staging_before = staging_names();

        let err = export_with(
            &scheduler,
            &log,
            dest.path().to_str().expect("chemin utf-8"),
            Vec::new(),
            Vec::new(),
            Some("a...b".to_string()),
            None,
        )
        .expect_err("le nom d'archive doit etre refuse");

        assert!(
            err.contains("invalide") || err.contains("non s"),
            "erreur inattendue : {err}"
        );
        assert!(
            entry_names(dest.path()).is_empty(),
            "aucun fichier ne doit etre ecrit en cas de nom d'archive invalide"
        );
        assert_eq!(
            staging_names(),
            staging_before,
            "aucun dossier temporaire ne doit etre cree"
        );
    }
}

// ---------------------------------------------------------------------------
// Test réel de bout en bout (scheduler Windows + COM + ZIP AES)
// ---------------------------------------------------------------------------

/// Test de bout en bout sur la vraie machine : export de quelques tâches
/// réelles vers un `.zip` chiffré AES-256 (mêmes fonctions que l'interface),
/// extraction avec un mauvais puis le bon mot de passe, validation,
/// construction du plan, simulation et import réel (tâches identiques → rien
/// n'est écrit, aucune élévation requise), puis vérification du rapport et
/// des logs (étapes présentes, mot de passe absent partout).
///
/// Ignoré par défaut : nécessite une machine Windows avec des tâches réelles.
/// Lancement : `cargo test --test e2e -- --ignored --nocapture` (unitaire) ou
/// `cargo test e2e_export_zip_aes_then_reimport -- --ignored --nocapture`.
#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::app_log::AppLog;
    use std::sync::Mutex;

    const ZIP_PASSWORD: &str = "e2e-secret-mot-de-passe";

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    #[ignore = "test réel : scheduler Windows + COM, à lancer explicitement"]
    fn e2e_export_zip_aes_then_reimport() {
        // 1. Préparer un état applicatif isolé (journal dans un dossier temporaire).
        let tmp = tempfile::tempdir().expect("dossier temporaire");
        let log = AppLog::with_dir(tmp.path().join("logs"));
        let passwords = Mutex::new(HashMap::<String, String>::new());

        // 2. Sélectionner quelques tâches réelles (racine, hors Microsoft).
        let scheduler = make_scheduler().expect("scheduler Windows");
        let tasks: Vec<String> = scheduler
            .list_tasks(false)
            .expect("liste des tâches")
            .into_iter()
            .map(|h| h.path)
            .filter(|p| !p.starts_with("\\Microsoft"))
            .take(3)
            .collect();
        assert!(!tasks.is_empty(), "aucune tâche réelle disponible pour le test e2e");
        eprintln!("== Tâches exportées : {tasks:?}");

        // 3. Export réel vers un .zip chiffré AES-256 (même code que l'interface).
        let export_dir = tmp.path().join("export");
        let summary = export_tasks_impl(
            &log,
            export_dir.to_str().unwrap(),
            tasks.clone(),
            vec![],
            Some("e2e.zip".to_string()),
            Some(ZIP_PASSWORD.to_string()),
        )
        .expect("export réel");
        assert_eq!(summary.exported.len(), tasks.len(), "toutes les tâches sélectionnées exportées");
        assert!(summary.zip_encrypted, "l'archive doit être chiffrée");
        let zip_path = summary.zip_path.expect("chemin du .zip");
        eprintln!("== .zip AES-256 : {zip_path}");
        assert!(Path::new(&zip_path).exists());

        // 4. Le chiffrement est réel : mauvais mot de passe refusé, aucun dossier créé.
        let bad_dest = tmp.path().join("bad");
        let bad = archive::unzip_verified(&log, Path::new(&zip_path), &bad_dest, Some("mauvais-mot-de-passe"));
        assert!(bad.is_err(), "le mauvais mot de passe doit être refusé");
        assert!(!bad_dest.exists(), "aucune extraction avec un mauvais mot de passe");

        // 5. Bon mot de passe : extraction + validation complète (empreintes SHA-256).
        let good_dest = tmp.path().join("good");
        let count = archive::unzip_verified(&log, Path::new(&zip_path), &good_dest, Some(ZIP_PASSWORD))
            .expect("extraction avec le bon mot de passe");
        eprintln!("== Archive extraite : {count} entrée(s) vers {}", good_dest.display());
        let validated = validate_archive_impl(&log, good_dest.to_str().unwrap()).expect("validation");
        assert!(validated.valid, "manifeste + empreintes valides");
        assert_eq!(validated.task_count, tasks.len());

        // 6. Plan : réimport sur la même machine → toutes les tâches sont identiques.
        let plan = import_build_plan_impl(&log, &passwords, good_dest.to_str().unwrap(), None, None)
            .expect("construction du plan");
        assert_eq!(plan.len(), tasks.len());
        assert!(
            plan.iter().all(|v| v.action_kind == "skip_identical"),
            "réimport même machine : tout doit être classé identique, obtenu : {:?}",
            plan.iter().map(|v| (v.path.as_str(), v.action_kind.as_str())).collect::<Vec<_>>()
        );

        // 7. Simulation puis import réel (en processus : tâches identiques → aucune
        //    écriture, donc aucun besoin d'élévation).
        let decisions = ImportDecisions::default();
        let dry = block_on(import_execute_impl(&log, &passwords, good_dest.to_str().unwrap(), decisions.clone(), true, false, None))
            .expect("simulation");
        assert_eq!(dry.exit_code, 0);
        assert_eq!(dry.skipped.len(), tasks.len(), "simulation : tout ignoré (identique)");

        let real = block_on(import_execute_impl(&log, &passwords, good_dest.to_str().unwrap(), decisions, false, false, None))
            .expect("import réel");
        assert_eq!(real.exit_code, 0, "aucune écriture nécessaire : code 0");
        assert!(real.failed.is_empty(), "aucun échec : {:?}", real.failed);
        eprintln!("== Rapport import : créées={} à-jour={} ignorées={} bloquées={} échecs={}",
            real.created.len(), real.updated.len(), real.skipped.len(), real.blocked.len(), real.failed.len());

        // 8. Logs : toutes les étapes sont journalisées, le mot de passe jamais.
        let (lines, _) = log.lines_after(0);
        let ring = lines.iter().map(|l| l.message.as_str()).collect::<Vec<_>>().join("\n");
        eprintln!("== Journal ({} ligne(s)) :\n{ring}\n==", ring.lines().count());
        for expected in ["Export terminé", "Archive valide", "Plan d'import établi", "Simulation d'import terminé", "Import terminé"] {
            assert!(ring.contains(expected), "log attendu : {expected}\n-- obtenu :\n{ring}");
        }
        assert!(!ring.contains(ZIP_PASSWORD), "le mot de passe du .zip ne doit jamais être journalisé (anneau)");

        let log_files: Vec<_> = std::fs::read_dir(tmp.path().join("logs"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        let file_content = log_files
            .iter()
            .map(|f| std::fs::read_to_string(f).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!file_content.contains(ZIP_PASSWORD), "le mot de passe du .zip ne doit jamais être journalisé (fichier)");
        eprintln!("== Logs vérifiés : {} ligne(s), {} fichier(s), secret absent", ring.lines().count(), log_files.len());
    }
}
//! Task backup and restore : interface graphique d'export/import des tâches
//! planifiées Windows, avec journalisation fichier et vue des logs intégrée.

mod app_log;
mod archive;
mod commands;
/// Import en processus enfant élevé (mode `--helper-import`) : voir `helper.rs`.
pub mod helper;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use app_log::AppLog;

/// État partagé de l'application, géré par Tauri et passé aux commandes.
///
/// Les mots de passe (import) sont stockés ici, en mémoire uniquement :
/// jamais écrits sur disque, jamais journalisés, jamais renvoyés à
/// l'interface.
pub struct AppState {
    /// Journal fichier + anneau mémoire.
    pub log: AppLog,
    /// Mots de passe saisis (clé = utilisateur en minuscules).
    pub passwords: Mutex<HashMap<String, String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let log = AppLog::init();
            log.info("Task backup and restore démarré");
            app.manage(AppState {
                log,
                passwords: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::export_tasks,
            commands::validate_archive,
            commands::import_build_plan,
            commands::extract_zip_archive,
            commands::pick_zip_file,
            commands::import_set_password,
            commands::import_clear_passwords,
            commands::import_execute,
            commands::get_logs,
            commands::log_dir,
            commands::open_log_folder,
            commands::is_elevated,
            commands::extract_zip_auto,
            commands::pick_folder,
        ])
        .run(tauri::generate_context!())
        .expect("erreur lors du lancement de Task backup and restore");
}
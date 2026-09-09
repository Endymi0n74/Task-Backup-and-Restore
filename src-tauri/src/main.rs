// Empêche la création d'une fenêtre console en release (app graphique pure).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Mode helper : import en processus enfant élevé, sans interface.
    // Détecté avant le démarrage de Tauri pour rester un processus léger.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == task_backup_restore::helper::HELPER_FLAG) {
        std::process::exit(task_backup_restore::helper::run_helper_args(&args));
    }
    task_backup_restore::run();
}
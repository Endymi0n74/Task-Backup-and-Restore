//! Arrêt déterministe du processus à la fermeture de la fenêtre.
//!
//!(bug corrigé en 1.1.1) Sous Windows Server 2016 / 2019, la fermeture de
//! la fenêtre pouvait laisser `TaskBackupRestore.exe` **vivant** dans le
//! gestionnaire des tâches : la fin de session passait alors par la
//! déconnexion des objets WebView2 puis par la boucle d'événements, deux
//! étapes susceptibles de bloquer (runtime WebView2 anciennement figé sur
//! ces serveurs). L'application est mono-fenêtre : la fermeture de la
//! fenêtre **est** la fin de la session — on journalise donc et on quitte
//! le processus explicitement, avant toute déconnexion d'objets.
//!
//! À la fermeture, les fichiers de travail de l'import élevé sont aussi
//! purgés (`helper::purge_work_files`) : le fichier de réponses, qui
//! contient brièvement les mots de passe sur disque, ne doit jamais
//! survivre à l'interface (cas d'une fermeture pendant l'invite UAC).

use tauri::Manager;

use crate::helper;
use crate::AppState;

/// Purge les fichiers de travail temporaires, journalise la fermeture puis
/// **quitte le processus**. Ne retourne jamais.
///
/// Appelé depuis `on_window_event` sur `CloseRequested`, c'est-à-dire
/// **avant** toute déconnexion de la fenêtre ou du WebView2 : quoi que le
/// runtime ait prévu de faire ensuite, le processus est déjà sorti.
pub fn exit_on_window_close(app: &tauri::AppHandle) {
    // 1. Aucun fichier de réponses/résultat ne doit rester sur disque.
    //    Un helper élevé qui aurait déjà chargé ses réponses n'est pas
    //    concerné (il les a supprimées en les lisant) ; une élévation encore
    //    en attente d'UAC échouera proprement à la lecture, sans secret
    //    abandonné.
    helper::purge_work_files();

    // 2. Dernière ligne de journal (fichier non tamponné : écrite
    //    immédiatement sur disque, aucun flush à faire).
    if let Some(state) = app.try_state::<AppState>() {
        state.log.info("Fenêtre fermée — arrêt de l'application");
    }

    // 3. Sortie déterministe : aucune déconnexion WebView2/COM/runtime ne
    //    doit pouvoir maintenir le processus en vie. Les processus enfants
    //    WebView2 détectent la fin de l'hôte et s'arrêtent d'eux-mêmes.
    std::process::exit(0);
}

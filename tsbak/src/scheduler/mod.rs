//! Abstraction de l'API du planificateur de taches Windows.
//!
//! Toute la logique metier (export.rs, import.rs) passe par ce trait, jamais
//! directement par les appels COM : cela permet de tester la classification
//! et les flux d'import/export sans machine Windows, via `MockScheduler`.

use crate::error::Result;
use crate::model::LogonType;

#[cfg(windows)]
pub mod windows_impl;

// Toujours compile (petit struct en memoire) afin que les tests d'integration
// du crate (dossier tests/) puissent l'utiliser sans feature dediee.
pub mod mock;

/// Une tache telle que vue par l'enumeration du planificateur.
#[derive(Debug, Clone)]
pub struct TaskHandle {
    /// Chemin complet, ex: "\\MonDossier\\MaTache".
    pub path: String,
}

/// Metadonnees d'authentification d'une tache, extraites de sa definition.
#[derive(Debug, Clone)]
pub struct TaskAuthInfo {
    /// Identifiant utilisateur associe (principal RunAs), si applicable.
    pub user_id: Option<String>,
    /// Type d'authentification de la tache.
    pub logon_type: LogonType,
}

/// Interface metier vers le planificateur de taches (local ou distant).
///
/// Les implementations doivent garantir que `get_task_xml` renvoie le XML
/// **exactement** tel que stocke par le planificateur (aucune reecriture),
/// conformement a l'exigence "export brut, jamais modifie".
pub trait TaskSchedulerApi {
    /// Enumere les taches. Si `recursive` est faux, seul le dossier racine est parcouru.
    fn list_tasks(&self, recursive: bool) -> Result<Vec<TaskHandle>>;

    /// Recupere le XML brut d'une tache existante.
    fn get_task_xml(&self, path: &str) -> Result<String>;

    /// Recupere les informations d'authentification (utilisateur, type de logon) d'une tache.
    fn get_task_auth_info(&self, path: &str) -> Result<TaskAuthInfo>;

    /// Indique si une tache existe deja a ce chemin sur la cible.
    fn task_exists(&self, path: &str) -> Result<bool>;

    /// Cree recursivement les dossiers manquants jusqu'a `folder_path`.
    fn ensure_folder(&self, folder_path: &str) -> Result<()>;

    /// Enregistre (cree ou met a jour) une tache a partir de son XML.
    /// `password` est passe uniquement en memoire, jamais journalise.
    fn register_task(
        &self,
        path: &str,
        xml: &str,
        user_id: Option<&str>,
        password: Option<&str>,
        logon_type: LogonType,
    ) -> Result<()>;

    /// Indique si l'appelant dispose des droits suffisants pour ecrire (admin).
    fn has_write_privileges(&self) -> bool;
}

/// Extrait le chemin de dossier parent d'un chemin de tache complet.
/// "\\A\\B\\Task" -> "\\A\\B", "\\Task" -> "\\".
pub fn parent_folder(task_path: &str) -> String {
    match task_path.rfind('\\') {
        Some(0) => "\\".to_string(),
        Some(idx) => task_path[..idx].to_string(),
        None => "\\".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_folder_root() {
        assert_eq!(parent_folder("\\Task"), "\\");
    }

    #[test]
    fn parent_folder_nested() {
        assert_eq!(parent_folder("\\A\\B\\Task"), "\\A\\B");
    }
}

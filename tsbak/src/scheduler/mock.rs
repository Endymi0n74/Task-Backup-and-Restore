//! Implementation en memoire de `TaskSchedulerApi`, utilisee par les tests
//! unitaires et d'integration pour valider la logique d'export/import sans
//! machine Windows ni acces COM reel.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use crate::error::{Result, TsbakError};
use crate::model::LogonType;
use crate::scheduler::{TaskAuthInfo, TaskHandle, TaskSchedulerApi};

/// Definition d'une tache simulee (XML, utilisateur, type d'authentification).
#[derive(Debug, Clone)]
pub struct MockTask {
    /// XML brut de la tache.
    pub xml: String,
    /// Identifiant utilisateur associe, si applicable.
    pub user_id: Option<String>,
    /// Type d'authentification de la tache.
    pub logon_type: LogonType,
}

/// Planificateur simule : les taches et dossiers existent en memoire.
/// `register_task` enregistre les appels effectues pour permettre aux tests
/// de verifier qu'aucune ecriture n'a eu lieu en mode dry-run.
pub struct MockScheduler {
    /// Taches presentes, indexees par chemin complet.
    pub tasks: RefCell<HashMap<String, MockTask>>,
    /// Dossiers existants (y compris la racine "\\").
    pub folders: RefCell<BTreeSet<String>>,
    /// Simule la presence (ou non) des droits d'ecriture administrateur.
    pub write_privileges: bool,
    /// Chemins des taches effectivement enregistrees via `register_task`.
    pub registrations: RefCell<Vec<String>>,
    /// Si renseigne, `register_task` echoue avec ce HRESULT pour ce chemin de tache.
    pub fail_on: RefCell<HashMap<String, u32>>,
}

impl MockScheduler {
    /// Cree un planificateur simule vide, avec ou sans droits d'ecriture.
    pub fn new(write_privileges: bool) -> Self {
        let mut folders = BTreeSet::new();
        folders.insert("\\".to_string());
        MockScheduler {
            tasks: RefCell::new(HashMap::new()),
            folders: RefCell::new(folders),
            write_privileges,
            registrations: RefCell::new(Vec::new()),
            fail_on: RefCell::new(HashMap::new()),
        }
    }

    /// Ajoute une tache preexistante (methode chainable, utile dans les tests).
    pub fn with_task(self, path: &str, task: MockTask) -> Self {
        self.tasks.borrow_mut().insert(path.to_string(), task);
        self
    }

    /// Nombre d'appels a `register_task` ayant reussi.
    pub fn registration_count(&self) -> usize {
        self.registrations.borrow().len()
    }
}

impl TaskSchedulerApi for MockScheduler {
    fn list_tasks(&self, _recursive: bool) -> Result<Vec<TaskHandle>> {
        Ok(self
            .tasks
            .borrow()
            .keys()
            .map(|p| TaskHandle { path: p.clone() })
            .collect())
    }

    fn get_task_xml(&self, path: &str) -> Result<String> {
        self.tasks
            .borrow()
            .get(path)
            .map(|t| t.xml.clone())
            .ok_or_else(|| TsbakError::Other(format!("tache introuvable: {path}")))
    }

    fn get_task_auth_info(&self, path: &str) -> Result<TaskAuthInfo> {
        self.tasks
            .borrow()
            .get(path)
            .map(|t| TaskAuthInfo {
                user_id: t.user_id.clone(),
                logon_type: t.logon_type,
            })
            .ok_or_else(|| TsbakError::Other(format!("tache introuvable: {path}")))
    }

    fn task_exists(&self, path: &str) -> Result<bool> {
        Ok(self.tasks.borrow().contains_key(path))
    }

    fn ensure_folder(&self, folder_path: &str) -> Result<()> {
        let mut current = String::new();
        for part in folder_path.split('\\').filter(|s| !s.is_empty()) {
            current.push('\\');
            current.push_str(part);
            self.folders.borrow_mut().insert(current.clone());
        }
        self.folders.borrow_mut().insert("\\".to_string());
        Ok(())
    }

    fn register_task(
        &self,
        path: &str,
        xml: &str,
        user_id: Option<&str>,
        _password: Option<&str>,
        logon_type: LogonType,
    ) -> Result<()> {
        if !self.write_privileges {
            return Err(TsbakError::AccessDenied {
                operation: format!("RegisterTask({path})"),
            });
        }
        if let Some(hresult) = self.fail_on.borrow().get(path).copied() {
            return Err(crate::error::translate_hresult("RegisterTask", hresult));
        }
        self.registrations.borrow_mut().push(path.to_string());
        self.tasks.borrow_mut().insert(
            path.to_string(),
            MockTask {
                xml: xml.to_string(),
                user_id: user_id.map(|s| s.to_string()),
                logon_type,
            },
        );
        Ok(())
    }

    fn has_write_privileges(&self) -> bool {
        self.write_privileges
    }
}

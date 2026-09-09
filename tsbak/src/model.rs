//! Structures de donnees partagees : manifeste d'export, description d'une
//! tache planifiee, et classification des actions d'import.

use serde::{Deserialize, Serialize};

/// Type d'authentification associe a une tache (reflet de TASK_LOGON_TYPE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogonType {
    /// Aucune information d'identification necessaire (ex: tache SYSTEM).
    None,
    /// Mot de passe stocke necessaire pour s'executer (TASK_LOGON_PASSWORD).
    Password,
    /// Le mot de passe est demande uniquement si necessaire.
    InteractiveTokenOrPassword,
    /// Jeton interactif uniquement, pas de secret stocke.
    InteractiveToken,
    /// Group Managed Service Account / groupe : pas de secret.
    Group,
    /// Service account (LocalService, NetworkService, LocalSystem) : pas de secret.
    ServiceAccount,
    /// S4U (Service for User) : pas de mot de passe stocke.
    S4U,
}

impl LogonType {
    /// Est-ce que ce type d'authentification necessite un mot de passe pour
    /// pouvoir enregistrer la tache avec RegisterTask ?
    pub fn requires_password(self) -> bool {
        matches!(self, LogonType::Password)
    }
}

/// Une entree du manifeste, decrivant une tache exportee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// Chemin complet dans le planificateur, ex: "\\MonDossier\\MaTache".
    pub path: String,
    /// Nom du fichier XML relatif au dossier d'export (sans le mot de passe, jamais modifie).
    pub xml_file: String,
    /// Empreinte SHA-256 du fichier XML exporte, pour detecter toute alteration.
    pub sha256: String,
    /// Identifiant utilisateur associe (principal RunAs), si applicable.
    pub user_id: Option<String>,
    /// Type d'authentification de la tache.
    pub logon_type: LogonType,
}

impl TaskRecord {
    /// Est-ce que cette tache necessite un mot de passe pour etre reimportee ?
    pub fn requires_password(&self) -> bool {
        self.logon_type.requires_password()
    }
}

/// Manifeste JSON accompagnant l'archive d'export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Version du format de manifeste (voir [`Manifest::CURRENT_VERSION`]).
    pub version: u32,
    /// Date/heure de l'export, au format RFC 3339.
    pub exported_at: String,
    /// Nom de la machine source, a titre indicatif.
    pub source_host: String,
    /// Liste des taches exportees.
    pub tasks: Vec<TaskRecord>,
}

impl Manifest {
    /// Version courante du format de manifeste produite par cette version de tsbak.
    pub const CURRENT_VERSION: u32 = 1;

    /// Construit un nouveau manifeste, horodate a l'instant present.
    pub fn new(source_host: String, tasks: Vec<TaskRecord>) -> Self {
        Manifest {
            version: Self::CURRENT_VERSION,
            exported_at: chrono::Local::now().to_rfc3339(),
            source_host,
            tasks,
        }
    }
}

/// Decision de classification pour une tache donnee au moment de l'import.
/// Cette classification doit etre identique en dry-run et en execution reelle :
/// c'est elle qui pilote a la fois le rapport et l'ecriture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportAction {
    /// La tache n'existe pas sur la cible : creation simple.
    Create,
    /// La tache existe et differe (XML different) : mise a jour autorisee.
    Update,
    /// La tache existe et est strictement identique : rien a faire.
    SkipIdentical,
    /// La tache existe, differe, et aucune politique de conflit n'a permis
    /// de decider (bloquant hors resolution interactive/answer-file).
    Conflict,
    /// La tache necessite un mot de passe qui n'a pas ete fourni.
    PasswordRequired {
        /// Utilisateur pour lequel le mot de passe est requis.
        user: String,
    },
    /// L'utilisateur source n'a pas de correspondance sur la machine cible.
    UserUnmapped {
        /// Identifiant de l'utilisateur source, tel qu'exporte.
        source_user: String,
    },
    /// Choix explicite de l'utilisateur (interactif ou answer-file) de sauter cette tache.
    SkippedByChoice,
}

impl ImportAction {
    /// Une action bloquante empeche toute ecriture pour cette tache.
    pub fn is_blocking(&self) -> bool {
        matches!(
            self,
            ImportAction::Conflict
                | ImportAction::PasswordRequired { .. }
                | ImportAction::UserUnmapped { .. }
        )
    }

    /// Une action qui entraine effectivement une ecriture (RegisterTask).
    pub fn is_writing(&self) -> bool {
        matches!(self, ImportAction::Create | ImportAction::Update)
    }

    /// Etiquette lisible affichee dans le plan d'import (ex: "CREATE").
    pub fn label(&self) -> &'static str {
        match self {
            ImportAction::Create => "CREATE",
            ImportAction::Update => "UPDATE",
            ImportAction::SkipIdentical => "SKIP (identique)",
            ImportAction::Conflict => "CONFLIT",
            ImportAction::PasswordRequired { .. } => "MOT DE PASSE REQUIS",
            ImportAction::UserUnmapped { .. } => "UTILISATEUR NON MAPPE",
            ImportAction::SkippedByChoice => "SKIP (choix utilisateur)",
        }
    }
}

/// Politique de resolution automatique des conflits (tache deja existante et differente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ConflictPolicy {
    /// Ecrase la tache existante par la version importee.
    Overwrite,
    /// Conserve la tache existante, ignore la version importee.
    Skip,
}

/// Resultat de l'execution (ou de la simulation) d'un plan d'import.
#[derive(Debug, Default)]
pub struct ExecutionReport {
    /// Chemins des taches creees.
    pub created: Vec<String>,
    /// Chemins des taches mises a jour.
    pub updated: Vec<String>,
    /// Chemins des taches ignorees (identiques ou choix explicite).
    pub skipped: Vec<String>,
    /// Taches bloquees, avec la raison (chemin, raison).
    pub blocked: Vec<(String, String)>,
    /// Taches dont l'ecriture a echoue, avec le message d'erreur (chemin, erreur).
    pub failed: Vec<(String, String)>,
}

impl ExecutionReport {
    /// Code de sortie tel que specifie : 0 succes complet, 1 partiel/bloque, 2 echec complet/fatal.
    /// Une tache "bloquee" (conflit, mot de passe requis, utilisateur non
    /// mappe) releve toujours du code 1 : elle a ete correctement identifiee,
    /// aucune ecriture n'a echoue. Le code 2 est reserve a un echec total des
    /// tentatives d'ecriture (toutes les taches traitees ont echoue) ou a une
    /// erreur fatale remontee avant meme de produire un rapport.
    pub fn exit_code(&self) -> i32 {
        let total = self.created.len()
            + self.updated.len()
            + self.skipped.len()
            + self.blocked.len()
            + self.failed.len();
        if total == 0 {
            return 0;
        }
        if self.blocked.is_empty() && self.failed.is_empty() {
            return 0;
        }
        if !self.failed.is_empty() && self.failed.len() == total {
            return 2;
        }
        1
    }
}

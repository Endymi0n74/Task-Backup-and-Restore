//! Erreurs de tsbak et traduction des codes HRESULT Windows en messages clairs.

use thiserror::Error;

/// Toutes les erreurs que peut renvoyer tsbak, qu'elles proviennent du
/// systeme de fichiers, du manifeste, de la resolution des secrets ou du
/// planificateur de taches lui-meme.
#[derive(Debug, Error)]
pub enum TsbakError {
    /// Erreur d'entree/sortie sur un fichier ou dossier donne.
    #[error("Erreur d'entree/sortie sur {path}: {source}")]
    Io {
        /// Chemin du fichier ou dossier concerne.
        path: String,
        /// Erreur d'E/S d'origine.
        #[source]
        source: std::io::Error,
    },

    /// Le XML d'une tache n'a pas pu etre analyse (fichier corrompu ou mal forme).
    #[error("XML invalide pour la tache '{task}': {reason}")]
    InvalidXml {
        /// Chemin complet de la tache concernee.
        task: String,
        /// Raison de l'invalidite.
        reason: String,
    },

    /// Le fichier manifest.json est absent, mal forme ou d'une version non supportee.
    #[error("Manifeste invalide ou corrompu: {0}")]
    InvalidManifest(String),

    /// L'empreinte SHA-256 du XML sur disque ne correspond pas a celle du manifeste.
    #[error("Empreinte SHA-256 incorrecte pour la tache '{task}' (fichier modifie ou corrompu)")]
    HashMismatch {
        /// Chemin complet de la tache concernee.
        task: String,
    },

    /// Les droits administrateur sont necessaires pour l'operation demandee.
    #[error("Acces refuse: droits administrateur requis pour '{operation}'")]
    AccessDenied {
        /// Description de l'operation qui a echoue.
        operation: String,
    },

    /// Un mot de passe est necessaire pour cette tache mais aucune source
    /// (fichier, fichier de reponses, invite interactive) n'a permis de le fournir.
    #[error("Mot de passe requis pour la tache '{task}' (utilisateur '{user}') mais aucun n'a ete fourni")]
    PasswordRequired {
        /// Chemin complet de la tache concernee.
        task: String,
        /// Utilisateur pour lequel le mot de passe est requis.
        user: String,
    },

    /// L'utilisateur source de la tache n'a pas de correspondance sur la machine cible.
    #[error("Utilisateur source '{source_user}' non mappe vers un utilisateur cible pour la tache '{task}'")]
    UserNotMapped {
        /// Chemin complet de la tache concernee.
        task: String,
        /// Identifiant de l'utilisateur source, tel qu'exporte.
        source_user: String,
    },

    /// La tache existe deja sur la cible et differe, sans qu'aucune
    /// politique ou reponse n'ait permis de decider quoi faire.
    #[error("Conflit non resolu pour la tache '{task}' (existe deja et differe de la version a importer)")]
    UnresolvedConflict {
        /// Chemin complet de la tache concernee.
        task: String,
    },

    /// Une reponse est necessaire mais aucun terminal interactif n'est
    /// disponible et aucun fichier de reponses ne couvre le cas.
    #[error("Entree utilisateur requise mais aucun terminal interactif disponible et aucun fichier de reponses fourni (taches bloquees: {blocked:?})")]
    NonInteractiveNoAnswer {
        /// Chemins des taches restees bloquees.
        blocked: Vec<String>,
    },

    /// Erreur remontee par le planificateur de taches Windows (HRESULT COM),
    /// deja traduite en message clair via [`translate_hresult`].
    #[error("Erreur du planificateur de taches Windows ({hresult:#010x}) lors de '{operation}': {message}")]
    Scheduler {
        /// Operation COM qui a echoue (ex: "RegisterTask(\\A\\B)").
        operation: String,
        /// Code HRESULT brut renvoye par l'API COM.
        hresult: u32,
        /// Message d'erreur traduit en langage clair.
        message: String,
    },

    /// La fonctionnalite demandee necessite une plateforme non disponible ici (typiquement Windows).
    #[error("Fonctionnalite non disponible sur cette plateforme: {0}")]
    PlatformUnsupported(String),

    /// Erreur generique, pour les cas qui ne correspondent a aucune autre variante.
    #[error("{0}")]
    Other(String),
}

/// Alias de [`std::result::Result`] specialise pour les erreurs de tsbak.
pub type Result<T> = std::result::Result<T, TsbakError>;

/// Traduit un HRESULT COM du Task Scheduler en message d'erreur comprehensible.
/// Les codes couverts sont ceux le plus frequemment rencontres avec
/// ITaskFolder::RegisterTask / RegisterTaskDefinition.
pub fn translate_hresult(operation: &str, hresult: u32) -> TsbakError {
    let message = match hresult {
        0x80070005 => {
            "Acces refuse. Relancez tsbak depuis une invite de commandes administrateur.".to_string()
        }
        0x80070534 => "Aucun mappage entre le nom de compte et l'ID de securite n'a ete effectue (l'utilisateur cible n'existe pas ou n'est pas accessible)."
            .to_string(),
        0x8007052E => "Nom d'utilisateur ou mot de passe incorrect pour le compte d'execution de la tache."
            .to_string(),
        0x80041318 => "Le mot de passe fourni ne correspond pas au compte specifie (ERROR_BAD_USERNAME cote planificateur).".to_string(),
        0x80070003 => "Chemin de dossier introuvable dans le planificateur de taches.".to_string(),
        0x80070002 => "Fichier ou ressource introuvable.".to_string(),
        0xC00E0002 => "Le XML de la tache n'a pas pu etre analyse (fichier corrompu ou mal forme)."
            .to_string(),
        0x8004131D => "Le service du planificateur de taches n'est pas disponible ou n'a pas pu etre contacte.".to_string(),
        0x8004131F => "Une instance de cette tache est deja en cours d'execution.".to_string(),
        0x800704EC => "Ce programme est bloque par une strategie de groupe.".to_string(),
        other => format!("Code HRESULT non repertorie: {other:#010x}."),
    };
    TsbakError::Scheduler {
        operation: operation.to_string(),
        hresult,
        message,
    }
}

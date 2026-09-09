//! Fichier de reponses (`--answer-file`) : permet un import 100% non
//! interactif en pre-repondant a toutes les questions que poserait le
//! wizard (mapping utilisateurs, mots de passe, decisions de conflit,
//! taches a sauter).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TsbakError};
use crate::model::ConflictPolicy;

/// Contenu structure d'un fichier de reponses, permettant un import
/// entierement automatise sans wizard interactif.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnswerFile {
    /// Mapping utilisateur source -> utilisateur cible, ex: "OLDPC\\bob": "NEWPC\\bob".
    #[serde(default)]
    pub user_map: HashMap<String, String>,
    /// Mots de passe par utilisateur CIBLE (apres mapping). Jamais ecrit nulle part.
    #[serde(default)]
    pub passwords: HashMap<String, String>,
    /// Decision de conflit par chemin de tache complet.
    #[serde(default)]
    pub conflict_decisions: HashMap<String, ConflictDecision>,
    /// Taches a sauter explicitement (chemin complet), quelle que soit leur classification.
    #[serde(default)]
    pub skip_tasks: Vec<String>,
}

/// Decision de resolution d'un conflit pour une tache donnee, telle que
/// specifiee dans un fichier de reponses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictDecision {
    /// Ecrase la tache existante par la version importee.
    Overwrite,
    /// Conserve la tache existante, ignore la version importee.
    Skip,
}

impl From<ConflictDecision> for ConflictPolicy {
    fn from(d: ConflictDecision) -> Self {
        match d {
            ConflictDecision::Overwrite => ConflictPolicy::Overwrite,
            ConflictDecision::Skip => ConflictPolicy::Skip,
        }
    }
}

impl AnswerFile {
    /// Charge et parse un fichier de reponses JSON depuis le disque.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| TsbakError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_json::from_str(&content)
            .map_err(|e| TsbakError::Other(format!("fichier de reponses invalide: {e}")))
    }
}

/// Parse les options repetees `--user-map "src:dest"` en table de correspondance.
pub fn parse_user_map(entries: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for entry in entries {
        let (src, dest) = entry.split_once(':').ok_or_else(|| {
            TsbakError::Other(format!(
                "mapping utilisateur invalide '{entry}', format attendu 'src:dest'"
            ))
        })?;
        map.insert(src.trim().to_string(), dest.trim().to_string());
    }
    Ok(map)
}

/// Comptes bien connus qui ne necessitent jamais de mapping utilisateur
/// (ils existent identiquement sur toute machine Windows).
const WELL_KNOWN_ACCOUNTS: &[&str] = &[
    "system",
    "nt authority\\system",
    "local service",
    "nt authority\\local service",
    "network service",
    "nt authority\\network service",
];

/// Indique si `user_id` est un compte systeme bien connu (SYSTEM, LOCAL
/// SERVICE, NETWORK SERVICE...), ne necessitant jamais de mapping utilisateur.
pub fn is_well_known_account(user_id: &str) -> bool {
    WELL_KNOWN_ACCOUNTS.contains(&user_id.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_map_entries() {
        let entries = vec!["OLDPC\\bob:NEWPC\\bob".to_string(), "a:b".to_string()];
        let map = parse_user_map(&entries).unwrap();
        assert_eq!(map.get("OLDPC\\bob"), Some(&"NEWPC\\bob".to_string()));
        assert_eq!(map.get("a"), Some(&"b".to_string()));
    }

    #[test]
    fn rejects_malformed_entry() {
        let entries = vec!["nocolon".to_string()];
        assert!(parse_user_map(&entries).is_err());
    }

    #[test]
    fn well_known_accounts_recognised() {
        assert!(is_well_known_account("SYSTEM"));
        assert!(is_well_known_account("NT AUTHORITY\\SYSTEM"));
        assert!(!is_well_known_account("DOMAIN\\alice"));
    }
}

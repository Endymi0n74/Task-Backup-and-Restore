//! Resolution des mots de passe necessaires a l'import des taches en
//! TASK_LOGON_PASSWORD. Aucun mot de passe n'est jamais journalise: le
//! type `Debug` n'est volontairement pas derive sur les structures qui en
//! contiennent, et les logs/rapports ne manipulent que des noms d'utilisateur.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::IsTerminal;

use crate::error::{Result, TsbakError};

/// Charge un fichier "user=password" (une entree par ligne, lignes vides et
/// commentaires '#' ignores). Les cles sont normalisees en minuscules.
pub fn load_password_file(path: &std::path::Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).map_err(|e| TsbakError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((user, pass)) = line.split_once('=') {
            map.insert(user.trim().to_lowercase(), pass.to_string());
        }
    }
    Ok(map)
}

/// Fournit les mots de passe necessaires a l'enregistrement des taches.
/// L'ordre de resolution est : fichier de mots de passe / fichier de
/// reponses (statique, non-interactif) puis, si un terminal est disponible
/// et qu'aucune source statique n'a repondu, une invite masquee.
pub struct PasswordResolver {
    static_sources: HashMap<String, String>,
    allow_interactive: bool,
    cache: RefCell<HashMap<String, String>>,
}

impl PasswordResolver {
    /// Construit un resolveur a partir des sources statiques disponibles
    /// (fichier de mots de passe, fichier de reponses) et indique si une
    /// invite interactive est autorisee en dernier recours.
    pub fn new(
        password_file: Option<HashMap<String, String>>,
        answer_file_passwords: Option<HashMap<String, String>>,
        allow_interactive: bool,
    ) -> Self {
        let mut static_sources = HashMap::new();
        if let Some(m) = password_file {
            for (k, v) in m {
                static_sources.insert(k.to_lowercase(), v);
            }
        }
        if let Some(m) = answer_file_passwords {
            for (k, v) in m {
                static_sources.insert(k.to_lowercase(), v);
            }
        }
        PasswordResolver {
            static_sources,
            allow_interactive,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Indique si l'entree standard est un terminal interactif.
    pub fn is_tty() -> bool {
        std::io::stdin().is_terminal()
    }

    /// Cherche un mot de passe deja connu (statique ou deja saisi pour cet
    /// utilisateur), sans jamais provoquer de prompt.
    pub fn lookup_static(&self, user: &str) -> Option<String> {
        let key = user.to_lowercase();
        if let Some(p) = self.cache.borrow().get(&key) {
            return Some(p.clone());
        }
        self.static_sources.get(&key).cloned()
    }

    /// Demande interactivement le mot de passe pour `user` (saisie masquee),
    /// pour le compte de la tache `task_path` (affiche uniquement pour le
    /// contexte, jamais le secret). Met le resultat en cache pour les
    /// prochaines taches partageant le meme utilisateur.
    pub fn prompt_interactive(&self, user: &str, task_path: &str) -> Result<Option<String>> {
        if !self.allow_interactive {
            return Ok(None);
        }
        let prompt = format!("Mot de passe pour '{user}' (requis par la tache '{task_path}'), Entree pour ignorer cette tache: ");
        let pw = rpassword::prompt_password(prompt)
            .map_err(|e| TsbakError::Other(format!("lecture du mot de passe: {e}")))?;
        if pw.is_empty() {
            return Ok(None);
        }
        self.cache
            .borrow_mut()
            .insert(user.to_lowercase(), pw.clone());
        Ok(Some(pw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_lookup_is_case_insensitive() {
        let mut file_map = HashMap::new();
        file_map.insert("domain\\alice".to_string(), "s3cret".to_string());
        let resolver = PasswordResolver::new(Some(file_map), None, false);
        assert_eq!(
            resolver.lookup_static("DOMAIN\\Alice"),
            Some("s3cret".to_string())
        );
        assert_eq!(resolver.lookup_static("DOMAIN\\bob"), None);
    }

    #[test]
    fn answer_file_overrides_nothing_missing_in_password_file() {
        let mut pf = HashMap::new();
        pf.insert("a".to_string(), "1".to_string());
        let mut af = HashMap::new();
        af.insert("b".to_string(), "2".to_string());
        let resolver = PasswordResolver::new(Some(pf), Some(af), false);
        assert_eq!(resolver.lookup_static("a"), Some("1".to_string()));
        assert_eq!(resolver.lookup_static("b"), Some("2".to_string()));
    }

    #[test]
    fn non_interactive_never_prompts() {
        let resolver = PasswordResolver::new(None, None, false);
        assert_eq!(resolver.prompt_interactive("x", "\\y").unwrap(), None);
    }
}

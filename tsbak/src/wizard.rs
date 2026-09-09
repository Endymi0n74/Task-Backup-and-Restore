//! Interactions humaines necessaires a l'import : mapping utilisateur et
//! decision de conflit. Abstrait derriere `Interactor` pour rester testable
//! et pour permettre un mode strictement non-interactif (`NullInteractor`).

use std::io::{self, Write};

use crate::model::ConflictPolicy;

/// Abstraction des questions posees a l'utilisateur pendant l'import.
pub trait Interactor {
    /// Demande comment resoudre un conflit (tache existante et differente).
    /// `None` signifie qu'aucune decision n'a pu etre prise (mode bloquant).
    fn ask_conflict(&self, task_path: &str) -> Option<ConflictPolicy>;

    /// Demande vers quel utilisateur cible mapper `source_user` pour la
    /// tache `task_path`. `None` si aucune reponse n'est disponible.
    fn ask_user_mapping(&self, source_user: &str, task_path: &str) -> Option<String>;
}

/// Ne repond jamais : utilise en mode non-interactif (pas de TTY, pas de
/// fichier de reponses couvrant le cas).
pub struct NullInteractor;

impl Interactor for NullInteractor {
    fn ask_conflict(&self, _task_path: &str) -> Option<ConflictPolicy> {
        None
    }
    fn ask_user_mapping(&self, _source_user: &str, _task_path: &str) -> Option<String> {
        None
    }
}

/// Pose les questions sur le terminal courant (stdin/stdout).
pub struct TtyInteractor;

impl TtyInteractor {
    fn ask_line(prompt: &str) -> Option<String> {
        print!("{prompt}");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        if io::stdin().read_line(&mut buf).is_err() {
            return None;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

impl Interactor for TtyInteractor {
    fn ask_conflict(&self, task_path: &str) -> Option<ConflictPolicy> {
        loop {
            let answer = Self::ask_line(&format!(
                "Conflit sur '{task_path}' (deja presente et differente). \
                 [o]verwrite / [s]kip / [?] revoir le detail : "
            ))?;
            match answer.to_lowercase().as_str() {
                "o" | "overwrite" => return Some(ConflictPolicy::Overwrite),
                "s" | "skip" => return Some(ConflictPolicy::Skip),
                _ => println!("Reponse non reconnue, tapez 'o' ou 's'."),
            }
        }
    }

    fn ask_user_mapping(&self, source_user: &str, task_path: &str) -> Option<String> {
        Self::ask_line(&format!(
            "Utilisateur '{source_user}' (tache '{task_path}') introuvable sur cette machine. \
             Utilisateur de remplacement (Entree pour bloquer cette tache) : "
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_interactor_never_answers() {
        let i = NullInteractor;
        assert_eq!(i.ask_conflict("\\x"), None);
        assert_eq!(i.ask_user_mapping("u", "\\x"), None);
    }
}

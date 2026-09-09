//! Definition de l'interface en ligne de commande (clap).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use tsbak::model::ConflictPolicy;

#[derive(Parser, Debug)]
#[command(
    name = "tsbak",
    version,
    about = "Export/import des taches planifiees Windows (Task Scheduler)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Liste les taches planifiees.
    List {
        /// Parcourt aussi les sous-dossiers.
        #[arg(long)]
        recursive: bool,

        /// Masque les taches systeme \Microsoft\ (comme l'interface).
        #[arg(long)]
        hide_microsoft: bool,
    },

    /// Exporte les taches planifiees vers un dossier (XML brut + manifest.json).
    Export {
        /// Dossier de destination de l'archive.
        dir: PathBuf,

        /// Motif d'inclusion (glob simple avec '*'), repetable.
        #[arg(long = "include")]
        include: Vec<String>,

        /// Motif d'exclusion (glob simple avec '*'), repetable.
        #[arg(long = "exclude")]
        exclude: Vec<String>,

        /// Masque les taches systeme \Microsoft\ (comme l'interface).
        #[arg(long)]
        hide_microsoft: bool,
    },

    /// Reimporte les taches depuis un dossier d'export.
    Import {
        /// Dossier contenant manifest.json et les XML exportes.
        dir: PathBuf,

        /// Simule l'import sans ecrire quoi que ce soit.
        #[arg(long)]
        dry_run: bool,

        /// Force le wizard interactif meme si un fichier de reponses est fourni.
        #[arg(long)]
        interactive: bool,

        /// Repond "oui" par defaut aux questions non bloquantes.
        #[arg(long)]
        yes: bool,

        /// Ignore (sans bloquer) les taches necessitant un mot de passe non fourni.
        #[arg(long)]
        skip_password_tasks: bool,

        /// Fichier "utilisateur=mot_de_passe" (un par ligne).
        #[arg(long)]
        password_file: Option<PathBuf>,

        /// Fichier de reponses JSON pour un import 100% non interactif.
        #[arg(long)]
        answer_file: Option<PathBuf>,

        /// Mapping utilisateur "source:destination", repetable.
        #[arg(long = "user-map")]
        user_map: Vec<String>,

        /// Politique par defaut en cas de conflit (tache deja presente et differente).
        #[arg(long, value_enum)]
        conflict_policy: Option<ConflictPolicy>,

        /// Dossier cible du planificateur (ex: \\Restore-2026-01-01) : restaure
        /// chaque tache sous ce dossier en preservant sa structure d'origine.
        #[arg(long)]
        folder: Option<String>,
    },

    /// Valide l'integrite d'une archive d'export (manifeste + empreintes + XML bien forme).
    Validate { dir: PathBuf },
}

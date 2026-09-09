mod cli;

use std::io::IsTerminal;

use clap::Parser;

use tsbak::answers::{parse_user_map, AnswerFile};
use tsbak::error::{Result, TsbakError};
use tsbak::export::{export, is_microsoft_task, PatternFilter};
use tsbak::import::{build_plan, execute_plan, load_and_verify, ImportOptions};
use tsbak::model::ExecutionReport;
use tsbak::password::{load_password_file, PasswordResolver};
use tsbak::scheduler::TaskSchedulerApi;
use tsbak::wizard::{Interactor, NullInteractor, TtyInteractor};

use cli::{Cli, Command};

#[cfg(windows)]
fn make_scheduler() -> Result<Box<dyn TaskSchedulerApi>> {
    use tsbak::scheduler::windows_impl::WindowsScheduler;
    Ok(Box::new(WindowsScheduler::connect()?))
}

#[cfg(not(windows))]
fn make_scheduler() -> Result<Box<dyn TaskSchedulerApi>> {
    Err(TsbakError::PlatformUnsupported(
        "l'acces au Task Scheduler Windows necessite d'executer tsbak sur Windows".to_string(),
    ))
}

fn local_host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn print_report(report: &ExecutionReport, dry_run: bool) {
    let verb = if dry_run { "seraient" } else { "ont ete" };
    println!();
    println!("=== Rapport d'import{} ===", if dry_run { " (dry-run)" } else { "" });
    if !report.created.is_empty() {
        println!("Crees ({}) : {} {}", report.created.len(), report.created.join(", "), verb);
    }
    if !report.updated.is_empty() {
        println!("Mis a jour ({}) : {}", report.updated.len(), report.updated.join(", "));
    }
    if !report.skipped.is_empty() {
        println!("Ignores ({}) : {}", report.skipped.len(), report.skipped.join(", "));
    }
    if !report.blocked.is_empty() {
        println!("Bloques ({}) :", report.blocked.len());
        for (path, reason) in &report.blocked {
            println!("  - {path}: {reason}");
        }
    }
    if !report.failed.is_empty() {
        println!("Echecs ({}) :", report.failed.len());
        for (path, reason) in &report.failed {
            println!("  - {path}: {reason}");
        }
    }
    if report.created.is_empty()
        && report.updated.is_empty()
        && report.skipped.is_empty()
        && report.blocked.is_empty()
        && report.failed.is_empty()
    {
        println!("Aucune tache a traiter.");
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    tsbak::log::init_logging();
    let cmd_name = match &cli.command {
        Command::List { .. } => "list",
        Command::Export { .. } => "export",
        Command::Import { .. } => "import",
        Command::Validate { .. } => "validate",
    };
    tsbak::log::info(format!(
        "tsbak {} : commande {} sur {}",
        env!("CARGO_PKG_VERSION"),
        cmd_name,
        local_host_name()
    ));
    match cli.command {
        Command::List { recursive, hide_microsoft } => {
            let scheduler = make_scheduler()?;
            let tasks = scheduler.list_tasks(recursive)?;
            let mut shown = 0usize;
            for t in &tasks {
                if hide_microsoft && is_microsoft_task(&t.path) {
                    continue;
                }
                println!("{}", t.path);
                shown += 1;
            }
            if hide_microsoft {
                let hidden = tasks.len() - shown;
                println!("{} tache(s) affichee(s) ({} Microsoft masquee(s)).", shown, hidden);
            } else {
                println!("{} tache(s).", tasks.len());
            }
            Ok(0)
        }

        Command::Export {
            dir,
            include,
            exclude,
            hide_microsoft,
        } => {
            let scheduler = make_scheduler()?;
            // Les taches systeme sont exclues de la meme facon que dans
            // l'interface (dossier \Microsoft\ entierement ignore).
            let mut exclude = exclude;
            if hide_microsoft {
                exclude.push("\\Microsoft\\*".to_string());
            }
            let filter = PatternFilter::new(include, exclude);
            tsbak::log::info(format!("Export vers {}", dir.display()));
            let summary = export(scheduler.as_ref(), &dir, true, &filter, &local_host_name())?;
            println!(
                "{} tache(s) exportee(s) vers {}",
                summary.exported.len(),
                dir.display()
            );
            if !summary.skipped_by_filter.is_empty() {
                println!(
                    "{} tache(s) ignoree(s) par les filtres --include/--exclude",
                    summary.skipped_by_filter.len()
                );
            }
            tsbak::log::info(format!(
                "Export termine : {} tache(s), {} ignoree(s) par filtre",
                summary.exported.len(),
                summary.skipped_by_filter.len()
            ));
            Ok(0)
        }

        Command::Import {
            dir,
            dry_run,
            interactive,
            yes,
            skip_password_tasks,
            password_file,
            answer_file,
            user_map,
            conflict_policy,
            folder,
        } => {
            let is_tty = std::io::stdin().is_terminal();
            if interactive && !is_tty {
                return Err(TsbakError::Other(
                    "--interactive necessite un terminal interactif (aucun detecte)".to_string(),
                ));
            }
            let allow_interactive = (is_tty || interactive) && !yes;

            let (manifest, xmls) = load_and_verify(&dir)?;

            let answers = match &answer_file {
                Some(p) => Some(AnswerFile::load(p)?),
                None => None,
            };

            let password_map = match &password_file {
                Some(p) => Some(load_password_file(p)?),
                None => None,
            };
            let answer_passwords = answers.as_ref().map(|a| a.passwords.clone());
            let resolver = PasswordResolver::new(password_map, answer_passwords, allow_interactive);

            let mut cli_user_map = parse_user_map(&user_map)?;
            if let Some(a) = &answers {
                for (k, v) in &a.user_map {
                    cli_user_map.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }

            let options = ImportOptions {
                conflict_policy,
                skip_password_tasks,
                user_map: cli_user_map,
                answers: answers.as_ref(),
                target_folder: folder,
            };

            let interactor: Box<dyn Interactor> = if allow_interactive {
                Box::new(TtyInteractor)
            } else {
                Box::new(NullInteractor)
            };

            let scheduler = make_scheduler()?;
            let plan = build_plan(
                scheduler.as_ref(),
                &manifest,
                &xmls,
                &options,
                &resolver,
                interactor.as_ref(),
            )?;

            println!("=== Plan d'import ({} tache(s)) ===", plan.len());
            for item in &plan {
                println!("{:<24} {}", item.action.label(), item.target_path);
            }
            tsbak::log::info(format!("Plan d'import : {} tache(s)", plan.len()));

            let has_blocking = plan.iter().any(|p| p.action.is_blocking());
            if has_blocking && !is_tty && answers.is_none() {
                eprintln!();
                eprintln!(
                    "Execution non interactive sans fichier de reponses : certaines taches restent bloquees (voir ci-dessus)."
                );
            }

            let report = execute_plan(scheduler.as_ref(), &plan, dry_run)?;
            print_report(&report, dry_run);
            tsbak::log::info(format!(
                "Import{} : {} creee(s), {} mise(s) a jour, {} ignoree(s), {} bloquee(s), {} echec(s)",
                if dry_run { " (dry-run)" } else { "" },
                report.created.len(),
                report.updated.len(),
                report.skipped.len(),
                report.blocked.len(),
                report.failed.len()
            ));
            Ok(report.exit_code())
        }

        Command::Validate { dir } => match load_and_verify(&dir) {
            Ok((manifest, _)) => {
                println!(
                    "Archive valide : {} tache(s), exportee(s) le {} depuis '{}'.",
                    manifest.tasks.len(),
                    manifest.exported_at,
                    manifest.source_host
                );
                tsbak::log::info(format!(
                    "Archive valide : {} tache(s), source '{}'",
                    manifest.tasks.len(),
                    manifest.source_host
                ));
                Ok(0)
            }
            Err(e) => {
                eprintln!("Archive invalide : {e}");
                tsbak::log::error(format!("Archive invalide : {e}"));
                Ok(2)
            }
        },
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Erreur: {e}");
            tsbak::log::error(format!("{e}"));
            std::process::exit(2);
        }
    }
}

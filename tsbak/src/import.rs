//! Import des taches planifiees : validation du manifeste, classification
//! (creer / mettre a jour / sauter / conflit / mot de passe requis /
//! utilisateur non mappe), puis execution (ou simulation en dry-run).
//!
//! La classification est calculee une seule fois et pilote a la fois le
//! rapport dry-run et l'ecriture reelle : les deux flux partagent donc
//! exactement la meme logique de decision (exigence "dry-run identique au
//! vrai flux").

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::answers::{is_well_known_account, AnswerFile};
use crate::error::{Result, TsbakError};
use crate::hash::sha256_hex;
use crate::model::{ConflictPolicy, ExecutionReport, ImportAction, LogonType, Manifest, TaskRecord};
use crate::password::PasswordResolver;
use crate::scheduler::TaskSchedulerApi;
use crate::wizard::Interactor;

/// Une ligne de plan enrichie des informations necessaires a l'ecriture
/// (utilisateur cible resolu, mot de passe si applicable). Le mot de passe
/// n'est jamais affiche : cette structure ne derive pas `Debug`.
pub struct PlanItem {
    /// Enregistrement d'origine (chemin, hash, utilisateur/logon type declares a l'export).
    pub record: TaskRecord,
    /// Chemin effectif de la tache sur la cible : `record.path` eventuellement
    /// prefixe par le dossier cible (`--folder` / `target_folder`). C'est ce
    /// chemin qui est utilise pour l'existence, la comparaison et l'ecriture.
    pub target_path: String,
    /// XML brut a enregistrer (identique a celui exporte).
    pub xml: String,
    /// Action retenue pour cette tache.
    pub action: ImportAction,
    /// Utilisateur cible resolu (apres mapping eventuel), si applicable.
    pub target_user: Option<String>,
    password: Option<String>,
}

impl PlanItem {
    /// Acces au mot de passe resolu pour cette tache, si applicable. Ne
    /// jamais journaliser cette valeur.
    pub fn take_password(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

/// Options gouvernant la construction du plan d'import.
pub struct ImportOptions<'a> {
    /// Politique par defaut en cas de conflit (tache existante et differente).
    pub conflict_policy: Option<ConflictPolicy>,
    /// Ignore (sans bloquer) les taches necessitant un mot de passe non fourni.
    pub skip_password_tasks: bool,
    /// Mapping utilisateur source -> cible, fourni via `--user-map`.
    pub user_map: HashMap<String, String>,
    /// Fichier de reponses eventuellement charge, pour un import non interactif.
    pub answers: Option<&'a AnswerFile>,
    /// Dossier cible du planificateur (ex: `\\Restore-2026-01-01`) : si
    /// renseigne, chaque tache est restauree sous ce dossier en **preservant
    /// sa structure d'origine** (`\\A\\B\\Tache` -> `\\Cible\\A\\B\\Tache`).
    /// Cela evite les collisions de noms et conserve le groupement source.
    pub target_folder: Option<String>,
}

/// Calcule le chemin effectif d'une tache sur la cible.
///
/// Sans dossier cible, le chemin est inchange. Avec un dossier cible, le
/// chemin d'origine (qui commence toujours par `\\`) est prefixe :
/// `\\A\\B\\Tache` + `\\Cible` -> `\\Cible\\A\\B\\Tache`.
///
/// La structure d'origine est preservee volontairement : deux taches de meme
/// nom dans des dossiers differents ne peuvent pas entrer en collision, et le
/// groupement source est conserve. Un dossier vide ou `None` laisse le chemin
/// inchange.
pub fn remap_path(path: &str, target_folder: Option<&str>) -> String {
    let Some(folder) = target_folder.map(str::trim).filter(|f| !f.is_empty()) else {
        return path.to_string();
    };
    let folder = folder.trim_matches('\\');
    if folder.is_empty() {
        return path.to_string();
    }
    format!("\\{folder}{path}")
}

/// Charge le manifeste d'un dossier d'export et verifie que chaque XML
/// correspond bien a l'empreinte enregistree lors de l'export.
pub fn load_and_verify(dir: &Path) -> Result<(Manifest, HashMap<String, String>)> {
    let manifest_path = dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path).map_err(|e| TsbakError::Io {
        path: manifest_path.display().to_string(),
        source: e,
    })?;
    let manifest: Manifest = serde_json::from_str(&raw)
        .map_err(|e| TsbakError::InvalidManifest(format!("{e}")))?;
    if manifest.version != Manifest::CURRENT_VERSION {
        return Err(TsbakError::InvalidManifest(format!(
            "version de manifeste non supportee: {}",
            manifest.version
        )));
    }

    let mut xmls = HashMap::new();
    for record in &manifest.tasks {
        let xml_path = dir.join(&record.xml_file);
        let xml = fs::read_to_string(&xml_path).map_err(|e| TsbakError::Io {
            path: xml_path.display().to_string(),
            source: e,
        })?;
        let actual_hash = sha256_hex(xml.as_bytes());
        if actual_hash != record.sha256 {
            return Err(TsbakError::HashMismatch {
                task: record.path.clone(),
            });
        }
        // Le XML doit rester analysable (bien forme) : on verifie ici sans
        // jamais le reecrire, conformement a l'exigence d'export brut.
        if quick_xml::Reader::from_str(&xml)
            .read_event()
            .is_err()
        {
            return Err(TsbakError::InvalidXml {
                task: record.path.clone(),
                reason: "XML mal forme".to_string(),
            });
        }
        xmls.insert(record.path.clone(), xml);
    }
    Ok((manifest, xmls))
}

/// Determine si une resolution explicite de l'utilisateur cible est
/// necessaire avant ecriture. Les taches TASK_LOGON_PASSWORD n'ont pas
/// besoin de cette verification prealable : leur compte est valide (ou pas)
/// au moment de RegisterTask, et l'echec HRESULT correspondant est deja
/// traduit en message clair (voir error::translate_hresult). Seuls les
/// logon types sans secret stocke (S4U, jeton interactif) n'ont aucune
/// autre occasion de detecter un compte absent : on le verifie donc ici.
fn needs_mapping_check(logon_type: LogonType) -> bool {
    matches!(
        logon_type,
        LogonType::InteractiveToken | LogonType::InteractiveTokenOrPassword | LogonType::S4U
    )
}

/// Construit le plan d'import complet. C'est la meme fonction qui est
/// utilisee en dry-run et en execution reelle : seule l'etape d'ecriture
/// (`execute_plan`) differe.
#[allow(clippy::too_many_arguments)]
pub fn build_plan(
    scheduler: &dyn TaskSchedulerApi,
    manifest: &Manifest,
    xmls: &HashMap<String, String>,
    options: &ImportOptions,
    resolver: &PasswordResolver,
    interactor: &dyn Interactor,
) -> Result<Vec<PlanItem>> {
    let empty_conflicts = HashMap::new();
    let empty_skips: Vec<String> = Vec::new();
    let (answer_user_map, answer_passwords_users, answer_conflicts, answer_skips): (
        HashMap<String, String>,
        Vec<String>,
        &HashMap<String, crate::answers::ConflictDecision>,
        &Vec<String>,
    ) = match options.answers {
        Some(a) => (
            a.user_map.clone(),
            a.passwords.keys().cloned().collect(),
            &a.conflict_decisions,
            &a.skip_tasks,
        ),
        None => (HashMap::new(), Vec::new(), &empty_conflicts, &empty_skips),
    };
    let _ = answer_passwords_users; // le contenu des mots de passe est deja fusionne dans `resolver`

    let mut plan = Vec::new();

    for record in &manifest.tasks {
        let xml = xmls
            .get(&record.path)
            .ok_or_else(|| TsbakError::Other(format!("XML manquant pour '{}'", record.path)))?
            .clone();
        let target_path = remap_path(&record.path, options.target_folder.as_deref());

        macro_rules! push_and_continue {
            ($action:expr) => {{
                plan.push(PlanItem {
                    record: record.clone(),
                    target_path: target_path.clone(),
                    xml,
                    action: $action,
                    target_user: None,
                    password: None,
                });
                continue;
            }};
        }

        // 1. Choix explicite de saut (answer-file), prioritaire sur tout le reste.
        if answer_skips.iter().any(|p| p == &record.path) {
            push_and_continue!(ImportAction::SkippedByChoice);
        }

        // 2. Existence / conflit : determine s'il y a quoi que ce soit a
        // ecrire. Les verifications portent sur le chemin **cible** (les
        // decisions de l'utilisateur, elles, restent clees par le chemin
        // d'origine de l'archive). On ne demande mapping ou mot de passe que
        // si une ecriture est effectivement necessaire (une tache identique ou
        // explicitement sautee ne doit jamais declencher de question a
        // l'utilisateur).
        let intended_write = if !scheduler.task_exists(&target_path)? {
            Some(ImportAction::Create)
        } else {
            let existing_xml = scheduler.get_task_xml(&target_path)?;
            if existing_xml == xml {
                push_and_continue!(ImportAction::SkipIdentical);
            }
            let resolved = if let Some(decision) = answer_conflicts.get(&record.path) {
                Some(ConflictPolicy::from(*decision))
            } else if let Some(policy) = options.conflict_policy {
                Some(policy)
            } else {
                interactor.ask_conflict(&target_path)
            };
            match resolved {
                Some(ConflictPolicy::Overwrite) => Some(ImportAction::Update),
                Some(ConflictPolicy::Skip) => push_and_continue!(ImportAction::SkippedByChoice),
                None => push_and_continue!(ImportAction::Conflict),
            }
        };

        // 3. Resolution de l'utilisateur cible, uniquement pour les logon
        // types qui n'ont aucune autre occasion de detecter un compte absent.
        let mut target_user = record.user_id.clone();
        if let Some(source_user) = record.user_id.clone() {
            if needs_mapping_check(record.logon_type) && !is_well_known_account(&source_user) {
                if let Some(mapped) = options
                    .user_map
                    .get(&source_user)
                    .or_else(|| answer_user_map.get(&source_user))
                {
                    target_user = Some(mapped.clone());
                } else if let Some(mapped) = interactor.ask_user_mapping(&source_user, &target_path) {
                    target_user = Some(mapped);
                } else {
                    push_and_continue!(ImportAction::UserUnmapped { source_user });
                }
            }
        }

        // 4. Resolution du mot de passe si le logon type en necessite un.
        let mut password = None;
        if record.requires_password() {
            let user_for_pw = target_user.clone().unwrap_or_default();
            password = resolver
                .lookup_static(&user_for_pw)
                .or(resolver.prompt_interactive(&user_for_pw, &record.path)?);
            if password.is_none() {
                if options.skip_password_tasks {
                    push_and_continue!(ImportAction::SkippedByChoice);
                }
                push_and_continue!(ImportAction::PasswordRequired { user: user_for_pw });
            }
        }

        plan.push(PlanItem {
            record: record.clone(),
            target_path,
            xml,
            action: intended_write.expect("resolu ci-dessus"),
            target_user,
            password,
        });
    }

    Ok(plan)
}

/// Execute le plan. En dry-run, aucune ecriture n'est effectuee : le rapport
/// produit reste identique (memes classifications), seule l'ecriture reelle
/// est court-circuitee.
pub fn execute_plan(
    scheduler: &dyn TaskSchedulerApi,
    plan: &[PlanItem],
    dry_run: bool,
) -> Result<ExecutionReport> {
    let mut report = ExecutionReport::default();

    if !dry_run && plan.iter().any(|p| p.action.is_writing()) && !scheduler.has_write_privileges()
    {
        return Err(TsbakError::AccessDenied {
            operation: "import (RegisterTask)".to_string(),
        });
    }

    for item in plan {
        match &item.action {
            ImportAction::Create | ImportAction::Update => {
                if dry_run {
                    match item.action {
                        ImportAction::Create => report.created.push(item.target_path.clone()),
                        ImportAction::Update => report.updated.push(item.target_path.clone()),
                        _ => unreachable!(),
                    }
                    continue;
                }
                let folder = crate::scheduler::parent_folder(&item.target_path);
                if let Err(e) = scheduler.ensure_folder(&folder) {
                    report.failed.push((item.target_path.clone(), e.to_string()));
                    continue;
                }
                let result = scheduler.register_task(
                    &item.target_path,
                    &item.xml,
                    item.target_user.as_deref(),
                    item.take_password(),
                    item.record.logon_type,
                );
                match result {
                    Ok(()) => match item.action {
                        ImportAction::Create => report.created.push(item.target_path.clone()),
                        ImportAction::Update => report.updated.push(item.target_path.clone()),
                        _ => unreachable!(),
                    },
                    Err(e) => report.failed.push((item.target_path.clone(), e.to_string())),
                }
            }
            ImportAction::SkipIdentical | ImportAction::SkippedByChoice => {
                report.skipped.push(item.target_path.clone());
            }
            ImportAction::Conflict => {
                report
                    .blocked
                    .push((item.target_path.clone(), "conflit non resolu".to_string()));
            }
            ImportAction::PasswordRequired { user } => {
                report.blocked.push((
                    item.target_path.clone(),
                    format!("mot de passe requis pour '{user}'"),
                ));
            }
            ImportAction::UserUnmapped { source_user } => {
                report.blocked.push((
                    item.target_path.clone(),
                    format!("utilisateur '{source_user}' non mappe"),
                ));
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LogonType, TaskRecord};
    use crate::scheduler::mock::{MockScheduler, MockTask};
    use crate::wizard::NullInteractor;

    fn record(path: &str, logon: LogonType, user: Option<&str>, xml: &str) -> (TaskRecord, String) {
        (
            TaskRecord {
                path: path.to_string(),
                xml_file: format!("{}.xml", path.replace('\\', "__")),
                sha256: sha256_hex(xml.as_bytes()),
                user_id: user.map(|s| s.to_string()),
                logon_type: logon,
            },
            xml.to_string(),
        )
    }

    fn manifest_and_xmls(entries: Vec<(TaskRecord, String)>) -> (Manifest, HashMap<String, String>) {
        let mut xmls = HashMap::new();
        let mut records = Vec::new();
        for (r, xml) in entries {
            xmls.insert(r.path.clone(), xml);
            records.push(r);
        }
        (Manifest::new("TESTHOST".to_string(), records), xmls)
    }

    #[test]
    fn creates_new_task_without_secret() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].action, ImportAction::Create);

        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\A\\NoSecret".to_string()]);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(scheduler.registration_count(), 1);
    }

    #[test]
    fn dry_run_never_writes() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        let report = execute_plan(&scheduler, &plan, true).unwrap();
        assert_eq!(report.created, vec!["\\A\\NoSecret".to_string()]);
        assert_eq!(scheduler.registration_count(), 0, "dry-run ne doit rien ecrire");
    }

    #[test]
    fn identical_existing_task_is_skipped() {
        let xml = "<Task>same</Task>";
        let (r, xml_owned) = record("\\A\\Same", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Same",
            MockTask {
                xml: xml.to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\A\\Same".to_string()]);
        assert_eq!(scheduler.registration_count(), 0);
    }

    #[test]
    fn differing_existing_task_without_policy_blocks() {
        let (r, xml) = record("\\A\\Diff", LogonType::None, None, "<Task>new</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Diff",
            MockTask {
                xml: "<Task>old</Task>".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Conflict);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.blocked.len(), 1);
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn conflict_policy_overwrite_updates() {
        let (r, xml) = record("\\A\\Diff", LogonType::None, None, "<Task>new</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\A\\Diff",
            MockTask {
                xml: "<Task>old</Task>".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: Some(ConflictPolicy::Overwrite),
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Update);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.updated, vec!["\\A\\Diff".to_string()]);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn password_required_blocks_without_source() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        match &plan[0].action {
            ImportAction::PasswordRequired { user } => assert_eq!(user, "DOMAIN\\alice"),
            other => panic!("expected PasswordRequired, got {other:?}"),
        }
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn skip_password_tasks_flag_skips_instead_of_blocking() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: true,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkippedByChoice);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\A\\WithPw".to_string()]);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn password_file_supplies_secret_and_task_is_created() {
        let (r, xml) = record(
            "\\A\\WithPw",
            LogonType::Password,
            Some("DOMAIN\\alice"),
            "<Task>pw</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let mut pf = HashMap::new();
        pf.insert("domain\\alice".to_string(), "s3cret".to_string());
        let resolver = PasswordResolver::new(Some(pf), None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\A\\WithPw".to_string()]);
    }

    #[test]
    fn unmapped_user_blocks_without_user_map() {
        let (r, xml) = record(
            "\\A\\S4u",
            LogonType::S4U,
            Some("OLDPC\\bob"),
            "<Task>s4u</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        match &plan[0].action {
            ImportAction::UserUnmapped { source_user } => assert_eq!(source_user, "OLDPC\\bob"),
            other => panic!("expected UserUnmapped, got {other:?}"),
        }
    }

    #[test]
    fn user_map_resolves_and_creates() {
        let (r, xml) = record(
            "\\A\\S4u",
            LogonType::S4U,
            Some("OLDPC\\bob"),
            "<Task>s4u</Task>",
        );
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let mut user_map = HashMap::new();
        user_map.insert("OLDPC\\bob".to_string(), "NEWPC\\bob".to_string());
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map,
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
        assert_eq!(plan[0].target_user, Some("NEWPC\\bob".to_string()));
    }

    #[test]
    fn well_known_account_needs_no_mapping() {
        let (r, xml) = record("\\A\\Sys", LogonType::S4U, Some("SYSTEM"), "<Task>sys</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::Create);
    }

    #[test]
    fn access_denied_when_no_write_privileges() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(false);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: None,
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        let result = execute_plan(&scheduler, &plan, false);
        assert!(matches!(result, Err(TsbakError::AccessDenied { .. })));
    }

    #[test]
    fn remap_path_without_folder_is_identity() {
        assert_eq!(remap_path("\\A\\B\\Task", None), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("")), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("  ")), "\\A\\B\\Task");
        assert_eq!(remap_path("\\A\\B\\Task", Some("\\")), "\\A\\B\\Task");
    }

    #[test]
    fn remap_path_prefixes_while_preserving_structure() {
        assert_eq!(remap_path("\\Task", Some("\\Restore")), "\\Restore\\Task");
        assert_eq!(
            remap_path("\\A\\B\\Task", Some("\\Restore-2026-01-01")),
            "\\Restore-2026-01-01\\A\\B\\Task"
        );
        // Les slashes superflus du dossier sont nettoyes, la structure source conservee.
        assert_eq!(remap_path("\\A\\Task", Some("Restore\\")), "\\Restore\\A\\Task");
    }

    #[test]
    fn target_folder_prefixes_creation_and_report() {
        let (r, xml) = record("\\A\\NoSecret", LogonType::None, None, "<Task>a</Task>");
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml)]);
        let scheduler = MockScheduler::new(true);
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: Some("\\Restore".to_string()),
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].target_path, "\\Restore\\A\\NoSecret");
        assert_eq!(plan[0].action, ImportAction::Create);

        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.created, vec!["\\Restore\\A\\NoSecret".to_string()]);
        assert_eq!(report.exit_code(), 0);
        // La tache est bien enregistree sous le chemin cible, pas le chemin source.
        assert!(scheduler.task_exists("\\Restore\\A\\NoSecret").unwrap());
        assert!(!scheduler.task_exists("\\A\\NoSecret").unwrap());
    }

    #[test]
    fn target_folder_skips_identical_under_remapped_path() {
        // Une tache deja presente **sous le chemin cible** avec le meme XML
        // doit etre classee identique (et non recreee sous un chemin different).
        let xml = "<Task>same</Task>";
        let (r, xml_owned) = record("\\A\\Same", LogonType::None, None, xml);
        let (manifest, xmls) = manifest_and_xmls(vec![(r, xml_owned)]);
        let scheduler = MockScheduler::new(true).with_task(
            "\\Restore\\A\\Same",
            MockTask {
                xml: xml.to_string(),
                user_id: None,
                logon_type: LogonType::None,
            },
        );
        let resolver = PasswordResolver::new(None, None, false);
        let options = ImportOptions {
            conflict_policy: None,
            skip_password_tasks: false,
            user_map: HashMap::new(),
            answers: None,
            target_folder: Some("\\Restore".to_string()),
        };
        let plan = build_plan(&scheduler, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
        assert_eq!(plan[0].action, ImportAction::SkipIdentical);
        assert_eq!(plan[0].target_path, "\\Restore\\A\\Same");
        let report = execute_plan(&scheduler, &plan, false).unwrap();
        assert_eq!(report.skipped, vec!["\\Restore\\A\\Same".to_string()]);
        assert_eq!(scheduler.registration_count(), 0);
    }

    #[test]
    fn hash_mismatch_is_detected_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = Manifest::new(
            "H".to_string(),
            vec![TaskRecord {
                path: "\\A\\B".to_string(),
                xml_file: "A__B.xml".to_string(),
                sha256: "0000000000000000000000000000000000000000000000000000000000000".to_string(),
                user_id: None,
                logon_type: LogonType::None,
            }],
        );
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("A__B.xml"), "<Task/>").unwrap();
        let result = load_and_verify(dir.path());
        assert!(matches!(result, Err(TsbakError::HashMismatch { .. })));
    }
}

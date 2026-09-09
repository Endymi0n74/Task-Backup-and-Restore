//! Test d'integration : export complet suivi d'un reimport sur une
//! instance "vide" du planificateur, verifiant que le round-trip ne
//! declenche jamais d'erreur XML et que le dry-run ne modifie rien.

use std::collections::HashMap;

use tempfile::tempdir;
use tsbak::answers::AnswerFile;
use tsbak::export::{export, PatternFilter};
use tsbak::import::{build_plan, execute_plan, load_and_verify, ImportOptions};
use tsbak::model::{ImportAction, LogonType};
use tsbak::password::PasswordResolver;
use tsbak::scheduler::mock::{MockScheduler, MockTask};
use tsbak::scheduler::TaskSchedulerApi;
use tsbak::wizard::NullInteractor;

fn source_scheduler() -> MockScheduler {
    MockScheduler::new(true)
        .with_task(
            "\\Backup\\Nightly",
            MockTask {
                xml: "<Task xmlns=\"x\"><Actions><Exec><Command>a.exe</Command></Exec></Actions></Task>"
                    .to_string(),
                user_id: Some("NT AUTHORITY\\SYSTEM".to_string()),
                logon_type: LogonType::ServiceAccount,
            },
        )
        .with_task(
            "\\Backup\\Weekly",
            MockTask {
                xml: "<Task xmlns=\"x\"><Actions><Exec><Command>b.exe</Command></Exec></Actions></Task>"
                    .to_string(),
                user_id: Some("DOMAIN\\alice".to_string()),
                logon_type: LogonType::Password,
            },
        )
}

#[test]
fn full_export_then_import_roundtrip_no_xml_error() {
    let source = source_scheduler();
    let dir = tempdir().unwrap();
    let filter = PatternFilter::new(vec![], vec![]);
    export(&source, dir.path(), true, &filter, "SOURCE-PC").unwrap();

    // Verifie l'integrite de l'archive independamment (equivalent de `tsbak validate`).
    let (manifest, xmls) = load_and_verify(dir.path()).expect("l'archive exportee doit etre valide");
    assert_eq!(manifest.tasks.len(), 2);

    let target = MockScheduler::new(true);
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
    let plan = build_plan(&target, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
    assert!(plan.iter().all(|p| p.action == ImportAction::Create));

    let report = execute_plan(&target, &plan, false).unwrap();
    assert_eq!(report.exit_code(), 0);
    assert_eq!(report.created.len(), 2);

    // Le XML enregistre sur la cible doit etre strictement identique a celui exporte.
    for record in &manifest.tasks {
        let expected = xmls.get(&record.path).unwrap();
        let actual = target.get_task_xml(&record.path).unwrap();
        assert_eq!(&actual, expected);
    }
}

#[test]
fn dry_run_on_full_roundtrip_never_writes() {
    let source = source_scheduler();
    let dir = tempdir().unwrap();
    let filter = PatternFilter::new(vec![], vec![]);
    export(&source, dir.path(), true, &filter, "SOURCE-PC").unwrap();
    let (manifest, xmls) = load_and_verify(dir.path()).unwrap();

    let target = MockScheduler::new(true);
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
    let plan = build_plan(&target, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
    let report = execute_plan(&target, &plan, true).unwrap();
    assert_eq!(report.created.len(), 2);
    assert_eq!(target.registration_count(), 0, "le dry-run ne doit rien ecrire");
}

#[test]
fn answer_file_drives_fully_non_interactive_import() {
    let source = source_scheduler();
    let dir = tempdir().unwrap();
    let filter = PatternFilter::new(vec![], vec![]);
    export(&source, dir.path(), true, &filter, "SOURCE-PC").unwrap();
    let (manifest, xmls) = load_and_verify(dir.path()).unwrap();

    let answer_json = r#"{
        "user_map": {},
        "passwords": { "DOMAIN\\alice": "s3cret" },
        "conflict_decisions": {},
        "skip_tasks": []
    }"#;
    let answer_path = dir.path().join("answers.json");
    std::fs::write(&answer_path, answer_json).unwrap();
    let answers = AnswerFile::load(&answer_path).unwrap();

    let target = MockScheduler::new(true);
    let resolver = PasswordResolver::new(None, Some(answers.passwords.clone()), false);
    let options = ImportOptions {
        conflict_policy: None,
        skip_password_tasks: false,
        user_map: HashMap::new(),
        answers: Some(&answers),
        target_folder: None,
    };
    let plan = build_plan(&target, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
    assert!(plan.iter().all(|p| p.action == ImportAction::Create));
    let report = execute_plan(&target, &plan, false).unwrap();
    assert_eq!(report.exit_code(), 0);
}

#[test]
fn non_tty_without_answer_blocks_cleanly_with_exit_code_one() {
    let source = source_scheduler();
    let dir = tempdir().unwrap();
    let filter = PatternFilter::new(vec![], vec![]);
    export(&source, dir.path(), true, &filter, "SOURCE-PC").unwrap();
    let (manifest, xmls) = load_and_verify(dir.path()).unwrap();

    // Aucune source de mot de passe, aucun interacteur : la tache avec mot
    // de passe doit etre bloquee proprement, sans empecher les autres.
    let target = MockScheduler::new(true);
    let resolver = PasswordResolver::new(None, None, false);
    let options = ImportOptions {
        conflict_policy: None,
        skip_password_tasks: false,
        user_map: HashMap::new(),
        answers: None,
        target_folder: None,
    };
    let plan = build_plan(&target, &manifest, &xmls, &options, &resolver, &NullInteractor).unwrap();
    let report = execute_plan(&target, &plan, false).unwrap();
    assert_eq!(report.created.len(), 1, "la tache SYSTEM doit passer sans intervention");
    assert_eq!(report.blocked.len(), 1, "la tache avec mot de passe doit etre bloquee");
    assert_eq!(report.exit_code(), 1);
}

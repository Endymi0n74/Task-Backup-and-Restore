//! Archives ZIP pour l'export/import : compression d'un dossier d'export
//! (XML bruts + manifest.json) en `.zip`, éventuellement chiffré AES-256
//! (variante WinZip AES), et extraction inverse.
//!
//! Garanties :
//! - **contenu restreint** : une archive produite ici ne contient QUE les XML
//!   de tâches planifiées et le `manifest.json` — tout autre fichier présent
//!   dans le dossier d'export (ancien `.zip`, journal, note…) est ignoré, à
//!   la compression comme à l'extraction ;
//! - le manifeste et les empreintes SHA-256 sont des fichiers comme les
//!   autres dans l'archive : rien n'est recalculé, la vérification
//!   d'intégrité de `tsbak` (`load_and_verify`) s'applique à l'identique
//!   après extraction ;
//! - l'extraction refuse les chemins absolus et les remontées `..`
//!   (protection zip-slip) : chaque nom d'entrée est validé par
//!   `enclosed_name()` avant toute écriture ;
//! - le mot de passe d'archive n'est **jamais journalisé** (aucun paramètre
//!   de log ne l'accepte) ; seul le fait qu'une archive soit chiffrée est
//!   journalisé.
//!
//! Extraction en deux temps : d'abord dans un dossier temporaire, puis
//! validation du manifeste via `tsbak::import::load_and_verify`, et
//! seulement ensuite renommage vers la destination finale. Une archive
//! invalide ne pollue jamais la destination.

use std::fs;
use std::io::Write;
use std::path::Path;

use zip::read::ZipArchive;
use zip::write::{FileOptions, SimpleFileOptions};
use zip::{AesMode, CompressionMethod, ZipWriter};

use crate::app_log::AppLog;

/// Noms admissibles dans une archive d'export : le `manifest.json` de la
/// racine et les fichiers `.xml` de tâches (extension insensible à la casse).
/// Tout le reste — anciennes archives, journaux, fichiers personnels — est
/// ignoré, en écriture comme en lecture.
fn is_export_entry(rel_name: &str) -> bool {
    let lower = rel_name.to_lowercase();
    lower == "manifest.json" || lower.ends_with(".xml")
}

/// Compresse `src_dir` vers `zip_path`. Si `password` est `Some`, l'archive
/// est chiffrée en AES-256 ; sinon compression Deflate seule. Seuls les XML
/// de tâches et `manifest.json` sont inclus ; les autres fichiers sont
/// ignorés (comptabilisés et journalisés).
pub fn zip_dir(
    log: &AppLog,
    src_dir: &Path,
    zip_path: &Path,
    password: Option<&str>,
) -> Result<(), String> {
    let file = fs::File::create(zip_path)
        .map_err(|e| format!("impossible de créer {} : {e}", zip_path.display()))?;
    let mut writer = ZipWriter::new(file);

    let options: FileOptions<'_, ()> = match password {
        Some(pw) => {
            log.info("Archive d'export chiffrée en AES-256 (mot de passe jamais journalisé)");
            SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .with_aes_encryption(AesMode::Aes256, pw)
        }
        None => SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    };

    let mut count = 0usize;
    let mut ignored = 0usize;
    collect_and_write(&mut writer, src_dir, Path::new(""), options, &mut count, &mut ignored)?;
    writer
        .finish()
        .map_err(|e| format!("clôture de l'archive : {e}"))?;

    if ignored > 0 {
        log.info(&format!(
            "Archive ZIP : {} fichier(s) hors export (non-XML/manifeste) ignoré(s)",
            ignored
        ));
    }
    log.info(&format!(
        "Archive ZIP créée : {} fichier(s) (XML de tâches + manifeste) vers {}",
        count,
        zip_path.display()
    ));
    Ok(())
}

/// Écrit récursivement le contenu de `fs_dir` (chemins relatifs `rel_dir`)
/// dans l'archive en cours d'écriture.
fn collect_and_write(
    writer: &mut ZipWriter<std::fs::File>,
    fs_dir: &Path,
    rel_dir: &Path,
    options: FileOptions<'_, ()>,
    count: &mut usize,
    ignored: &mut usize,
) -> Result<(), String> {
    let entries =
        fs::read_dir(fs_dir).map_err(|e| format!("lecture de {} : {e}", fs_dir.display()))?;
    let mut names: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .collect();
    // Tri stable : ordre d'archive déterministe (utile pour les tests).
    names.sort();
    for path in names {
        let name = rel_dir.join(path.file_name().unwrap_or_default());
        let name_str = name.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            collect_and_write(writer, &path, &name, options, count, ignored)?;
        } else if is_export_entry(&name_str) {
            writer
                .start_file(name_str.as_str(), options)
                .map_err(|e| format!("démarrage de l'entrée '{name_str}' : {e}"))?;
            let data = fs::read(&path)
                .map_err(|e| format!("lecture de {} : {e}", path.display()))?;
            writer
                .write_all(&data)
                .map_err(|e| format!("écriture de '{name_str}' : {e}"))?;
            *count += 1;
        } else {
            // Tout fichier qui n'est ni un XML de tâche ni le manifeste est
            // volontairement exclu de l'archive.
            *ignored += 1;
        }
    }
    Ok(())
}

/// Extrait `zip_path` dans `dest_dir` après **validation** de l'archive
/// extraite (manifeste + empreintes SHA-256). En cas d'échec de validation,
/// l'extraction est annulée et `dest_dir` n'est pas créé.
///
/// Seuls les XML de tâches et le `manifest.json` sont extraits : toute autre
/// entrée présente dans l'archive est ignorée (et journalisée).
///
/// Si l'archive est chiffrée, `password` doit correspondre : l'erreur de
/// mot de passe est signalée clairement. Retourne le nombre de fichiers
/// extraits.
pub fn unzip_verified(
    log: &AppLog,
    zip_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
) -> Result<usize, String> {
    if dest_dir.exists() {
        return Err(format!(
            "la destination {} existe déjà, choisissez un autre nom",
            dest_dir.display()
        ));
    }
    let staging = temp_dir_for(dest_dir);
    let _ = fs::create_dir_all(&staging);

    let (extracted, ignored) = match extract_all(zip_path, &staging, password) {
        Ok(counts) => counts,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    if ignored > 0 {
        log.info(&format!(
            "Archive ZIP : {} entrée(s) hors export (non-XML/manifeste) ignorée(s) à l'extraction",
            ignored
        ));
    }

    // Validation d'intégrité (manifeste + empreintes) avant de promouvoir
    // l'extraction vers sa destination finale.
    if let Err(e) = tsbak::import::load_and_verify(&staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("archive invalide après extraction : {e}"));
    }

    if let Err(rename_err) = fs::rename(&staging, dest_dir) {
        // Renommage impossible (destinations sur disques différents ?) :
        // on tente une copie complète avant d'abandonner.
        copy_dir_recursive(&staging, dest_dir).map_err(|copy_err| {
            let _ = fs::remove_dir_all(&staging);
            format!(
                "installation de {} : rename : {rename_err} ; copie : {copy_err}",
                dest_dir.display()
            )
        })?;
        let _ = fs::remove_dir_all(&staging);
    }

    let count = fs::read_dir(dest_dir)
        .map(|d| d.flatten().count())
        .unwrap_or(0);
    log.info(&format!(
        "Archive ZIP extraite et vérifiée vers {} ({} fichier(s) XML/manifeste extraits, {} entrée(s) ignorée(s))",
        dest_dir.display(),
        extracted,
        ignored
    ));
    Ok(count)
}

/// Extrait toutes les entrées de `zip_path` vers `dest_dir`, avec protection
/// zip-slip (`enclosed_name`) et gestion du chiffrement AES/ZipCrypto.
/// Seules les entrées d'export (XML + manifeste) sont écrites ; retourne
/// `(extraites, ignorées)`.
fn extract_all(
    zip_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
) -> Result<(usize, usize), String> {
    let file = fs::File::open(zip_path)
        .map_err(|e| format!("ouverture de {} : {e}", zip_path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("lecture de l'archive ZIP : {e}"))?;

    let mut extracted = 0usize;
    let mut ignored = 0usize;

    for i in 0..archive.len() {
        let mut entry = if let Some(pw) = password {
            match archive.by_index_decrypt(i, pw.as_bytes()) {
                Ok(e) => e,
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err("mot de passe d'archive incorrect".to_string())
                }
                Err(e) => {
                    return Err(format!(
                        "entrée {} : {e} (archive chiffrée ? mot de passe requis ?)",
                        i
                    ))
                }
            }
        } else {
            match archive.by_index(i) {
                Ok(e) => e,
                Err(zip::result::ZipError::UnsupportedArchive(
                    zip::result::ZipError::PASSWORD_REQUIRED,
                )) => {
                    return Err("archive chiffrée : un mot de passe est requis".to_string())
                }
                Err(e) => return Err(format!("entrée {i} : {e}")),
            }
        };

        // Protection zip-slip : refuse les chemins absolus et les `..`.
        let Some(rel) = entry.enclosed_name() else {
            return Err(format!(
                "entrée {} : chemin non sûr refusé (zip-slip)",
                entry.name()
            ));
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let out_path = dest_dir.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| format!("création du dossier {} : {e}", out_path.display()))?;
        } else if !is_export_entry(&rel_str) {
            // Entrée étrangère (ancienne archive, journal, fichier personnel…) :
            // ignorée volontairement, jamais écrite sur disque.
            ignored += 1;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("création du dossier {} : {e}", parent.display()))?;
            }
            let mut out = fs::File::create(&out_path)
                .map_err(|e| format!("création de {} : {e}", out_path.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("extraction de {} : {e}", out_path.display()))?;
            extracted += 1;
        }
    }
    Ok((extracted, ignored))
}

/// Dossier temporaire frère de la destination (même disque => rename atomique).
fn temp_dir_for(dest_dir: &Path) -> std::path::PathBuf {
    let parent = dest_dir.parent().unwrap_or_else(|| Path::new("."));
    let base = dest_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "extract".to_string());
    parent.join(format!(".tsbak-extract-{base}"))
}

/// Copie récursive de secours si `rename` échoue (disques différents).
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_log() -> AppLog {
        let dir = tempfile::tempdir().unwrap();
        AppLog::with_dir(dir.path().to_path_buf())
    }

    /// Écrit un dossier d'export **valide** : manifeste conforme au format
    /// `tsbak::model::Manifest` avec de vraies empreintes SHA-256.
    fn sample_export(dir: &Path) {
        fs::create_dir_all(dir.join("Backup")).unwrap();
        let nightly = "<Task>nightly</Task>";
        let weekly = "<Task>weekly</Task>";
        fs::write(dir.join("Backup__Nightly.xml"), nightly).unwrap();
        fs::write(dir.join("Backup__Weekly.xml"), weekly).unwrap();
        let manifest = format!(
            r#"{{"version":1,"exported_at":"2026-09-08T12:00:00Z","source_host":"PC-TEST","tasks":[{{"path":"\\Backup\\Nightly","xml_file":"Backup__Nightly.xml","sha256":"{h1}","user_id":null,"logon_type":"None"}},{{"path":"\\Backup\\Weekly","xml_file":"Backup__Weekly.xml","sha256":"{h2}","user_id":null,"logon_type":"None"}}]}}"#,
            h1 = tsbak::hash::sha256_hex(nightly.as_bytes()),
            h2 = tsbak::hash::sha256_hex(weekly.as_bytes()),
        );
        fs::write(dir.join("manifest.json"), manifest).unwrap();
    }

    #[test]
    fn roundtrip_plain_zip() {
        let log = test_log();
        let src = tempfile::tempdir().unwrap();
        sample_export(src.path());
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("export.zip");

        zip_dir(&log, src.path(), &zip_path, None).unwrap();
        assert!(zip_path.exists());

        let dest = out.path().join("extracted");
        let count = unzip_verified(&log, &zip_path, &dest, None).unwrap();
        assert!(count >= 1);

        // Contenu identique après roundtrip (manifeste inclus, empreintes intactes).
        assert!(fs::read_to_string(dest.join("manifest.json"))
            .unwrap()
            .contains("\"source_host\":\"PC-TEST\""));
        assert_eq!(
            fs::read_to_string(dest.join("Backup__Nightly.xml")).unwrap(),
            "<Task>nightly</Task>"
        );
        assert_eq!(
            fs::read_to_string(dest.join("Backup__Weekly.xml")).unwrap(),
            "<Task>weekly</Task>"
        );
    }

    #[test]
    fn roundtrip_aes_encrypted_zip() {
        let log = test_log();
        let src = tempfile::tempdir().unwrap();
        sample_export(src.path());
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("export-enc.zip");

        zip_dir(&log, src.path(), &zip_path, Some("mot-de-passe-fort")).unwrap();

        // Sans mot de passe : erreur claire.
        let dest_bad = out.path().join("no-password");
        let err = unzip_verified(&log, &zip_path, &dest_bad, None).unwrap_err();
        assert!(
            err.contains("mot de passe"),
            "erreur attendue sur mot de passe manquant, obtenu : {err}"
        );

        // Avec un mauvais mot de passe : erreur claire.
        let dest_wrong = out.path().join("wrong-password");
        let err = unzip_verified(&log, &zip_path, &dest_wrong, Some("mauvais")).unwrap_err();
        assert!(err.contains("mot de passe"), "obtenu : {err}");

        // Avec le bon mot de passe : roundtrip complet.
        let dest = out.path().join("extracted-enc");
        unzip_verified(&log, &zip_path, &dest, Some("mot-de-passe-fort")).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("Backup__Nightly.xml")).unwrap(),
            "<Task>nightly</Task>"
        );
    }

    #[test]
    fn invalid_archive_is_rejected_and_dest_not_created() {
        let log = test_log();
        let out = tempfile::tempdir().unwrap();

        // Manifeste référençant un XML dont l'empreinte ne correspond pas :
        // la vérification d'intégrité de `tsbak` doit rejeter l'archive.
        let bad_src = tempfile::tempdir().unwrap();
        fs::write(bad_src.path().join("X.xml"), "<Task>x</Task>").unwrap();
        fs::write(
            bad_src.path().join("manifest.json"),
            r#"{"version":1,"exported_at":"2026-09-08T12:00:00Z","source_host":"PC-TEST","tasks":[{"path":"\\X","xml_file":"X.xml","sha256":"0000","user_id":null,"logon_type":"None"}]}"#,
        )
        .unwrap();
        let zip_path = out.path().join("bad.zip");
        zip_dir(&log, bad_src.path(), &zip_path, None).unwrap();

        let dest = out.path().join("should-not-exist");
        assert!(unzip_verified(&log, &zip_path, &dest, None).is_err());
        assert!(!dest.exists(), "la destination ne doit pas être créée");
        // Le dossier temporaire a été nettoyé.
        let staging = temp_dir_for(&dest);
        assert!(!staging.exists(), "le dossier temporaire doit être nettoyé");
    }

    #[test]
    fn zip_slip_is_rejected() {
        let log = test_log();
        let out = tempfile::tempdir().unwrap();

        // Fabrique à la main une archive avec une entrée malveillante.
        let zip_path = out.path().join("evil.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = ZipWriter::new(file);
            w.start_file("../evil.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"pwned").unwrap();
            w.finish().unwrap();
        }

        let dest = out.path().join("extracted");
        let err = unzip_verified(&log, &zip_path, &dest, None).unwrap_err();
        assert!(
            err.contains("zip-slip"),
            "erreur zip-slip attendue, obtenu : {err}"
        );
        assert!(!out.path().join("evil.txt").exists());
    }

    #[test]
    fn zip_contains_only_task_xml_and_manifest() {
        let log = test_log();
        let src = tempfile::tempdir().unwrap();
        sample_export(src.path());
        // Fichiers étrangers dans le dossier d'export : ils ne doivent PAS
        // entrer dans l'archive (anciennes archives, journaux, notes…).
        fs::write(src.path().join("ancien-export.zip"), b"old zip").unwrap();
        fs::write(src.path().join("export-2026-09-09.log"), b"log").unwrap();
        fs::write(src.path().join("notes.txt"), b"notes").unwrap();
        fs::write(src.path().join("schema.json"), b"{}").unwrap();
        fs::create_dir_all(src.path().join("sous-dossier")).unwrap();
        fs::write(src.path().join("sous-dossier").join("divers.bin"), b"x").unwrap();

        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("export.zip");
        zip_dir(&log, src.path(), &zip_path, None).unwrap();

        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["Backup__Nightly.xml", "Backup__Weekly.xml", "manifest.json"],
            "l'archive ne doit contenir que les XML de tâches et le manifeste"
        );
    }

    #[test]
    fn extraction_ignores_foreign_entries() {
        let log = test_log();
        let out = tempfile::tempdir().unwrap();

        // Archive valide + une entrée étrangère (ancien journal) et un XML
        // inconnu du manifeste (toléré à l'extraction, rejeté plus tard par
        // la vérification d'intégrité uniquement s'il est référencé). Ici le
        // XML étranger n'est pas référencé : il doit être ignoré à l'écriture.
        let zip_path = out.path().join("mixed.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            let xml = "<Task>nightly</Task>";
            w.start_file("Nightly.xml", opts).unwrap();
            w.write_all(xml.as_bytes()).unwrap();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(
                format!(
                    r#"{{"version":1,"exported_at":"2026-09-08T12:00:00Z","source_host":"PC-TEST","tasks":[{{"path":"\\Nightly","xml_file":"Nightly.xml","sha256":"{h}","user_id":null,"logon_type":"None"}}]}}"#,
                    h = tsbak::hash::sha256_hex(xml.as_bytes()),
                )
                .as_bytes(),
            )
            .unwrap();
            w.start_file("vieux-journal.log", opts).unwrap();
            w.write_all(b"secret").unwrap();
            w.start_file("Archive ancienne.zip", opts).unwrap();
            w.write_all(b"PK").unwrap();
            w.finish().unwrap();
        }

        let dest = out.path().join("extracted");
        unzip_verified(&log, &zip_path, &dest, None).unwrap();
        assert!(dest.join("Nightly.xml").exists());
        assert!(dest.join("manifest.json").exists());
        assert!(!dest.join("vieux-journal.log").exists(), "entrée étrangère non extraite");
        assert!(!dest.join("Archive ancienne.zip").exists(), "entrée étrangère non extraite");    }

    #[test]
    fn subdirectories_are_preserved() {
        let log = test_log();
        let src = tempfile::tempdir().unwrap();
        fs::create_dir_all(src.path().join("Backup")).unwrap();
        let deep = "<Task>deep</Task>";
        fs::write(src.path().join("Backup").join("Deep__Deep.xml"), deep).unwrap();
        let manifest = format!(
            r#"{{"version":1,"exported_at":"2026-09-08T12:00:00Z","source_host":"PC-TEST","tasks":[{{"path":"\\Backup\\Deep","xml_file":"Backup/Deep__Deep.xml","sha256":"{h}","user_id":null,"logon_type":"None"}}]}}"#,
            h = tsbak::hash::sha256_hex(deep.as_bytes()),
        );
        fs::write(src.path().join("manifest.json"), manifest).unwrap();

        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("sub.zip");
        zip_dir(&log, src.path(), &zip_path, None).unwrap();

        let dest = out.path().join("extracted");
        unzip_verified(&log, &zip_path, &dest, None).unwrap();
        // Les sous-dossiers traversent la compression et la vérification.
        assert_eq!(
            fs::read_to_string(dest.join("Backup").join("Deep__Deep.xml")).unwrap(),
            "<Task>deep</Task>"
        );
    }
}
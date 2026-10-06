//! tsbak : export/import des taches planifiees Windows (Task Scheduler) au
//! format XML brut, pour migration ou restauration entre machines.
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod answers;
pub mod error;
pub mod export;
pub mod hash;
pub mod import;
pub mod log;
pub mod model;
pub mod password;
pub mod scheduler;
pub mod wizard;

/// Nom de la machine hôte (variable `COMPUTERNAME`, repli `HOSTNAME`, puis
/// `unknown-host`) : figure dans le manifeste d'export et les journaux.
/// Partagé par le CLI et l'interface pour des empreintes strictement
/// identiques d'une interface à l'autre.
pub fn local_host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

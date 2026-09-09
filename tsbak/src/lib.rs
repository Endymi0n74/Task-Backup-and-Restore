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

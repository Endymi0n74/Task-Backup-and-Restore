//! Implementation reelle de `TaskSchedulerApi` via l'API COM du Task
//! Scheduler Windows (ITaskService / ITaskFolder / IRegisteredTask), en
//! s'appuyant sur les bindings officiels `windows-rs`.
//!
//! Ce module n'est compile que pour une cible Windows (`cfg(windows)`).
//! Il ne fait aucune transformation du XML lu depuis le planificateur :
//! `get_task_xml` renvoie exactement la chaine retournee par
//! `IRegisteredTask::Xml`, condition necessaire pour un reimport fiable.
//!
//! Signatures verifiees contre les bindings reels de `windows-rs` 0.58
//! (compilation + execution sur machine Windows) le 8 septembre 2026 :
//! certaines methodes (IPrincipal::UserId, IPrincipal::LogonType)
//! utilisent des parametres de sortie `*mut T`, les collections sont
//! 1-based (`get_Item`/`Count`), et `RegisterTask` suit l'ordre (path,
//! xmltext, flags, userid, password, logontype, sddl). La detection
//! d'elevation (`is_elevated`) suit le meme schema que la crate publiee
//! `check_elevation` (OpenProcessToken avec un `TOKEN_ACCESS_MASK`,
//! GetTokenInformation(TokenElevation, ...)).

use windows::core::{BSTR, VARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::TaskScheduler::{
    ITaskFolder, ITaskService, TaskScheduler, TASK_CREATE_OR_UPDATE, TASK_LOGON_GROUP,
    TASK_LOGON_INTERACTIVE_TOKEN, TASK_LOGON_INTERACTIVE_TOKEN_OR_PASSWORD, TASK_LOGON_NONE,
    TASK_LOGON_PASSWORD, TASK_LOGON_S4U, TASK_LOGON_SERVICE_ACCOUNT, TASK_LOGON_TYPE,
};

use crate::error::{translate_hresult, Result, TsbakError};
use crate::model::LogonType;
use crate::scheduler::{parent_folder, TaskAuthInfo, TaskHandle, TaskSchedulerApi};

fn to_win_logon(l: LogonType) -> TASK_LOGON_TYPE {
    match l {
        LogonType::None => TASK_LOGON_NONE,
        LogonType::Password => TASK_LOGON_PASSWORD,
        LogonType::InteractiveTokenOrPassword => TASK_LOGON_INTERACTIVE_TOKEN_OR_PASSWORD,
        LogonType::InteractiveToken => TASK_LOGON_INTERACTIVE_TOKEN,
        LogonType::Group => TASK_LOGON_GROUP,
        LogonType::ServiceAccount => TASK_LOGON_SERVICE_ACCOUNT,
        LogonType::S4U => TASK_LOGON_S4U,
    }
}

fn from_win_logon(l: TASK_LOGON_TYPE) -> LogonType {
    match l {
        TASK_LOGON_PASSWORD => LogonType::Password,
        TASK_LOGON_INTERACTIVE_TOKEN_OR_PASSWORD => LogonType::InteractiveTokenOrPassword,
        TASK_LOGON_INTERACTIVE_TOKEN => LogonType::InteractiveToken,
        TASK_LOGON_GROUP => LogonType::Group,
        TASK_LOGON_SERVICE_ACCOUNT => LogonType::ServiceAccount,
        TASK_LOGON_S4U => LogonType::S4U,
        _ => LogonType::None,
    }
}

/// Wrapper autour d'ITaskService. Initialise COM sur le thread courant.
///
/// Le champ est un `Option` pour permettre au `Drop` de liberer l'interface
/// **avant** `CoUninitialize` : relacher des pointeurs COM apres
/// `CoUninitialize` provoque une violation d'acces a la fermeture du
/// processus (STATUS_ACCESS_VIOLATION, observee lors des premiers tests
/// reels sur machine Windows).
pub struct WindowsScheduler {
    service: Option<ITaskService>,
}

impl WindowsScheduler {
    /// Se connecte au service de planification de taches local
    /// (CoCreateInstance + ITaskService::Connect sur le thread courant).
    pub fn connect() -> Result<Self> {
        unsafe {
            // Erreur ignoree si deja initialise (RPC_E_CHANGED_MODE compris) : on
            // laisse l'appelant realiser un seul CoInitializeEx par thread.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).map_err(|e| {
                    translate_hresult("CoCreateInstance(TaskScheduler)", e.code().0 as u32)
                })?;
            service
                .Connect(&VARIANT::default(), &VARIANT::default(), &VARIANT::default(), &VARIANT::default())
                .map_err(|e| translate_hresult("ITaskService::Connect", e.code().0 as u32))?;
            Ok(WindowsScheduler { service: Some(service) })
        }
    }

    /// Acces au service COM. Toujours `Some` tant que l'objet est vivant
    /// (vide uniquement pendant `Drop`, juste avant `CoUninitialize`).
    fn svc(&self) -> &ITaskService {
        self.service
            .as_ref()
            .expect("ITaskService present tant que WindowsScheduler est vivant")
    }

    fn folder(&self, path: &str) -> Result<ITaskFolder> {
        unsafe {
            self.svc()
                .GetFolder(&BSTR::from(path))
                .map_err(|e| translate_hresult(&format!("GetFolder({path})"), e.code().0 as u32))
        }
    }

    fn walk_folder(&self, path: &str, recursive: bool, out: &mut Vec<TaskHandle>) -> Result<()> {
        unsafe {
            let folder = self.folder(path)?;
            let tasks = folder
                .GetTasks(0)
                .map_err(|e| translate_hresult(&format!("GetTasks({path})"), e.code().0 as u32))?;
            let count = tasks
                .Count()
                .map_err(|e| translate_hresult("IRegisteredTaskCollection::Count", e.code().0 as u32))?;
            for i in 1..=count {
                let item = tasks
                    .get_Item(&VARIANT::from(i))
                    .map_err(|e| translate_hresult("IRegisteredTaskCollection::Item", e.code().0 as u32))?;
                let name = item
                    .Path()
                    .map_err(|e| translate_hresult("IRegisteredTask::Path", e.code().0 as u32))?;
                out.push(TaskHandle {
                    path: name.to_string(),
                });
            }
            if recursive {
                let subfolders = folder
                    .GetFolders(0)
                    .map_err(|e| translate_hresult(&format!("GetFolders({path})"), e.code().0 as u32))?;
                let fcount = subfolders
                    .Count()
                    .map_err(|e| translate_hresult("ITaskFolderCollection::Count", e.code().0 as u32))?;
                for i in 1..=fcount {
                    let sub = subfolders
                        .get_Item(&VARIANT::from(i))
                        .map_err(|e| translate_hresult("ITaskFolderCollection::Item", e.code().0 as u32))?;
                    let sub_path = sub
                        .Path()
                        .map_err(|e| translate_hresult("ITaskFolder::Path", e.code().0 as u32))?
                        .to_string();
                    self.walk_folder(&sub_path, recursive, out)?;
                }
            }
            Ok(())
        }
    }
}

impl Drop for WindowsScheduler {
    fn drop(&mut self) {
        unsafe {
            // Libere ITaskService pendant que COM est encore initialise,
            // puis deinitialise COM (voir commentaire sur le struct).
            drop(self.service.take());
            CoUninitialize();
        }
    }
}

impl TaskSchedulerApi for WindowsScheduler {
    fn list_tasks(&self, recursive: bool) -> Result<Vec<TaskHandle>> {
        let mut out = Vec::new();
        self.walk_folder("\\", recursive, &mut out)?;
        Ok(out)
    }

    fn get_task_xml(&self, path: &str) -> Result<String> {
        unsafe {
            let folder_path = parent_folder(path);
            let name = path.rsplit('\\').next().unwrap_or(path);
            let folder = self.folder(&folder_path)?;
            let task = folder
                .GetTask(&BSTR::from(name))
                .map_err(|e| translate_hresult(&format!("GetTask({path})"), e.code().0 as u32))?;
            let xml = task
                .Xml()
                .map_err(|e| translate_hresult("IRegisteredTask::Xml", e.code().0 as u32))?;
            Ok(xml.to_string())
        }
    }

    fn get_task_auth_info(&self, path: &str) -> Result<TaskAuthInfo> {
        unsafe {
            let folder_path = parent_folder(path);
            let name = path.rsplit('\\').next().unwrap_or(path);
            let folder = self.folder(&folder_path)?;
            let task = folder
                .GetTask(&BSTR::from(name))
                .map_err(|e| translate_hresult(&format!("GetTask({path})"), e.code().0 as u32))?;
            let def = task
                .Definition()
                .map_err(|e| translate_hresult("IRegisteredTask::Definition", e.code().0 as u32))?;
            let principal = def
                .Principal()
                .map_err(|e| translate_hresult("ITaskDefinition::Principal", e.code().0 as u32))?;
            // UserId/LogonType utilisent des parametres de sortie dans les
            // bindings windows-0.58 : on les remplit puis on les lit.
            let mut user_bstr = BSTR::default();
            let user_id = match principal.UserId(&mut user_bstr) {
                Ok(()) => {
                    let s = user_bstr.to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                }
                // Certaines taches n'exposent pas de UserId : ce n'est pas
                // une erreur bloquante, l'info est simplement absente.
                Err(_) => None,
            };
            let mut logon_type = TASK_LOGON_TYPE::default();
            principal
                .LogonType(&mut logon_type)
                .map_err(|e| translate_hresult("IPrincipal::LogonType", e.code().0 as u32))?;
            Ok(TaskAuthInfo {
                user_id,
                logon_type: from_win_logon(logon_type),
            })
        }
    }

    fn task_exists(&self, path: &str) -> Result<bool> {
        unsafe {
            let folder_path = parent_folder(path);
            let name = path.rsplit('\\').next().unwrap_or(path);
            let folder = match self.folder(&folder_path) {
                Ok(f) => f,
                Err(_) => return Ok(false),
            };
            Ok(folder.GetTask(&BSTR::from(name)).is_ok())
        }
    }

    fn ensure_folder(&self, folder_path: &str) -> Result<()> {
        if folder_path == "\\" || folder_path.is_empty() {
            return Ok(());
        }
        unsafe {
            let root = self.folder("\\")?;
            let mut current = String::new();
            for part in folder_path.split('\\').filter(|s| !s.is_empty()) {
                current.push('\\');
                current.push_str(part);
                // CreateFolder echoue si le dossier existe deja : on ignore ce cas precis.
                let _ = root.CreateFolder(&BSTR::from(current.as_str()), &VARIANT::default());
            }
            Ok(())
        }
    }

    fn register_task(
        &self,
        path: &str,
        xml: &str,
        user_id: Option<&str>,
        password: Option<&str>,
        logon_type: LogonType,
    ) -> Result<()> {
        if !self.has_write_privileges() {
            return Err(TsbakError::AccessDenied {
                operation: format!("RegisterTask({path})"),
            });
        }
        unsafe {
            let folder_path = parent_folder(path);
            let name = path.rsplit('\\').next().unwrap_or(path);
            self.ensure_folder(&folder_path)?;
            let folder = self.folder(&folder_path)?;

            let user_variant = match user_id {
                Some(u) => VARIANT::from(BSTR::from(u)),
                None => VARIANT::default(),
            };
            let password_variant = match password {
                Some(p) => VARIANT::from(BSTR::from(p)),
                None => VARIANT::default(),
            };

            folder
                .RegisterTask(
                    &BSTR::from(name),
                    &BSTR::from(xml),
                    TASK_CREATE_OR_UPDATE.0,
                    &user_variant,
                    &password_variant,
                    to_win_logon(logon_type),
                    &VARIANT::default(),
                )
                .map_err(|e| translate_hresult(&format!("RegisterTask({path})"), e.code().0 as u32))?;
            Ok(())
        }
    }

    fn has_write_privileges(&self) -> bool {
        is_elevated()
    }
}

/// Verifie si le process courant tourne avec un jeton d'administrateur elevé.
/// Implementation confirmee par comparaison avec `check_elevation` (crate
/// tierce publiee, meme approche a l'identique) : `OpenProcessToken` attend
/// un `TOKEN_ACCESS_MASK`, pas directement la constante `TOKEN_QUERY`.
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ACCESS_MASK, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ACCESS_MASK(TOKEN_QUERY.0), &mut token)
            .is_err()
        {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .is_ok();
        ok && elevation.TokenIsElevated != 0
    }
}

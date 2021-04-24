use directories::ProjectDirs;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

const APP_QUALIFIER: &str = "xyz";
const APP_ORGANIZATION: &str = "overengineering";
const APP_NAME: &str = "conductor";

static EXECUTION_MODE_ACQUIRED: AtomicBool = AtomicBool::new(false);
static IS_SERVICE: AtomicBool = AtomicBool::new(false);

pub enum PathAsset {
    KeyStore,
    PeerCatalog,
}

pub enum ExecutionMode {
    Service,
    User,
}

#[cfg(target_os = "windows")]
fn get_system_data_folder() -> PathBuf {
    PathBuf::from("C:/ProgramData")
}

#[cfg(not(target_os = "windows"))]
fn get_system_data_folder() -> PathBuf {
    PathBuf::from("/etc")
}

pub fn locate_path(asset: PathAsset) -> PathBuf {
    let base_dir = match current_execution_mode() {
        ExecutionMode::Service => get_system_data_folder().join(APP_NAME),
        ExecutionMode::User => ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
            .expect("Failed to acqire app data folder")
            .data_dir()
            .to_owned(),
    };

    std::fs::create_dir_all(&base_dir).expect("Failed to create app data folder");

    match asset {
        PathAsset::PeerCatalog => base_dir.join("peer_catalog.json"),
        PathAsset::KeyStore => base_dir.join("keystore.json"),
    }
}

pub fn current_execution_mode() -> ExecutionMode {
    EXECUTION_MODE_ACQUIRED.store(true, Ordering::SeqCst);
    if IS_SERVICE.load(Ordering::SeqCst) {
        ExecutionMode::Service
    } else {
        ExecutionMode::User
    }
}

pub fn set_execution_mode(execution_mode: ExecutionMode) {
    // Protect application from changing execution mode after
    // it has been already acquired - this will prevent inconsistent
    // data load/store
    if EXECUTION_MODE_ACQUIRED.load(Ordering::SeqCst) {
        panic!("Can't set execution mode after program started");
    }
    if let ExecutionMode::Service = execution_mode {
        IS_SERVICE.store(true, Ordering::SeqCst);
    } else {
        IS_SERVICE.store(false, Ordering::SeqCst);
    }
}

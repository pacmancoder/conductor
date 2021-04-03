//! Concepts:
//! tunnel link - definition of service which will be visibleto the network
//! only owner can create listening connection for the tunnel link
//! users can connect to multiple rooms and create new rooms

use conductor_lib::proto::tunnel::{TransportProtocol, TunnelEncryption};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Data already exist")]
    AlreadyExist,
    #[error("Data is not supported by underlying database")]
    NotSupported,
    #[error("Internal error")]
    InternalError,
    #[error("Data not found")]
    NotFound,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Failed to serialize value: {0}")]
    DeserializationError(String),
    #[error("Failed to serialize value: {0}")]
    SerializationError(String),
}

pub type DatabaseResult<T> = std::result::Result<T, DatabaseError>;

fn generate_hashed_password(password: &str) -> (String, String) {
    let mut hasher = Sha512::new();
    let salt = uuid::Uuid::new_v4().to_string();
    hasher.update(salt.as_bytes());
    hasher.update(password.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    (salt, hash)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub salt: String,
    pub password: String,
    pub room_ids: Vec<Uuid>,
}

impl User {
    pub fn new_for_credentials(username: &str, password: &str) -> Self {
        let (salt, password) = generate_hashed_password(password);

        Self {
            id: uuid::Uuid::new_v4(),
            username: username.to_owned(),
            password,
            salt,
            display_name: None,
            room_ids: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelLink {
    id: Uuid,
    name: String,
    protocol: TransportProtocol,
    encryption: TunnelEncryption,
    max_connections: Option<NonZeroUsize>,
    owner: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Room {
    id: Uuid,
    users: Vec<Uuid>,
    tunnel_links: Vec<TunnelLink>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkStatus {
    id: Uuid,
    host_ready: bool,
    connected_users: Vec<Uuid>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Database {
    username_to_id_mapping: HashMap<String, Uuid>,
    users: HashMap<Uuid, User>,
    rooms: HashMap<Uuid, Room>,
    link_statuses: HashMap<Uuid, LinkStatus>,

    #[serde(skip)]
    changed: AtomicBool,

    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Database {
    pub async fn open(path: &Path) -> DatabaseResult<Self> {
        if !path.exists() {
            return Ok(Default::default());
        }

        let mut file = File::open(path).await?;
        let mut data = Vec::with_capacity(file.metadata().await?.len() as usize);
        file.read_to_end(&mut data).await?;

        let parsed = serde_json::from_slice(&data)
            .map_err(|e| DatabaseError::DeserializationError(e.to_string()))?;

        Ok(parsed)
    }

    pub async fn flush_changes(&self) -> DatabaseResult<()> {
        if !self.changed.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(path) = &self.path {
            let data = serde_json::to_vec(self)
                .map_err(|e| DatabaseError::SerializationError(e.to_string()))?;

            let mut file = File::create(path).await?;
            file.write_all(&data).await?;
        }

        self.changed.store(false, Ordering::SeqCst);

        Ok(())
    }

    pub async fn find_user_id_by_username(&mut self, username: &str) -> DatabaseResult<Uuid> {
        self.username_to_id_mapping
            .get(username)
            .cloned()
            .ok_or(DatabaseError::NotFound)
    }
    pub async fn fetch_user(&mut self, id: &Uuid) -> DatabaseResult<User> {
        self.users.get(id).cloned().ok_or(DatabaseError::NotFound)
    }

    pub async fn create_user(&mut self, user: &User) -> DatabaseResult<()> {
        if self.users.contains_key(&user.id) {
            return Err(DatabaseError::AlreadyExist);
        }

        self.users.insert(user.id, user.clone());
        self.username_to_id_mapping
            .insert(user.username.clone(), user.id);
        Ok(())
    }

    pub async fn store_user(&mut self, user: &User) -> DatabaseResult<()> {
        if !self.users.contains_key(&user.id) {
            return Err(DatabaseError::NotFound);
        }

        self.users.insert(user.id, user.clone());
        Ok(())
    }

    pub async fn fetch_room(&mut self, id: &Uuid) -> DatabaseResult<Room> {
        self.rooms.get(id).cloned().ok_or(DatabaseError::NotFound)
    }

    pub async fn create_room(&mut self, room: &Room) -> DatabaseResult<()> {
        if self.rooms.contains_key(&room.id) {
            return Err(DatabaseError::AlreadyExist);
        }

        self.rooms.insert(room.id, room.clone());
        Ok(())
    }

    pub async fn store_room(&mut self, room: &Room) -> DatabaseResult<()> {
        if !self.rooms.contains_key(&room.id) {
            return Err(DatabaseError::NotFound);
        }

        self.rooms.insert(room.id, room.clone());
        Ok(())
    }

    pub async fn fetch_link_status(&mut self, id: &Uuid) -> DatabaseResult<LinkStatus> {
        self.link_statuses
            .get(id)
            .cloned()
            .ok_or(DatabaseError::NotFound)
    }

    pub async fn create_link_status(&mut self, link_status: &LinkStatus) -> DatabaseResult<()> {
        if self.link_statuses.contains_key(&link_status.id) {
            return Err(DatabaseError::AlreadyExist);
        }

        self.link_statuses
            .insert(link_status.id, link_status.clone());
        Ok(())
    }

    pub async fn store_link_status(&mut self, link_status: &LinkStatus) -> DatabaseResult<()> {
        if !self.link_statuses.contains_key(&link_status.id) {
            return Err(DatabaseError::NotFound);
        }

        self.link_statuses
            .insert(link_status.id, link_status.clone());
        Ok(())
    }
}

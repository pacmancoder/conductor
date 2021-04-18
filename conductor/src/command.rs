use crate::net::{PortNum, Protocol};
use std::path::PathBuf;

pub struct CreateCommand {
    pub port: PortNum,
    pub client_port: Option<PortNum>,
    pub protocol: Protocol,
    pub server: String,
    pub key: PathBuf,
    pub file_name: PathBuf,
}

pub struct ServeCommand {
    pub port: PortNum,
    pub public_key: PathBuf,
}
pub struct ConnectCommand {
    pub config: PathBuf,
}

pub struct ListenCommand {
    pub config: PathBuf,
}

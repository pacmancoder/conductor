use crate::net::{PortNum, Protocol};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ConnectionConfig {
    port: PortNum,
    client_port: Option<PortNum>,
    protocol: Protocol,
    server: String,
    key: String,
    id: uuid::Uuid,
}

use enum_primitive_derive::Primitive;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransportProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TunnelEncryption {
    // No encryption
    None,
    // Point-to-point encryption between peers; Use it when conductor server instance can't be
    // trusted (for example, for globally-accessible conductor.overengineering.xyz instance)
    Tls,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ConnectionType {
    Client,
    Host,
}

#[derive(Primitive, Eq, PartialEq)]
pub enum ResultCode {
    Ok = 0x0000,
    UnknownFailure = 0xFFFF,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTunnelJwtClaims {
    // Expiration timestamp
    exp: i64,
    // Issued at timestamp
    iat: i64,
    // Not before timestamp
    nbf: i64,

    // Conductor server hostname
    iss: String,
    // Peer id
    sub: Uuid,
    // Tunnel ID
    tid: Uuid,
    ct: ConnectionType,
}

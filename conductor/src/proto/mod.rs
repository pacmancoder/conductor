//! # General protocol description
//! 1. Client sends Initiation request (unencrypted)
//!     - Request contains protocol version for initiation
//! 1. Server sends its public key in response (unencrypted)
//!     - Client checks server key against footprint in config (requects if not equal)
//! 1. Client sends encrypted request for tunnel (encrypted via server RSA)
//!     - Tunnel info (id, role, channel_id)
//!     - Client public key (for challenge)
//!     - On this step server may reject cient based on client key footprint
//! 1. Server sends challenge for client to ensure client owns the key (encrypted via client key)
//!     - 64 bytes of random data
//! 1. Client sends decrypted challenge (encrypted via server RSA)
//!     - 64 bytes decrypted via client key
//! 1. Server sends Success message or error if challenge was failed (encrypted via client key)
mod protocol_revision;

pub use protocol_revision::ProtocolRevision;

use crate::{encoding::Base64EncodedData, net::TunnelTransport};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
enum PeerInfo {
    ClientStream {
        connection_id: Uuid,
        channel_id: Uuid,
    },
    ServerStream {
        connection_id: Uuid,
        channel_id: Uuid,
    },
    ServerListener {
        connection_id: Uuid,
    },
}

#[derive(Serialize, Deserialize)]
struct PeerJwt {
    peer: PeerInfo,
}

#[derive(Serialize, Deserialize)]
pub enum ServerFailure {
    InternalError,
    VersionRejected,
    SignatureRejected,
    ClientKeyRejected,
    ClientChallengeFailed,
    AccessDenied,
    Conflict,
}

#[derive(Serialize, Deserialize)]
pub enum PeerFailure {
    InternalError,
    SignatureRejected,
    ServerKeyRejected,
}

#[derive(Serialize, Deserialize)]
pub enum TunnelRole {
    Listen,
    Accept,
    Connect,
}

#[derive(Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub role: TunnelRole,
    pub client_key: Base64EncodedData,
}

#[derive(Serialize, Deserialize)]
pub enum PeerMessage {
    InitiateSession { protocol_revision: ProtocolRevision },
    CreateSession(CreateSessionRequest),
    ClientChallengeDone { data: Base64EncodedData },
    Failure(PeerFailure),
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    InitiateSessionAccepted {
        server_name: String,
        server_key: Base64EncodedData,
    },
    ClientChallenge {
        data: Base64EncodedData,
    },
    TransportReady {
        tunnel_protocol: TunnelTransport,
    },
    Failure(ServerFailure),
}

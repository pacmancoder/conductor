//! # General protocol description
//! 1. Client sends Initiation request (signed but signature is not checked)
//!     - Request holds protocol version for initiation
//! 1. Server sends its public key in response (signed but signature is not chekced)
//!     - On this step client may reject server
//! 1. Client sends encrypted request for tunnel (encrypted via server RSA)
//!     - Tunnel info (id, role)
//!     - Client public key (for challenge)
//!     - On this step server may reject cient
//! 1. Server sends challenge for client to ensure client owns the key (signed, validated)
//!     - 64 bytes of random data encrypted via RSA
//! 1. Client sends unencrypted challenge (encrypted via server RSA)
//!     - 64 bytes decrypted via client key
//! 1. Server sends Success message or error if challenge was failed (signed, validated)
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
}

#[derive(Serialize, Deserialize)]
pub enum PeerFailure {
    InternalError,
    SignatureRejected,
    ServerKeyRejected,
}

#[derive(Serialize, Deserialize)]
pub enum TunnelRole {
    Host,
    Client,
}

#[derive(Serialize, Deserialize)]
pub enum PeerMessage {
    InitiateSession {
        version: ProtocolRevision,
    },
    CreateSession {
        id: Uuid,
        role: TunnelRole,
        client_key: Base64EncodedData,
    },
    ClientChallengeDone {
        data: Base64EncodedData,
    },
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

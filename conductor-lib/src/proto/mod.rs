//! Conductor protocol
//!
//! # Header (bits table)
//! |  7  |  6  |  5  |  4  |  3  |  2  |  1  |  0  |
//! | --- | --- | --- | --- | --- | --- | --- | --- |
//! | ccm | cdm | ced |  0  |  0  |  0  |  0  |  0  |
//!
//! - ccm: CONTROL_CHANNEL_MESSAGE. Set to one when next message is CCM message
//! - cdm: CHANNEL_DATA_MESSAGE. Set to one when next message is CDM message
//! - ced: CONTROL_ENCRYPTION_DISABLED. Disables TLS on Control channel. Should be allowed only
//!        if already using safe transport for the whole server connection (e.g: wss)
//! Invalid cases:
//! - ccm & cdm can't be set simultaneously.
//!
//! CCM Packet data layout (bytes)
//!
//! | **position** | buffer[0]  | buffer[1]...buffer[4] | buffer[5]...buffer[6]  | buffer[7]  |
//! | ------------ | ---------- | --------------------- | ---------------------- | ---------- |
//! | **example**  | 0b10000000 |  0xDEADBEAF           | 0xCAFE                 | 0x00       |
//! | **purpose**  | header     |  channel_id           | payload_size           | reserved   |
//! | **type**     | u8         |  u32, LE              | u16, LE                | u8         |
//!
//! CCM Packet data layout (bytes)
//!
//! | **position** | buffer[0]  | buffer[1]...buffer[4] | buffer[5]...buffer[6]  | buffer[7]  |
//! | ------------ | ---------- | --------------------- | ---------------------- | ---------- |
//! | **example**  | 0b10000000 |  0xDEADBEAF           | 0xCAFE                 | 0x00       |
//! | **purpose**  | header     |  channel_id           | payload_size           | reserved   |
//! | **type**     | u8         |  u32, LE              | u16, LE                | u8         |

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// TODO: Tokio Decoder/Encoder

bitflags! {
    pub struct ChannelPacketFlags: u8 {
        const CONTROL_CHANNEL_MESSAGE = 0b10000000;
        const CHANNEL_DATA_MESSAGE = 0b01000000;
        const CONTROL_ENCRYPTION_DISABLED = 0b00100000;
    }
}

/// Unique channel id between peer and conductor-server instance
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct DataChannelId(u32);

/// Unique Client <-> Host tunnel identifier
type TunnelId = Uuid;

/// Auth JWT token
type AuthToken = String;

/// Request sent by a peer
#[derive(Serialize, Deserialize)]
enum PeerControlRequest {
    /// Should be sent as the first message by the peer. Should contain machine auth token (JWT)
    /// generated
    AuthPeer(AuthToken),
    /// Server keeps channels alive for some time after peer disconnects, so if peer's network
    /// connection fails, it can resume it after authenticating again
    ResumePreviousSession,

    /// Sent by peer if it wants to upgrade data stream of the channel to TLS after the next packet.
    /// If server rejects this request, both peer and server SHOULD close channel.
    UpgradeToTls(DataChannelId),
    /// By default channels starts with TLS enabled to transfer auth token and configure channel
    /// securely. However, later it can be downgraded by the peer.
    DowngradeToTcp(DataChannelId),

    /// Sent by the "server" peer which will expose its resources over the channel
    EstablishHostChannel(TunnelId),
    /// Sent by the "client" peer which will expose its resources over the channel
    EstablishClientChannel(TunnelId),
    CloseChannel(DataChannelId),

    /// Sent by the client regularly to keep connection with the server
    KeepAlive,
}

#[derive(Serialize, Deserialize)]
enum ServerControlResponseData {
    /// Returned for  EstablishHost & EstablishClient requests
    ChannelCreated(DataChannelId),
}

// Returned by server as a response for PeerControlRequest
type ServerControlResponse = Result<Option<ServerControlResponseData>, String>;

/// Request sent by a server to the peer
#[derive(Serialize, Deserialize)]
enum ServerControlRequest {
    CloseChannel(DataChannelId),
}

// Returned by peer as a response for ServerControlRequest
type PeerControlResponse = Result<(), String>;

#[derive(Serialize, Deserialize)]
enum PeerControlMessage {
    PeerRequest(PeerControlRequest),
    PeerResponse(PeerControlResponse),
}

enum PeerChannelMessage {
    Control(PeerControlMessage),
    Data {
        channel: DataChannelId,
        data: Vec<u8>,
    },
}

#[derive(Serialize, Deserialize)]
enum ServerControlMessage {
    ServerRequest(ServerControlRequest),
    ServerResponse(ServerControlResponse),
}

enum ServerChannelMessage {
    Control(ServerControlMessage),
    Data {
        channel: DataChannelId,
        data: Vec<u8>,
    },
}

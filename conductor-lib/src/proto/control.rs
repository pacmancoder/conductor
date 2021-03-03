use crate::proto::{AuthToken, DataChannelId, TunnelId};
use serde::{Deserialize, Serialize};

/// Request sent by a peer
#[derive(Serialize, Deserialize)]
pub enum PeerControlRequest {
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
pub enum ServerControlResponseData {
    /// Returned for  EstablishHost & EstablishClient requests
    ChannelCreated(DataChannelId),
}

// Returned by server as a response for PeerControlRequest
pub type ServerControlResponse = Result<Option<ServerControlResponseData>, String>;

/// Request sent by a server to the peer
#[derive(Serialize, Deserialize)]
pub enum ServerControlRequest {
    CloseChannel(DataChannelId),
}

// Returned by peer as a response for ServerControlRequest
pub type PeerControlResponse = Result<(), String>;

#[derive(Serialize, Deserialize)]
pub enum PeerControlMessage {
    PeerRequest(PeerControlRequest),
    PeerResponse(PeerControlResponse),
}

#[derive(Serialize, Deserialize)]
pub enum ServerControlMessage {
    ServerRequest(ServerControlRequest),
    ServerResponse(ServerControlResponse),
}

use crate::{
    auth::io::{receive_encrypted_jwt, receive_serialized, send_encrypted_jwt, send_serialized},
    crypto::keys::{KeyPair, PublicKey},
    net::TunnelTransport,
    proto::{CreateSessionRequest, PeerMessage, ProtocolRevision, ServerMessage, TunnelRole},
    Error, Result,
};
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

pub struct PeerAuth<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub stream: T,
    pub connection_uuid: uuid::Uuid,
    pub role: TunnelRole,
    pub server_key_fingerprint: String,
    pub peer_keys: KeyPair,
}

pub struct PeerAuthResult<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    stream: T,
    transport: TunnelTransport,
}

impl<T> PeerAuth<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn perform(mut self) -> Result<PeerAuthResult<T>> {
        log::info!("Initiating new session...");

        let peer_message = PeerMessage::InitiateSession {
            protocol_revision: ProtocolRevision::R0,
        };

        send_serialized(&mut self.stream, peer_message)
            .await
            .map_err(|e| {
                log::error!("Failed to initiate session");
                e
            })?;

        let server_message: ServerMessage =
            receive_serialized(&mut self.stream).await.map_err(|e| {
                log::error!("Failed to receive session initiation response");
                e
            })?;

        let (server_key, server_name) = match server_message {
            ServerMessage::InitiateSessionAccepted {
                server_key,
                server_name,
            } => (server_key, server_name),
            _ => {
                log::error!("Server retuned invalid session initiation request");
                return Err(Error::AuthSequenceFailed);
            }
        };

        log::info!("Perforing auth with \"{}\"...", server_name);

        let server_key = PublicKey::parse_pkcs8(server_key.as_ref()).map_err(|e| {
            log::error!("Server returned corrupted public key");
            e
        })?;

        let server_key_fingerprint = server_key.fingerprint();

        if server_key_fingerprint != self.server_key_fingerprint {
            log::info!(
                "Server returned invalid fingerprint.\n\tExpected: {}\n\tActual: {}",
                self.server_key_fingerprint,
                server_key_fingerprint
            );
            return Err(Error::InvalidServerKey);
        }

        let client_key = AsRef::<[u8]>::as_ref(&self.peer_keys.public).into();

        let peer_message = PeerMessage::CreateSession(CreateSessionRequest {
            id: self.connection_uuid,
            client_key,
            channel_id: Uuid::new_v4(),
            role: self.role,
        });

        send_encrypted_jwt(&mut self.stream, peer_message, server_key.as_ref())
            .await
            .map_err(|e| {
                log::error!("Failed to send create session request");
                e
            })?;

        let server_message: ServerMessage =
            receive_encrypted_jwt(&mut self.stream, self.peer_keys.private.as_ref())
                .await
                .map_err(|e| {
                    log::error!("Server rejected session request");
                    e
                })?;

        let peer_message = match server_message {
            ServerMessage::ClientChallenge { data } => PeerMessage::ClientChallengeDone { data },
            _ => {
                log::error!("Server returned invalid client challenge response");
                return Err(Error::AuthSequenceFailed);
            }
        };

        log::info!("Server accepted peer, processing received challenge...");

        send_encrypted_jwt(&mut self.stream, peer_message, server_key.as_ref())
            .await
            .map_err(|e| {
                log::error!("Failed to send peer challenge response");
                e
            })?;

        let server_message: ServerMessage =
            receive_encrypted_jwt(&mut self.stream, self.peer_keys.private.as_ref())
                .await
                .map_err(|e| {
                    log::error!("Failed to receive tunnel creation status response");
                    e
                })?;

        let transport = match server_message {
            ServerMessage::TransportReady { tunnel_protocol } => tunnel_protocol,
            _ => {
                log::error!("Server returned invalid tunnel creation status response");
                return Err(Error::AuthSequenceFailed);
            }
        };

        log::info!("Tunnel has been established");

        Ok(PeerAuthResult {
            stream: self.stream,
            transport,
        })
    }
}

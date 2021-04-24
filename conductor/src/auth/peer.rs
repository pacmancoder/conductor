use crate::{
    auth::io::{receive_encrypted_jwt, receive_serialized, send_encrypted_jwt, send_serialized},
    crypto::keys::{KeyPair, PublicKey},
    net::TunnelTransport,
    proto::{CreateSessionRequest, PeerMessage, ProtocolRevision, ServerMessage, TunnelRole},
    Error, Result,
};
use tokio::net::TcpStream;
use uuid::Uuid;

pub struct PeerAuth {
    pub stream: TcpStream,
    pub connection_uuid: uuid::Uuid,
    pub role: TunnelRole,
    pub server_key_fingerprint: String,
    pub peer_keys: KeyPair,
}

pub struct PeerAuthResult {
    stream: TcpStream,
    transport: TunnelTransport,
}

impl PeerAuth {
    pub async fn perform(mut self) -> Result<PeerAuthResult> {
        // TODO: improve error handling
        let peer_message = PeerMessage::InitiateSession {
            protocol_revision: ProtocolRevision::R0,
        };

        send_serialized(&mut self.stream, peer_message)
            .await
            .map_err(|_| Error::from_application_error("Failed to send session initiation"))?;

        let server_message: ServerMessage = receive_serialized(&mut self.stream)
            .await
            .map_err(|_| Error::from_application_error("Session initiation failed"))?;

        let (server_key, _server_name) = match server_message {
            ServerMessage::InitiateSessionAccepted {
                server_key,
                server_name,
            } => (server_key, server_name),
            _ => {
                return Err(Error::from_application_error(
                    "Server returned invalid initiation response",
                ));
            }
        };

        let server_key = PublicKey::parse_pkcs8(server_key.as_ref())?;

        if server_key.fingerprint() != self.server_key_fingerprint {
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
            .map(|_| Error::from_application_error("Failed to send create session request"))?;

        let server_message: ServerMessage =
            receive_encrypted_jwt(&mut self.stream, self.peer_keys.private.as_ref())
                .await
                .map_err(|_| Error::from_application_error("Session creation failed"))?;

        let peer_message = match server_message {
            ServerMessage::ClientChallenge { data } => {
                PeerMessage::ClientChallengeDone { data }
            },
            _ => {
                return Err(Error::from_application_error(
                    "Server returned invalid initiation response",
                ));
            }
        };


        send_encrypted_jwt(&mut self.stream, peer_message, server_key.as_ref())
            .await
            .map(|_| Error::from_application_error("Failed to send peer challenge"))?;

        let server_message: ServerMessage =
            receive_encrypted_jwt(&mut self.stream, self.peer_keys.private.as_ref())
                .await
                .map_err(|_| Error::from_application_error("Session creation failed"))?;

        let transport = match server_message {
            ServerMessage::TransportReady { tunnel_protocol } => tunnel_protocol,
            _ => {
                return Err(Error::from_application_error("Server rejected peer"));
            }
        };

        Ok(PeerAuthResult {
            stream: self.stream,
            transport,
        })
    }
}

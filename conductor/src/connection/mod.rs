mod config;

pub use config::ConnectionConfig;

use std::collections::HashMap;
use tokio::{net::TcpStream, sync::mpsc};
use uuid::Uuid;

struct ConnectionRecord {
    channel_request_tx: mpsc::Sender<Uuid>,
    channel_responses: HashMap<Uuid, mpsc::Sender<TcpStream>>,
}

#[derive(Default)]
struct ConnectionRegistry {
    connections: HashMap<Uuid, ConnectionRecord>,
}

struct ConnectionListeningEnd {
    channel_request_rx: mpsc::Receiver<Uuid>,
}

struct HostConnectionEnd {
    channel_response_tx: mpsc::Sender<TcpStream>,
}

struct ClientConnectionEnd {
    channel_request_tx: mpsc::Sender<Uuid>,
    channel_response_rx: mpsc::Receiver<TcpStream>,
}

impl ConnectionRegistry {
    pub fn listen_connection(&mut self, connection_id: Uuid) -> Option<ConnectionListeningEnd> {
        if self.connections.contains_key(&connection_id) {
            return None;
        }

        let (tx, rx) = mpsc::channel(32);

        let connection = ConnectionRecord {
            channel_request_tx: tx,
            channel_responses: HashMap::new(),
        };

        self.connections.insert(connection_id, connection);

        Some(ConnectionListeningEnd {
            channel_request_rx: rx,
        })
    }

    pub fn free_connection(&mut self, connection_id: Uuid) {
        self.connections.remove(&connection_id);
    }

    pub fn connect_host(
        &mut self,
        connection_id: Uuid,
        channel_id: Uuid,
    ) -> Option<HostConnectionEnd> {
        let connection = self.connections.get(&connection_id)?;
        let channel = connection.channel_responses.get(&channel_id)?;

        Some(HostConnectionEnd {
            channel_response_tx: channel.clone(),
        })
    }

    pub fn connect_client(
        &mut self,
        connection_id: Uuid,
        channel_id: Uuid,
    ) -> Option<ClientConnectionEnd> {
        let connection = self.connections.get_mut(&connection_id)?;

        let channel_request_tx = connection.channel_request_tx.clone();

        let (tx, rx) = mpsc::channel(32);

        connection.channel_responses.insert(channel_id, tx);

        Some(ClientConnectionEnd {
            channel_request_tx,
            channel_response_rx: rx,
        })
    }
}

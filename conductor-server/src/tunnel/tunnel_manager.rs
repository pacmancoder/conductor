use conductor_lib::proto::tunnel::{TransportProtocol, TunnelEncryption};
use std::collections::HashMap;

use std::sync::{atomic::AtomicUsize, Arc};
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TransportLinkError {
    #[error("Link already exist")]
    LinkAlreadyExist,
}

// 1. Host will wait on heb hook for requests (receives tunnel info claims)
// 2. When client connects, tunnel manager will send to peer request for connection and save
//    the provided sender to respond when peer respond back
// 3. client connection receives transport or error and responds back

struct TransportLink {
    // Underlying transport link protocol
    protocol: TransportProtocol,
    // End-to-end encryption type
    encryption: TunnelEncryption,

    // Bytes transferred from host to client peer (for statistics)
    bytes_tx: Arc<AtomicUsize>,
    // Bytes transferred from client to server peer (for statistics)
    bytes_rx: Arc<AtomicUsize>,
}

impl TransportLink {
    fn new(protocol: TransportProtocol, encryption: TunnelEncryption) -> Self {
        Self {
            protocol,
            encryption,
            bytes_rx: Arc::new(AtomicUsize::new(0)),
            bytes_tx: Arc::new(AtomicUsize::new(0)),
        }
    }
}

type TransportLinkMap = HashMap<Uuid, TransportLink>; // tunnel id => tunnel
type UserTransportLinks = HashMap<Uuid, TransportLinkMap>; // user id => TransportLinkMap

struct LinkDropHandle {
    manager: Arc<TransportLinkManager>,
    user: Uuid,
    id: Uuid,
}

impl LinkDropHandle {
    async fn drop(self) {
        let mut user_links = self.manager.user_transport_links.lock().await;

        if let Some(links) = user_links.get_mut(&self.user) {
            links.remove(&self.id);
        }
    }
}

impl Drop for LinkDropHandle {
    fn drop(&mut self) {
        assert!(false, "LinkDropHandle should be dropped manually!");
    }
}

struct TransportLinkManager {
    user_transport_links: Mutex<UserTransportLinks>,
}

impl TransportLinkManager {
    fn new() -> Arc<Self> {
        let this = Self {
            user_transport_links: Mutex::new(HashMap::new()),
        };

        Arc::new(this)
    }

    async fn register_link(
        self: Arc<Self>,
        user: Uuid,
        id: Uuid,
        link: TransportLink,
    ) -> Result<(), TransportLinkError> {
        let mut user_links = self.user_transport_links.lock().await;

        if !user_links.contains_key(&user) {
            user_links.insert(user, TransportLinkMap::new());
        }

        let links = user_links.get_mut(&user).unwrap();

        if links.contains_key(&id) {
            return Err(TransportLinkError::LinkAlreadyExist);
        }

        links.insert(id, link);

        Ok(())
    }
}

use crate::{db::Database, rest::SyncDatabase};
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

const DEFAULT_REST_API_PORT: u16 = 1613;

pub struct RestServer {
    port: u16,
}

impl Default for RestServer {
    fn default() -> Self {
        RestServer {
            port: DEFAULT_REST_API_PORT,
        }
    }
}

impl RestServer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    pub async fn run(self) {
        use crate::rest::{auth, health, tunnel};

        let mut config = rocket::Config::default();
        config.port = self.port;
        config.ctrlc = false;

        let database = Database::open(Path::new("conductor.db"))
            .await
            .expect("Failed to open database");

        let routes = Vec::new()
            .into_iter()
            .chain(auth::routes())
            .chain(health::routes())
            .chain(tunnel::routes())
            .collect::<Vec<_>>();

        rocket::custom(config)
            .manage(Arc::new(Mutex::new(database)))
            .mount("/api/v1", routes)
            .launch()
            .await
            .expect("REST API server failed");
    }
}

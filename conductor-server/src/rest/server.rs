
const DEFAULT_REST_API_PORT: u16 = 1613;

#[rocket::get("/health")]
async fn get_health() -> &'static str {
    "I am alive and good!"
}

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
        let mut config = rocket::Config::default();
        config.port = self.port;
        config.ctrlc = false;

        rocket::custom(config)
            .mount("/api/v1", rocket::routes![get_health])
            .launch().await
            .expect("REST API server failed");
    }
}
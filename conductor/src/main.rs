use anyhow::Context;
use clap::Clap;
use futures::FutureExt;
use picky::jose::{
    jwt::{JwtSig, JwtValidator},
    jws::JwsAlg,
};
use picky::key::{PublicKey, PrivateKey};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
struct PortNum(u16);

impl FromStr for PortNum {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u16>()
            .with_context(|| format!("Invalid port number '{}'", s))
            .map(|p| Self(p))
    }
}

#[derive(Clap)]
#[clap(version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"))]
#[clap(setting = clap::AppSettings::ColoredHelp)]
struct App {
    #[clap(long, default_value = "info", possible_values = &["error", "warn", "info", "debug", "trace"])]
    log_level: log::LevelFilter,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Clap)]
enum Command {
    New(NewCommand),
    Serve(ServeCommand),
    Connect(ConnectCommand),
    Listen(ListenCommand),
}

#[derive(Clap)]
struct ServeCommand {
    #[clap(long)]
    port: PortNum,
    #[clap(long)]
    public_key: PathBuf,
}

#[derive(Clap)]
struct ConnectCommand {
    config: PathBuf,
}


#[derive(Clap)]
struct ListenCommand {
    config: PathBuf,
}

#[derive(Serialize, Deserialize)]
enum Protocol {
    Tcp,
    Udp,
}

impl FromStr for Protocol {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            protocol @ _ => Err(anyhow::anyhow!("Invalid protocol '{}'", protocol)),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Config {
    port: PortNum,
    client_port: Option<PortNum>,
    protocol: Protocol,
    server: String,
    key: String,
    id: uuid::Uuid,
}

#[derive(Clap)]
struct NewCommand {
    #[clap(long, short)]
    port: PortNum,
    #[clap(long, short)]
    client_port: Option<PortNum>,
    #[clap(long, default_value = "tcp", possible_values = &["tcp", "udp"])]
    protocol: Protocol,
    #[clap(long, short)]
    server: String,
    #[clap(long, short)]
    key: PathBuf,

    file_name: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = App::parse();

    env_logger::builder()
        .filter_level(args.log_level)
        .try_init().expect("Failed to init logger");

    match args.command {
        Command::New(cmd) => new_command(cmd).await,
        Command::Serve(cmd) => serve_command(cmd).await,
        Command::Connect(cmd) => connect_command(cmd).await,
        Command::Listen(cmd) => listen_command(cmd).await,
    }
}

async fn new_command(cmd: NewCommand) -> Result<(), anyhow::Error> {
    let mut key = Vec::new();

    File::open(cmd.key)
        .await
        .with_context(|| "Failed to open key file")?
        .read_to_end(&mut key)
        .await
        .with_context(|| "Failed to read key file")?;

    picky::pem::parse_pem(&key).with_context(|| "Invalid pem key file")?;

    let config = Config {
        port: cmd.port,
        client_port: cmd.client_port,
        protocol: cmd.protocol,
        server: cmd.server,
        key: base64::encode(key),
        id: uuid::Uuid::new_v4(),
    };

    let serialized_config =
        toml::to_string(&config).with_context(|| "Failed to serialize config")?;

    let mut config_file = File::create(cmd.file_name)
        .await
        .with_context(|| "Failed to create config file")?;

    config_file
        .write_all(serialized_config.as_bytes())
        .await
        .with_context(|| "Failed to write config file")?;

    config_file
        .flush()
        .await
        .with_context(|| "Failed to flush config file")?;

    Ok(())
}

async fn serve_command(cmd: ServeCommand) -> Result<(), anyhow::Error> {
    let mut key = Vec::new();
    File::open(cmd.public_key)
        .await
        .with_context(|| "Failed to open key file")?
        .read_to_end(&mut key)
        .await
        .with_context(|| "Failed to read key file")?;

    let pem = picky::pem::parse_pem(&key).with_context(|| "Invalid pem key file")?;

    let key = PublicKey::from_pem(&pem).with_context(|| "Invalid public key")?;

    let shared_key = Arc::new(key);
    let shared_connections = Arc::new(Mutex::new(Connections::default()));

    let port = cmd.port.0;

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .with_context(|| format!("Failed to bind port {}", port))?;

    log::info!("Starting server...");

    loop {
        match listener.accept().await {
            Ok((stream, socket)) => {
                log::info!("Incomming peer {}", socket);
                let key = shared_key.clone();
                let connections = shared_connections.clone();
                tokio::spawn(async move {
                    match process_peer(key, connections, stream).await {
                        Ok(()) => log::info!("Client exited"),
                        Err(e) => log::error!("Client failed: {}", e),
                    };
                });
                log::info!("Accepted peer {}", socket);
            }
            Err(e) => {
                log::error!("Peer connection failed: {}", e);
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize)]
enum PeerInfo {
    ClientStream {
        connection_id: uuid::Uuid,
        channel_id: uuid::Uuid,
    },
    ServerStream {
        connection_id: uuid::Uuid,
        channel_id: uuid::Uuid,
    },
    ServerListener {
        connection_id: uuid::Uuid,
    },
}

#[derive(Serialize, Deserialize)]
struct PeerJwt {
    peer: PeerInfo,
}

struct Connection {
    channel_request_tx: tokio::sync::mpsc::Sender<uuid::Uuid>,
    channel_responses: HashMap<uuid::Uuid, tokio::sync::mpsc::Sender<TcpStream>>,
}

#[derive(Default)]
struct Connections {
    connections: HashMap<uuid::Uuid, Connection>,
}

struct ConnectionListeningEnd {
    channel_request_rx: tokio::sync::mpsc::Receiver<uuid::Uuid>,
}

struct HostConnectionEnd {
    channel_response_tx: tokio::sync::mpsc::Sender<TcpStream>,
}

struct ClientConnectionEnd {
    channel_request_tx: tokio::sync::mpsc::Sender<uuid::Uuid>,
    channel_response_rx: tokio::sync::mpsc::Receiver<TcpStream>,
}

impl Connections {
    pub fn listen_connection(
        &mut self,
        connection_id: uuid::Uuid,
    ) -> Option<ConnectionListeningEnd> {
        if self.connections.contains_key(&connection_id) {
            return None;
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        let connection = Connection {
            channel_request_tx: tx,
            channel_responses: HashMap::new(),
        };

        self.connections.insert(connection_id, connection);

        Some(ConnectionListeningEnd {
            channel_request_rx: rx,
        })
    }

    pub fn free_connection(&mut self, connection_id: uuid::Uuid) {
        self.connections.remove(&connection_id);
    }

    pub fn connect_host(
        &mut self,
        connection_id: uuid::Uuid,
        channel_id: uuid::Uuid,
    ) -> Option<HostConnectionEnd> {
        let connection = self.connections.get(&connection_id)?;
        let channel = connection.channel_responses.get(&channel_id)?;

        Some(HostConnectionEnd {
            channel_response_tx: channel.clone(),
        })
    }

    pub fn connect_client(
        &mut self,
        connection_id: uuid::Uuid,
        channel_id: uuid::Uuid,
    ) -> Option<ClientConnectionEnd> {
        let connection = self.connections.get_mut(&connection_id)?;

        let channel_request_tx = connection.channel_request_tx.clone();

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        connection.channel_responses.insert(channel_id, tx);

        Some(ClientConnectionEnd {
            channel_request_tx,
            channel_response_rx: rx,
        })
    }
}

async fn forward_reader_to_writer<R, W>(mut reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0; 4096];

    while let Some(count) = NonZeroUsize::new(reader.read(&mut buf).await?) {
        writer.write_all(&buf[0..count.get()]).await?;
    }

    Ok(())
}

async fn forward_socket(client: TcpStream, server: TcpStream) -> Result<(), anyhow::Error> {
    let (client_reader, client_writer) = tokio::io::split(client);
    let (server_reader, server_writer) = tokio::io::split(server);

    let client_to_server = forward_reader_to_writer(client_reader, server_writer).fuse();
    let server_to_client = forward_reader_to_writer(server_reader, client_writer).fuse();

    log::info!("Forwarding started...");
    tokio::select! {
        _ = client_to_server => {},
        _ = server_to_client => {},
    }
    Ok(())
}

async fn process_peer(
    key: Arc<PublicKey>,
    connections: Arc<Mutex<Connections>>,
    mut stream: TcpStream,
) -> Result<(), anyhow::Error> {
    // TODO: timeouts
    let header_size = stream
        .read_u32()
        .await
        .with_context(|| "Failed to read client header size")?;

    let mut header_buffer = vec![0u8; header_size as usize];

    stream
        .read_exact(&mut header_buffer)
        .await
        .with_context(|| "Failed to read client header content")?;

    let jwt_string =
        String::from_utf8(header_buffer).with_context(|| "Peer header is not a string")?;

    let jwt_claims: JwtSig<PeerJwt> = JwtSig::decode(&jwt_string, &key, &JwtValidator::no_check())
        .with_context(|| "Invalid Jwt token")?;

    match jwt_claims.claims.peer {
        PeerInfo::ClientStream {
            connection_id,
            channel_id,
        } => {
            log::info!("Accepting client stream peer...");
            let ClientConnectionEnd {
                channel_request_tx,
                mut channel_response_rx,
            } = {
                let mut connections = connections.lock().await;
                connections
                    .connect_client(connection_id, channel_id)
                    .with_context(|| "Connection does not exist")?
            };

            log::info!("Sending connect request over channel...");
            channel_request_tx.send(channel_id).await;
            let host_stream = channel_response_rx
                .recv()
                .await
                .with_context(|| "Failed to receive response from host")?;

            log::info!("Got host stream! starting forwarding...");

            stream.write_u32(0x80000000).await
                .with_context(|| "Failed to write result")?;

            forward_socket(stream, host_stream).await?;
        }
        PeerInfo::ServerStream {
            connection_id,
            channel_id,
        } => {
            log::info!("Accepting server stream peer...");
            let HostConnectionEnd {
                channel_response_tx,
            } = {
                let mut connections = connections.lock().await;
                connections
                    .connect_host(connection_id, channel_id)
                    .with_context(|| "Connection does not exist (Host)")?
            };

            log::info!("Sending server stream to the client...");

            stream.write_u32(0x80000000).await
                .with_context(|| "Failed to write result")?;

            channel_response_tx.send(stream).await;
        }
        PeerInfo::ServerListener { connection_id } => {
            log::info!("Accepting server listener peer...");
            let ConnectionListeningEnd {
                mut channel_request_rx,
            } = {
                let mut connections = connections.lock().await;
                connections
                    .listen_connection(connection_id.clone())
                    .with_context(|| "Connection already in listening mode")?
            };

            stream.write_u32(0x80000000).await
                .with_context(|| "Failed to write result")?;

            while let Some(request) = channel_request_rx.recv().await {
                log::info!("Sending connection request...");
                let bytes = request.as_bytes();

                if let Err(e) = stream.write_all(bytes).await {
                    log::info!("Failed to write connect request: {}", e);
                    break;
                }
            }

            log::info!("Cleaning up connection...");

            let mut connections = connections.lock().await;
            connections.free_connection(connection_id);
        }
    }

    Ok(())
}

async fn process_local_client(config: Arc<Config>, local_stream: TcpStream) -> Result<(), anyhow::Error> {

    log::info!("Constructing client jwt...");

    let jwt_claims = PeerJwt {
        peer: PeerInfo::ClientStream {
            connection_id: config.id.clone(),
            channel_id: uuid::Uuid::new_v4(),
        }
    };

    let raw_key = base64::decode(&config.key)
        .with_context(|| "Failed to decode base64 key")?;

    let pem = picky::pem::parse_pem(&raw_key)
        .with_context(|| "Invalid pem key")?;
    let key = PrivateKey::from_pem(&pem)
        .with_context(|| "Invalid private key")?;

    let jwt = JwtSig::new(JwsAlg::RS256, jwt_claims)
        .encode(&key)
        .with_context(|| "Failed to create client jwt")?;

    let jwt_bytes = jwt.as_bytes();

    log::info!("Connecting to server...");

    let mut stream = TcpStream::connect(&config.server)
        .await
        .with_context(|| "Failed to connect to the server socket")?;

    stream.write_u32(jwt_bytes.len() as u32)
        .await
        .with_context(|| "Failed to write jwt size")?;

    stream.write_all(jwt_bytes)
        .await
        .with_context(|| "Failed to write client jwt")?;

    log::info!("Waiting for server to answer...");

    let result = stream.read_u32()
        .await
        .with_context(|| "Failed to read server result")?;

    if result != 0x80000000 {
        anyhow::bail!("Invalid server response");
    }

    log::info!("[OK] Forwarding socket...");

    forward_socket(local_stream, stream).await?;
    Ok(())
}

async fn connect_command(cmd: ConnectCommand) -> Result<(), anyhow::Error> {
    log::info!("Reading config...");
    let mut file = File::open(cmd.config)
        .await
        .with_context(|| "Failed to open config file")?;

    let mut serialized_config = String::new();
    file.read_to_string(&mut serialized_config)
        .await
        .with_context(|| "Failed to read the config")?;

    let config: Config = toml::from_str(&serialized_config)
        .with_context(|| "Invalid config file")?;

    let config = Arc::new(config);

    let listener = TcpListener::bind(format!("0.0.0.0:{}", config.client_port.clone().map(|p| p.0).unwrap_or(0)))
        .await
        .with_context(|| "Failed to bind client port")?;

    log::info!("[OK] Bound local port {}", listener.local_addr().expect("failed to get local port"));

    loop {
        match listener.accept().await {
            Ok((stream, socket)) => {
                log::info!("Incomming connection {}", socket);
                let config = config.clone();
                tokio::spawn(async move {
                    match process_local_client(config, stream).await {
                        Ok(()) => log::info!("Client connection closed"),
                        Err(e) => log::error!("Client conenction failed: {}", e),
                    };
                });
            }
            Err(e) => {
                log::error!("Peer connection failed: {}", e);
            }
        }
    }

    Ok(())
}

async fn listen_command(cmd: ListenCommand) -> Result<(), anyhow::Error> {
    log::info!("Reading config...");
    let mut file = File::open(cmd.config)
        .await
        .with_context(|| "Failed to open config file")?;

    let mut serialized_config = String::new();
    file.read_to_string(&mut serialized_config)
        .await
        .with_context(|| "Failed to read the config")?;

    let config: Config = toml::from_str(&serialized_config)
        .with_context(|| "Invalid config file")?;

    let config = Arc::new(config);

    log::info!("Constructing listener jwt");

    let raw_key = base64::decode(&config.key)
        .with_context(|| "Failed to decode base64 key")?;

    let pem = picky::pem::parse_pem(&raw_key)
        .with_context(|| "Invalid pem key")?;
    let key = PrivateKey::from_pem(&pem)
        .with_context(|| "Invalid private key")?;

    let key = Arc::new(key);

    let jwt_claims = PeerJwt {
        peer: PeerInfo::ServerListener {
            connection_id: config.id.clone(),
        }
    };

    let jwt = JwtSig::new(JwsAlg::RS256, jwt_claims)
        .encode(&key)
        .with_context(|| "Failed to create server listener jwt")?;

    let jwt_bytes = jwt.as_bytes();

    log::info!("Connecting to server...");

    let mut stream = TcpStream::connect(&config.server)
        .await
        .with_context(|| "Failed to connect to the server")?;

    stream.write_u32(jwt_bytes.len() as u32)
        .await
        .with_context(|| "Failed to write jwt size")?;

    stream.write_all(jwt_bytes)
        .await
        .with_context(|| "Failed to write server listener jwt")?;

    log::info!("Waiting for response...");

    let result = stream.read_u32()
        .await
        .with_context(|| "Failed to read server result")?;

    if result != 0x80000000 {
        anyhow::bail!("Invalid server response");
    }

    log::info!("[OK] Listener established, waiting for new connections...");

    loop {
        let mut header_buffer = [0u8; 16];

        stream
            .read_exact(&mut header_buffer)
            .await
            .with_context(|| "Failed to read connection request")?;

        let channel_id = uuid::Uuid::from_bytes(header_buffer);

        log::info!("Received new connection request, creating channel...");

        let config = config.clone();
        let key = key.clone();
        tokio::task::spawn(async move {
            match process_server_stream(config, key, channel_id).await {
                Ok(()) => log::info!("Server connection closed"),
                Err(e) => log::error!("Server conenction failed: {}", e),
            };
        });
    }

    Ok(())
}

async fn process_server_stream(config: Arc<Config>, key: Arc<PrivateKey>, channel_id: uuid::Uuid) -> Result<(), anyhow::Error> {
    log::info!("Creating server stream jwt...");

    let jwt_claims = PeerJwt {
        peer: PeerInfo::ServerStream {
            connection_id: config.id.clone(),
            channel_id
        }
    };

    let jwt = JwtSig::new(JwsAlg::RS256, jwt_claims)
        .encode(&key)
        .with_context(|| "Failed to create server stream jwt")?;

    let jwt_bytes = jwt.as_bytes();

    log::info!("Connecting to server as stream...");

    let mut stream = TcpStream::connect(&config.server)
        .await
        .with_context(|| "Failed to connect to the server")?;

    stream.write_u32(jwt_bytes.len() as u32)
        .await
        .with_context(|| "Failed to write jwt size")?;

    stream.write_all(jwt_bytes)
        .await
        .with_context(|| "Failed to write server listener jwt")?;

    log::info!("Waiting for response...");

    let result = stream.read_u32()
        .await
        .with_context(|| "Failed to read server result")?;

    if result != 0x80000000 {
        anyhow::bail!("Invalid server response");
    }

    log::info!("[OK] Server Stream established, waiting for new connections...");

    log::info!("Connecting to local socket...");

    let local_stream = TcpStream::connect(format!("127.0.0.1:{}", config.port.0))
        .await
        .with_context(|| "Failed to connect to local port")?;

    forward_socket(local_stream, stream).await?;

    Ok(())
}

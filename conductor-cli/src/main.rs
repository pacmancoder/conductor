mod args;

use args::{AcceptedCommand, Args};

/*
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
*/

fn main() {
    match Args::accept() {
        AcceptedCommand::Create(_) => {}
        AcceptedCommand::Serve(_) => {}
        AcceptedCommand::Connect(_) => {}
        AcceptedCommand::Listen(_) => {}
    }
}

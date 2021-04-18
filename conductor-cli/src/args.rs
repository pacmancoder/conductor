use clap::Clap;
use conductor::{
    command,
    net::{PortNum, Protocol},
};
use std::path::PathBuf;

/// Easy tunneling tool which just works
#[derive(Clap)]
#[clap(version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"))]
#[clap(setting = clap::AppSettings::ColoredHelp)]
pub struct Args {
    /// Set log level filter
    #[clap(long, default_value = "info", possible_values = &["error", "warn", "info", "debug", "trace"])]
    log_level: log::LevelFilter,

    #[clap(subcommand)]
    command: ArgsCommand,
}

pub enum AcceptedCommand {
    Create(command::CreateCommand),
    Serve(command::ServeCommand),
    Connect(command::ConnectCommand),
    Listen(command::ListenCommand),
}

impl Args {
    pub fn accept() -> AcceptedCommand {
        let args = Args::parse();

        env_logger::builder()
            .filter_level(args.log_level)
            .try_init()
            .map_err(|e| panic!("Failed to init logger: {}", e))
            .unwrap();

        match args.command {
            ArgsCommand::Create(cmd) => AcceptedCommand::Create(command::CreateCommand {
                port: cmd.port,
                client_port: cmd.client_port,
                protocol: cmd.protocol,
                server: cmd.server,
                key: cmd.key,
                file_name: cmd.file_name,
            }),
            ArgsCommand::Serve(cmd) => AcceptedCommand::Serve(command::ServeCommand {
                port: cmd.port,
                public_key: cmd.public_key,
            }),
            ArgsCommand::Connect(cmd) => {
                AcceptedCommand::Connect(command::ConnectCommand { config: cmd.config })
            }
            ArgsCommand::Listen(cmd) => {
                AcceptedCommand::Listen(command::ListenCommand { config: cmd.config })
            }
        }
    }
}

#[derive(Clap)]
enum ArgsCommand {
    Create(CreateCommand),
    Serve(ServeCommand),
    Connect(ConnectCommand),
    Listen(ListenCommand),
}

/// Create new tunnel configuration
#[derive(Clap)]
struct CreateCommand {
    /// Port which will be used on the listening side of the tunnel
    #[clap(long, short)]
    port: PortNum,
    /// Hint for port which will be used on the client side of the tunnel.
    /// If not specified, any free system port will be used
    #[clap(long, short)]
    client_port: Option<PortNum>,
    /// Protocol for the tunnel. udp will emulate udp over tcp tunnel
    #[clap(long, default_value = "tcp", possible_values = &["tcp", "udp"])]
    protocol: Protocol,
    /// URL for the conductor server
    #[clap(long, short)]
    server: String,
    /// Private key
    #[clap(long, short)]
    key: PathBuf,
    /// Output for the config file
    file_name: PathBuf,
}

/// Serve conductor server
#[derive(Clap)]
struct ServeCommand {
    #[clap(long)]
    port: PortNum,
    #[clap(long)]
    public_key: PathBuf,
}

/// Connect to the tunnel using config file
#[derive(Clap)]
struct ConnectCommand {
    config: PathBuf,
}

/// Listen on the tunnel using config file
#[derive(Clap)]
struct ListenCommand {
    config: PathBuf,
}

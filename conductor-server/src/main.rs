use clap::Clap;
use anyhow::Context;
use tokio::sync::Notify;
use std::sync::Arc;

use conductor_server::rest::server::RestServer;

#[derive(Clap)]
#[clap(version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"))]
struct CliArgs;

fn register_ctrlc_handler() -> Arc<Notify> {
    let shutdown_event = Arc::new(Notify::new());

    {
        let event = shutdown_event.clone();
        ctrlc::set_handler(move || {
            event.notify_waiters();
            log::warn!("Detected stop server request (Ctrl+C)");
        }).expect("Failed to set ctrl-c handler");
    }

    shutdown_event
}

fn spawn_server_tasks(runtime: &tokio::runtime::Runtime) {
    runtime.spawn(RestServer::new().run());

    // TODO: Return gracefull shutdown handles Vec
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let _ = CliArgs::parse();

    let shutdown_event = register_ctrlc_handler();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .with_context(|| "Failed to initialize tokio runtime")?;

    spawn_server_tasks(&runtime);

    runtime.block_on(async {
        shutdown_event.notified().await;
    });

    drop(runtime);
    log::info!("Server was gracefully shut down");

    Ok(())
}
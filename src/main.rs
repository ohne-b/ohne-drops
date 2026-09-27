use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use twitch_drops_miner::{
    miner::Miner,
    web::{self, App},
};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    host: IpAddr,
    #[arg(long, env = "PORT", default_value_t = 8080)]
    port: u16,
    #[arg(long, env = "DATA_DIR", default_value = "data")]
    data_dir: PathBuf,
    #[arg(long, env = "LOG_DIR", default_value = "logs")]
    log_dir: PathBuf,
    #[arg(long, env = "PUBLIC_BASE_URL", default_value = "")]
    public_base_url: String,
    #[arg(short,long,action=clap::ArgAction::Count)]
    verbose: u8,
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    Healthcheck,
}

fn log_filter(verbose: u8) -> EnvFilter {
    // Transport TRACE output includes OAuth-bearing frames. Only this package
    // may increase verbosity; ambient RUST_LOG cannot enable dependency traces.
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    EnvFilter::new(format!("warn,twitch_drops_miner={level}"))
}

fn logging(args: &Args) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("TDM")
        .filename_suffix("log")
        .max_log_files(5)
        .build(&args.log_dir)
        .context("could not open log directory")?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    tracing_subscriber::registry()
        .with(log_filter(args.verbose))
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_writer(std::io::stderr),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(false)
                .with_writer(writer),
        )
        .try_init()
        .context("could not initialize logging")?;
    Ok(guard)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {_=terminate.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn run(args: Args) -> Result<()> {
    if matches!(args.command, Some(Action::Healthcheck)) {
        let host = if args.host.is_unspecified() {
            "127.0.0.1".parse().unwrap()
        } else {
            args.host
        };
        let url = format!("http://{}/healthz", SocketAddr::new(host, args.port));
        let response = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()?
            .get(url)
            .send()
            .await
            .context("health check failed")?;
        anyhow::ensure!(
            response.status() == reqwest::StatusCode::OK,
            "health check failed"
        );
        return Ok(());
    }
    let _logs = logging(&args)?;
    let (app, commands) = App::open(args.data_dir, &args.public_base_url)?;
    let address = SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("could not bind dashboard address")?;
    tracing::info!(version=env!("CARGO_PKG_VERSION"),%address,"Starting Twitch Drops Miner");
    let shutdown = app.shutdown.clone();
    let router = web::router(app.clone());
    let mut server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
    });
    let mut miner = tokio::spawn(Miner::new(app.clone(), commands).run());
    let mut miner_finished = false;
    let mut server_finished = false;
    let mut failure = None;
    tokio::select! {
        _=shutdown_signal()=>{},
        _=app.shutdown.cancelled()=>{},
        result=&mut miner=>{miner_finished=true;if !matches!(result,Ok(Ok(()))){failure=Some("mining task failed");}},
        result=&mut server=>{server_finished=true;if !matches!(result,Ok(Ok(()))){failure=Some("dashboard server failed");}},
    }
    app.shutdown.cancel();
    if !miner_finished && !matches!(miner.await, Ok(Ok(()))) {
        failure = Some("mining task failed");
    }
    app.drain_writes().await;
    app.sockets.close().await;
    if !server_finished {
        match tokio::time::timeout(Duration::from_secs(10), &mut server).await {
            Ok(Ok(Ok(()))) => {}
            Ok(_) => failure = Some("dashboard server failed"),
            Err(_) => {
                server.abort();
                let _ = server.await;
            }
        }
    }
    tracing::info!("Shutdown complete");
    if let Some(failure) = failure {
        anyhow::bail!(failure);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Twitch Drops Miner: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Writer(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn maximal_verbosity_cannot_log_transport_frames_or_request_credentials() {
        let output = Arc::new(Mutex::new(vec![]));
        let writer = Writer(output.clone());
        let subscriber = tracing_subscriber::registry().with(log_filter(255)).with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || writer.clone()),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::trace!(target:"tungstenite::protocol","LISTEN auth_token=secret");
            tracing::debug!(target:"reqwest::connect","proxy password=secret");
            tracing::trace!(target:"twitch_drops_miner","safe application diagnostic");
        });
        let text = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(text.contains("safe application diagnostic"));
        assert!(!text.contains("secret"));
    }
}

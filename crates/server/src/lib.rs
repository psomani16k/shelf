mod api;
mod auth;
mod data;
mod error;
mod frontend;
mod library;
mod logging;

use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{Router, middleware};
use shelf_utility::timeutil::Time;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::info;

use crate::{api::AppState, auth::Auth, data::DataFile, library::ServerLibrary};

pub use logging::init as init_logging;

/// Where the server keeps things. The default is the Docker image's layout.
pub struct Config {
    /// The library: a folder of `.epub` files.
    pub books: PathBuf,
    /// The server's own data (`data.json`), kept out of the library so it never syncs with it.
    pub data: PathBuf,
    pub bind: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            books: PathBuf::from("/books"),
            data: PathBuf::from("/data"),
            bind: "0.0.0.0:8080".to_owned(),
        }
    }
}

/// Serves until Ctrl-C or SIGTERM. Expects a `tracing` subscriber to be installed already
/// (`init_logging`, or the caller's own).
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let Config { books, data, bind } = config;
    let data_file = DataFile::load(&data, Time::now())
        .await
        .with_context(|| format!("couldn't load the server data in {}", data.display()))?;
    let auth = Auth::new(data_file);
    // Never log the users themselves: they hold passwords.
    info!(users = auth.user_count().await, "server data loaded");

    let library = ServerLibrary::open(books.clone(), || {})
        .await
        .with_context(|| format!("couldn't open the library at {}", books.display()))?;

    let state = AppState {
        library: Arc::new(library),
        auth: Arc::new(auth),
    };
    let app = Router::new()
        .nest(
            "/api",
            // Outermost last: the id is set, then the access log runs with it.
            api::router(state.clone())
                .layer(middleware::from_fn(logging::access_log))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid)),
        )
        .fallback(frontend::serve)
        .with_state(state);

    let listener = TcpListener::bind(&bind)
        .await
        .with_context(|| format!("couldn't listen on {bind}"))?;
    info!(address = %bind, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    info!("stopped");
    Ok(())
}

/// Ctrl-C, or SIGTERM from `docker stop`.
async fn shutdown_signal() {
    let mut terminate = signal(SignalKind::terminate()).expect("can install a SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

//! # HTTP Server
//!
//! Binds an axum router on the bridge's `config_port` (default 8080). The
//! server is cheap to run in a background tokio task and shares the same
//! [`BridgeState`] used by Tauri commands.

use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use super::handlers;

/// Handle that lets the caller shut down the HTTP server gracefully.
#[derive(Debug)]
pub struct HttpServerHandle {
    pub cancel: tokio::sync::oneshot::Sender<()>,
}

/// Build the router and bind to `0.0.0.0:{port}`. Returns a handle that can
/// be used to abort the server on app shutdown.
///
/// The server keeps a clone of the shared [`ApiState`](crate::state::ApiState);
/// all handlers are async and non-blocking.
pub async fn start_http_server(
    port: u16,
    state: Arc<crate::state::ApiState>,
) -> std::io::Result<HttpServerHandle> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    let app = Router::new()
        .route("/", get(handlers::html_status_page))
        .route("/status", get(handlers::get_status))
        .route("/peers", get(handlers::get_peers))
        .route("/feedback", get(handlers::get_feedback))
        .route("/config/global", get(handlers::get_global_config))
        .route("/config/global", post(handlers::save_global_config))
        .route("/devices/registered", get(handlers::get_registered_devices))
        .route("/devices/{slot}/target", post(handlers::set_device_target))
        // TODO: re-add /devices/{slot}/test/{cmd} once the axum Handler trait
        // bound issue with the async OscSender future is resolved.
        // .route("/devices/{slot}/test/{cmd}", post(handlers::test_device))
        .with_state(state);

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        log::info!("HTTP config API listening on http://{addr}");
        let serve = axum::serve(listener, app);
        tokio::select! {
            _ = serve => {
                log::info!("HTTP config API server stopped");
            }
            _ = &mut rx => {
                log::info!("HTTP config API server received shutdown signal");
            }
        }
    });

    Ok(HttpServerHandle { cancel: tx })
}

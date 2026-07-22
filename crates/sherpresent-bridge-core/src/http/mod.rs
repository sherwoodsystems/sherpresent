//! # HTTP Config API
//!
//! Serves the remote configuration surface on `config_port` (default 8080).
//!
//! Mirrors the JSON endpoints originally exposed by `bridge-py/config_server.py`:
//!
//! - `GET  /status`                -> running status
//! - `GET  /peers`                 -> discovered desktop peers
//! - `GET  /feedback`              -> cached OSC feedback state
//! - `GET  /config/global`         -> bridge config
//! - `POST /config/global`         -> save bridge config
//! - `GET  /devices/registered`    -> device slot registration
//! - `POST /devices/{slot}/target` -> assign a target to a slot

pub mod handlers;
pub mod server;

pub use server::{start_http_server, HttpServerHandle};

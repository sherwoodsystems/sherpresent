//! # HTTP Config API
//!
//! Serves the remote configuration surface that the desktop app's Bridges page
//! (and external browsers) can talk to on `config_port` (default 8080).
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
//! - `POST /devices/{slot}/test/{next|prev}` -> test OSC to a slot
//!
//! A tiny HTML status page is served at `/` for users who open the config
//! port in a browser; the real configuration UI is the Tauri webview on the
//! bridge device itself.

pub mod handlers;
pub mod server;

pub use server::{start_http_server, HttpServerHandle};

//! # OSC
//!
//! Outgoing (`osc::sender`) and incoming (`osc::feedback`) OSC handling for
//! the SherPresent bridge.
//!
//! The bridge speaks the oscpoint OSC schema (`/oscpoint/...`) so it can
//! interoperate with both [OSCPoint](https://github.com/phuvf/oscpoint) on
//! Windows and SherPresent desktop on macOS/Linux.

pub mod feedback;
pub mod sender;
pub mod messages;

//! # Channel Module
//!
//! Manages peer-to-peer channel synchronization between sher-present instances.
//!
//! NOTE: This module is planned for future sher-present-to-sher-present sync.
//! Currently, mDNS discovery handles peer discovery and command source tracking
//! handles rpi-osc-bridge devices.

#[allow(dead_code)]
pub mod manager;
#[allow(dead_code)]
pub mod peer;

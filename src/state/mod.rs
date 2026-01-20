//! State machine module
//!
//! Provides the core state machine implementation and types for the VPN manager.

pub mod machine;
pub mod types;

pub use machine::{StateMachine, StateMachineConfig};
pub use types::{ActiveVpnInfo, Event, NmVpnState, TransitionReason, VpnState};

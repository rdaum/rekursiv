//! Architectural models for the Rekursiv object and microinstruction processors.
//!
//! [`Model`] implements OBJEKT command semantics for the native emulator and RTL
//! comparisons. It owns the pager, object RAM, virtual registers, and backing
//! store. It has no RTL or Verilator dependency and does not model cycle timing.
//!
//! # Command execution
//!
//! * [`Model::execute_response`] serves the native emulator without resident
//!   access logs. [`Model::execute`] also records requests for metrics and tests.
//! * [`Model::execute_raw`] accepts encoded ports for RTL comparisons.
//!   [`Model::execute_faults`] injects failures at specified request positions.
//! * [`Model::service`] supplies bootstrap and maintenance operations.
//!
//! Commands read their operands from the old register state. Resident command
//! failures preserve local state; streaming transfers can leave partial effects.
//! See the individual methods for their validation and publication boundaries.
//!
//! # Source layout
//!
//! The OBJEKT implementation separates state (`model.rs`, `state.rs`), command
//! boundaries (`command.rs`), resident operations (`datapath.rs`), and lookup
//! (`pager.rs`). `transfer.rs`, `directory.rs`, and `exchange.rs` handle commands
//! that can use backing storage. `effects.rs` defines observations; `service.rs`
//! implements bootstrap and maintenance services.
//!
//! [`processor`] models LOGIK/NUMERIK instruction retirement; [`float`] supplies
//! floating-point reference results. [`Model::recover`] and [`Model::collect_ram`]
//! are test specifications. The native emulator executes collector microcode
//! against its own maintenance datapath instead of calling these graph walkers.

mod command;
mod datapath;
mod directory;
mod effects;
mod exchange;
mod model;
mod pager;
mod recovery;
mod service;
mod state;
mod transfer;

pub mod float;
pub mod processor;
pub mod ram_gc;
pub mod store;

pub use effects::{MemoryEffect, Outcome};
pub use model::Model;
pub use state::{PreparedAccess, State};

#[cfg(test)]
mod tests;

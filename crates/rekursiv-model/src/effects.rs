//! Command responses and observable RAM/backing-store requests.
#![deny(missing_docs)]

use rekursiv_asm::{Response, Status, StoreRequest, Word};

/// One attempted external RAM word access, including an access that faults.
///
/// Reads of the cached first object word produce no RAM request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryEffect {
    /// Zero-based RAM word address.
    pub address: u32,
    /// True for a write request, false for a read request.
    pub write: bool,
    /// Write data. For reads, this request-side value is not the returned RAM word.
    pub value: Word,
}
/// Response and request observations from one object command or service.
///
/// Request vectors include failed attempts and retain order within each channel.
/// They describe traffic, not a rollback log: streaming commands can publish
/// partial effects before a later request fails. The vectors do not specify
/// cycle timing or a total order between RAM and backing-store requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Architectural status and returned word.
    pub response: Response,
    /// RAM requests in issue order, including an injected failing request.
    pub memory: Vec<MemoryEffect>,
    /// Backing-store requests in issue order, including an injected failing request.
    pub store: Vec<StoreRequest>,
}
impl Outcome {
    pub(super) fn error(status: Status) -> Self {
        Self {
            response: Response::error(status),
            memory: Vec::new(),
            store: Vec::new(),
        }
    }
}

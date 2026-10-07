//! Typed and wire command boundaries, validation, and dispatch.
#![deny(missing_docs)]

use crate::{Model, Outcome};
use rekursiv_asm::{Command, Faults, Pager, Ports, Response, Status};

impl Model {
    /// Validate and execute a typed command, with RAM and backing-store observations.
    ///
    /// Static command validation precedes the maintenance-lock check. If
    /// `memory_error` is true, the first external RAM request fails. Cached
    /// first-word reads issue no RAM request and therefore do not inject a fault.
    ///
    /// The response reports validation, lookup, bounds, type, and transfer errors.
    /// Resident datapath errors preserve registers and RAM. Streaming transfers
    /// retain their documented partial effects and attempted-request records.
    /// See [`Self::execute_transfer`] for those publication boundaries.
    pub fn execute(&mut self, command: Command, memory_error: bool) -> Outcome {
        let command = match command.validate() {
            Ok(command) => command,
            Err(status) => return Outcome::error(status),
        };
        if self.maintenance {
            return Outcome::error(Status::BadCommand);
        }
        self.execute_validated(
            command,
            Faults {
                memory_at: memory_error.then_some(0),
                store_at: None,
            },
        )
    }
    /// Execute a typed command without returning request observations.
    ///
    /// This has the same validation order, architectural effects, and fault
    /// behavior as [`Self::execute`]. Common resident commands avoid allocation
    /// of an access log. Streaming commands still use the shared transfer path
    /// and discard its observations after execution.
    pub fn execute_response(&mut self, command: Command, memory_error: bool) -> Response {
        let command = match command.validate() {
            Ok(command) => command,
            Err(status) => return Response::error(status),
        };
        if self.maintenance {
            return Response::error(Status::BadCommand);
        }
        match command.pager {
            Pager::NextObject
            | Pager::FindObject
            | Pager::Exchange
            | Pager::Fetch
            | Pager::Allocate => {
                self.execute_validated(
                    command,
                    Faults {
                        memory_at: memory_error.then_some(0),
                        store_at: None,
                    },
                )
                .response
            }
            _ => match self.transition(command, memory_error, &mut None) {
                Ok(data) => Response::ok(data),
                Err(status) => Response::error(status),
            },
        }
    }
    /// Decode and execute wire ports, with an optional fault on the first RAM request.
    ///
    /// Equivalent to [`Self::execute_faults`] with no backing-store fault and
    /// `memory_at` set to zero when `memory_error` is true. The maintenance lock
    /// takes precedence over malformed wire fields, unlike the typed boundary.
    pub fn execute_raw(&mut self, ports: Ports, memory_error: bool) -> Outcome {
        self.execute_faults(
            ports,
            Faults {
                memory_at: memory_error.then_some(0),
                store_at: None,
            },
        )
    }
    /// Decode wire ports and execute with indexed RAM and backing-store faults.
    ///
    /// Fault positions are zero-based within each request channel for this
    /// command. The failing request appears in the returned observations.
    /// Maintenance rejects requests before decoding their fields; otherwise,
    /// decoding validates numeric controls before dispatch.
    ///
    /// The response carries all command errors. Earlier streaming effects can
    /// remain visible after a failure; resident datapath effects are atomic.
    pub fn execute_faults(&mut self, ports: Ports, faults: Faults) -> Outcome {
        if self.maintenance {
            return Outcome::error(Status::BadCommand);
        }
        match ports.decode() {
            Ok(command) => self.execute_validated(command, faults),
            Err(status) => Outcome::error(status),
        }
    }
    // Callers have checked the command and maintenance lock. Keep
    // dispatch and all memory/store effects shared; only wire conversion differs.
    fn execute_validated(&mut self, command: Command, faults: Faults) -> Outcome {
        match command.pager {
            Pager::NextObject | Pager::FindObject => {
                return self.execute_directory(command, faults);
            }
            Pager::Exchange => return self.execute_exchange(command, faults),
            Pager::Fetch | Pager::Allocate => {
                return self.transfer_validated(command, faults);
            }
            _ => {}
        }
        let memory_error = faults.memory_at == Some(0);
        let mut effect = None;
        // transition validates all fallible conditions before publishing a
        // memory write or replacing state. Cloning the complete disk image on
        // every register read adds no rollback protection and makes original
        // image execution prohibitively expensive.
        let result = self.transition(command, memory_error, &mut effect);
        match result {
            Ok(data) => Outcome {
                response: Response::ok(data),
                memory: effect.into_iter().collect(),
                store: Vec::new(),
            },
            Err(e) => Outcome {
                response: Response::error(e),
                memory: effect.into_iter().collect(),
                store: Vec::new(),
            },
        }
    }
}

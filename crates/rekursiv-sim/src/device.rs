//! RTL handshake adapter for the shared external peripherals.
use crate::{Harness, Result};
pub use rekursiv_devices::*;
impl Harness<'_> {
    pub(super) fn tick_device(&mut self) -> Result<()> {
        self.rtl.io_ready_i = self.device.ready() as u8;
        self.rtl.io_response_i = 0;
        if let Some(reply) = self.device.response() {
            self.rtl.io_response_i = 1;
            self.rtl.io_result_i = reply.data;
            self.rtl.io_error_i = reply.error as u8;
        }
        self.rtl.eval();
        let request = (self.rtl.io_valid_o != 0).then_some(Request {
            address: self.rtl.io_address_o,
            write: self.rtl.io_write_o != 0,
            data: self.rtl.io_data_o,
        });
        self.device.tick(request, self.rtl.io_response_ready_o != 0)
    }
}

//! Generic 32-bit register device for transport tests. Addresses and payloads
//! are opaque; no guest classes, primitives, processes, or object references
//! enter this adapter. Workstation peripherals will implement this same wire
//! contract with their own device registers and FIFOs.
use super::*;
use std::collections::BTreeMap;
mod bitmap;
pub use bitmap::{Bitmap, BitmapFrame};
mod clocks;
pub use clocks::Clocks;
mod input;
pub use input::{Input, InputKind, InputPacket};
mod pointer;
pub use pointer::Pointer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub address: u32,
    pub write: bool,
    pub data: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reply {
    pub data: u32,
    pub error: bool,
}
#[derive(Clone, Copy)]
struct Pending {
    request: Request,
    reply: Reply,
    delay: u32,
    clock_effect: Option<clocks::Effect>,
    pointer_effect: Option<pointer::Effect>,
    input_effect: Option<input::Effect>,
    bitmap_effect: Option<(bool, bitmap::Effect)>,
}
/// External counted event source. This device has no access to guest memory.
/// Tests schedule notifications by device clock tick, independent of CPU state.
#[derive(Default)]
pub struct Events {
    pub counts: [u32; 4],
    pub schedule: BTreeMap<u64, u32>,
    pub ticks: u64,
    pub acknowledgements: [u64; 4],
}
impl Events {
    pub fn status(&self) -> u32 {
        self.counts
            .iter()
            .enumerate()
            .fold(0, |mask, (i, &n)| mask | (u32::from(n != 0) << i))
    }
    fn acknowledge(&mut self, mask: u32) {
        for (i, count) in self.counts.iter_mut().enumerate() {
            if mask & (1 << i) != 0 && *count != 0 {
                *count -= 1;
                self.acknowledgements[i] += 1;
            }
        }
    }
    fn tick(&mut self) -> Result<()> {
        // Arrivals follow acknowledgements on this edge: an arriving event is
        // never erased by a simultaneous consume of an earlier notification.
        if let Some(mask) = self.schedule.remove(&self.ticks) {
            for (i, count) in self.counts.iter_mut().enumerate() {
                if mask & (1 << i) != 0 {
                    *count = count
                        .checked_add(1)
                        .ok_or_else(|| eyre::eyre!("event counter overflow"))?;
                }
            }
        }
        self.ticks += 1;
        Ok(())
    }
}
#[derive(Default)]
pub struct Device {
    pub registers: BTreeMap<u32, u32>,
    pub requests: Vec<Request>,
    pub completions: Vec<(Request, Reply)>,
    pub timing: Timing,
    pub fail_next: bool,
    pub events: Option<Events>,
    pub clocks: Option<Clocks>,
    pub pointer: Option<Pointer>,
    pub input: Option<Input>,
    pub cursor_bitmap: Option<Bitmap>,
    pub display_bitmap: Option<Bitmap>,
    pending: Option<Pending>,
    held: Option<Request>,
    wait: u32,
}
impl Harness<'_> {
    fn device_request(&self) -> Request {
        Request {
            address: self.rtl.io_address_o,
            write: self.rtl.io_write_o != 0,
            data: self.rtl.io_data_o,
        }
    }
    pub(super) fn tick_device(&mut self) -> Result<()> {
        if let Some(held) = self.device.held {
            ensure!(
                self.rtl.io_valid_o != 0 && self.device_request() == held,
                "device request changed under backpressure"
            );
        }
        self.rtl.io_ready_i = (self.device.pending.is_none()
            && self.device.wait >= self.device.timing.request_delay)
            as u8;
        self.rtl.io_response_i = 0;
        if let Some(p) = self.device.pending {
            if p.delay == 0 {
                self.rtl.io_response_i = 1;
                self.rtl.io_result_i = p.reply.data;
                self.rtl.io_error_i = p.reply.error as u8;
            }
        }
        self.rtl.eval();
        let accepted = self.rtl.io_valid_o != 0 && self.rtl.io_ready_i != 0;
        let complete = self.rtl.io_response_i != 0 && self.rtl.io_response_ready_o != 0;
        self.device.held = if self.rtl.io_valid_o != 0 && !accepted {
            Some(self.device_request())
        } else {
            None
        };
        if complete {
            let p = self.device.pending.take().unwrap();
            if let Some(effect) = p.clock_effect.filter(|_| !p.reply.error) {
                if self.device.clocks.as_mut().unwrap().commit(effect) {
                    self.device
                        .events
                        .as_mut()
                        .ok_or_else(|| eyre::eyre!("timer requires event controller"))?
                        .counts[1] = 0;
                }
            } else if let Some(effect) = p.pointer_effect.filter(|_| !p.reply.error) {
                self.device.pointer.as_mut().unwrap().commit(effect);
            } else if let Some(effect) = p.input_effect.filter(|_| !p.reply.error) {
                self.device.input.as_mut().unwrap().commit(effect);
            } else if let Some((cursor, effect)) = p.bitmap_effect.filter(|_| !p.reply.error) {
                (if cursor {
                    self.device.cursor_bitmap.as_mut()
                } else {
                    self.device.display_bitmap.as_mut()
                })
                .unwrap()
                .commit(effect);
            } else if p.request.write && !p.reply.error {
                match (p.request.address, self.device.events.as_mut()) {
                    (0x104, Some(events)) => events.acknowledge(p.request.data),
                    _ => {
                        self.device
                            .registers
                            .insert(p.request.address, p.request.data);
                    }
                }
            }
            self.device.completions.push((p.request, p.reply));
        } else if let Some(p) = &mut self.device.pending {
            p.delay = p.delay.saturating_sub(1);
        }
        if accepted {
            ensure!(
                self.device.pending.is_none(),
                "second outstanding device request"
            );
            let request = self.device_request();
            let mut clock_effect = None;
            let mut pointer_effect = None;
            let mut input_effect = None;
            let mut bitmap_effect = None;
            let value = if let Some(clocks) = self
                .device
                .clocks
                .as_ref()
                .filter(|_| (0x200..0x220).contains(&request.address))
            {
                clocks.preview(request).map(|(value, effect)| {
                    clock_effect = effect;
                    value
                })
            } else if let Some(pointer) = self.device.pointer.as_ref().filter(|_| {
                (0x300..0x310).contains(&request.address)
                    || (0x400..0x410).contains(&request.address)
            }) {
                pointer.preview(request).map(|(value, effect)| {
                    pointer_effect = effect;
                    value
                })
            } else if let Some(input) = self
                .device
                .input
                .as_ref()
                .filter(|_| (0x310..0x328).contains(&request.address))
            {
                input.preview(request).map(|(value, effect)| {
                    input_effect = effect;
                    value
                })
            } else if (0x410..0x430).contains(&request.address)
                || (0x510..0x530).contains(&request.address)
            {
                let cursor = request.address < 0x500;
                let base = if cursor { 0x410 } else { 0x510 };
                let bitmap = if cursor {
                    self.device.cursor_bitmap.as_ref()
                } else {
                    self.device.display_bitmap.as_ref()
                };
                if let Some(bitmap) = bitmap {
                    bitmap.preview(request, base).map(|(value, effect)| {
                        bitmap_effect = effect.map(|effect| (cursor, effect));
                        value
                    })
                } else if !request.write && (request.address == base || request.address == base + 4)
                {
                    Some(0)
                } else {
                    None
                }
            } else if let Some(events) = &self.device.events {
                match (request.address, request.write) {
                    (0x100, false) => Some(events.status()),
                    (0x104, true) if request.data & !15 == 0 => Some(0),
                    (0x100 | 0x104, _) => None,
                    _ => self.device.registers.get(&request.address).copied(),
                }
            } else {
                self.device.registers.get(&request.address).copied()
            };
            let reply = Reply {
                data: if request.write { 0 } else { value.unwrap_or(0) },
                error: self.device.fail_next || value.is_none(),
            };
            self.device.fail_next = false;
            self.device.pending = Some(Pending {
                request,
                reply,
                delay: self.device.timing.memory_latency,
                clock_effect,
                pointer_effect,
                input_effect,
                bitmap_effect,
            });
            self.device.requests.push(request);
            self.device.wait = 0;
        } else if self.rtl.io_valid_o != 0 {
            self.device.wait += 1;
        } else {
            self.device.wait = 0;
        }
        if self
            .device
            .clocks
            .as_mut()
            .is_some_and(|clocks| clocks.tick())
        {
            let events = self
                .device
                .events
                .as_mut()
                .ok_or_else(|| eyre::eyre!("timer requires event controller"))?;
            events.counts[1] = events.counts[1]
                .checked_add(1)
                .ok_or_else(|| eyre::eyre!("timer event counter overflow"))?;
        }
        let mut arrivals = 0;
        if let Some(pointer) = &mut self.device.pointer {
            let clock = if self.device.input.is_some() {
                let clocks = self
                    .device
                    .clocks
                    .as_ref()
                    .ok_or_else(|| eyre::eyre!("pointer sampling requires a clock"))?;
                Some((clocks.monotonic_ms, clocks.day_milliseconds()))
            } else {
                None
            };
            if let Some(packet) = pointer.tick(clock) {
                arrivals += u32::from(self.device.input.as_mut().unwrap().push(packet));
            }
        }
        if let Some(input) = &mut self.device.input {
            arrivals += input.tick();
            let events = self
                .device
                .events
                .as_mut()
                .ok_or_else(|| eyre::eyre!("input requires event controller"))?;
            events.counts[0] = events.counts[0]
                .checked_add(arrivals)
                .ok_or_else(|| eyre::eyre!("input event counter overflow"))?;
        }
        if let Some(events) = &mut self.device.events {
            events.tick()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counted_events_preserve_arrivals_on_the_acknowledgement_edge() -> Result<()> {
        let mut events = Events {
            counts: [2, 1, 0, 0],
            ..Default::default()
        };
        events.schedule.insert(0, 3);
        assert_eq!(events.status(), 3);
        events.acknowledge(3);
        events.tick()?;
        assert_eq!(events.counts, [2, 1, 0, 0]);
        assert_eq!(events.acknowledgements, [1, 1, 0, 0]);
        events.acknowledge(1);
        assert_eq!(events.status(), 3);
        events.acknowledge(3);
        assert_eq!(events.status(), 0);
        assert_eq!(events.acknowledgements, [3, 2, 0, 0]);
        // Acknowledging an empty source neither underflows nor invents delivery.
        events.acknowledge(15);
        assert_eq!(events.status(), 0);
        assert_eq!(events.acknowledgements, [3, 2, 0, 0]);
        Ok(())
    }
}

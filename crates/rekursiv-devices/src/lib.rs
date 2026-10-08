//! External peripherals shared by the RTL harness and native emulator.
//! Addresses and payloads describe device registers, never guest classes,
//! primitives, processes, or object references. Request effects commit only
//! when the consumer accepts the reply.
use eyre::{ensure, Result};
use std::collections::BTreeMap;
mod files;
pub use files::Files;
mod bitmap;
pub use bitmap::{Bitmap, BitmapFrame, CursorMode};
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
    file_effect: Option<files::Effect>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub request_delay: u32,
    pub memory_latency: u32,
    pub response_stall: u32,
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
    /// Disable transaction history for long-running interactive sessions.
    pub discard_history: bool,
    pub fail_next: bool,
    pub events: Option<Events>,
    pub clocks: Option<Clocks>,
    pub pointer: Option<Pointer>,
    pub input: Option<Input>,
    pub cursor_bitmap: Option<Bitmap>,
    pub display_bitmap: Option<Bitmap>,
    pub files: Option<Files>,
    pending: Option<Pending>,
    held: Option<Request>,
    wait: u32,
}
impl Device {
    pub fn workstation(utc_seconds: u64, ticks_per_ms: u64) -> Self {
        // Workstation power-on behavior: the visible cursor follows the
        // physical pointer until software explicitly unlinks it at 0x40c.
        let mut pointer = Pointer::default();
        pointer.linked = true;
        Self {
            discard_history: true,
            events: Some(Events::default()),
            clocks: Some(Clocks::new(utc_seconds, 0, ticks_per_ms)),
            pointer: Some(pointer),
            input: Some(Input::new(1024)),
            cursor_bitmap: Some(Bitmap::new(16, 16)),
            display_bitmap: Some(Bitmap::new(1024, 1024)),
            ..Default::default()
        }
    }

    /// Enqueue a physical key transition and count its notification atomically.
    /// Rejected packets set FIFO overrun without inventing an interrupt.
    pub fn push_input(&mut self, packet: InputPacket) -> Result<bool> {
        let events = self
            .events
            .as_mut()
            .ok_or_else(|| eyre::eyre!("input requires event controller"))?;
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| eyre::eyre!("input device is absent"))?;
        ensure!(events.counts[0] != u32::MAX, "input event counter overflow");
        let accepted = input.push(packet);
        events.counts[0] += u32::from(accepted);
        Ok(accepted)
    }

    pub fn ready(&self) -> bool {
        self.pending.is_none() && self.wait >= self.timing.request_delay
    }
    pub fn response(&self) -> Option<Reply> {
        self.pending.filter(|p| p.delay == 0).map(|p| p.reply)
    }
    /// Advance one device edge. Effects commit only when a reply is consumed.
    /// Both executors use this method, including its acknowledgement/arrival order.
    pub fn tick(&mut self, request: Option<Request>, response_ready: bool) -> Result<()> {
        if let Some(held) = self.held {
            ensure!(
                request == Some(held),
                "device request changed under backpressure"
            );
        }
        let accepted = request.is_some() && self.ready();
        let complete = self.response().is_some() && response_ready;
        self.held = request.filter(|_| !accepted);
        if complete {
            let p = self.pending.take().unwrap();
            if let Some(effect) = p.clock_effect.filter(|_| !p.reply.error) {
                if self.clocks.as_mut().unwrap().commit(effect) {
                    self.events
                        .as_mut()
                        .ok_or_else(|| eyre::eyre!("timer requires event controller"))?
                        .counts[1] = 0;
                }
            } else if let Some(effect) = p.pointer_effect.filter(|_| !p.reply.error) {
                self.pointer.as_mut().unwrap().commit(effect);
            } else if let Some(effect) = p.input_effect.filter(|_| !p.reply.error) {
                self.input.as_mut().unwrap().commit(effect);
            } else if let Some((cursor, effect)) = p.bitmap_effect.filter(|_| !p.reply.error) {
                (if cursor {
                    self.cursor_bitmap.as_mut()
                } else {
                    self.display_bitmap.as_mut()
                })
                .unwrap()
                .commit(effect);
            } else if let Some(effect) = p.file_effect.filter(|_| !p.reply.error) {
                self.files.as_mut().unwrap().commit(effect);
            } else if p.request.write && !p.reply.error {
                match (p.request.address, self.events.as_mut()) {
                    (0x104, Some(events)) => events.acknowledge(p.request.data),
                    _ => {
                        self.registers.insert(p.request.address, p.request.data);
                    }
                }
            }
            if !self.discard_history {
                self.completions.push((p.request, p.reply));
            }
        } else if let Some(p) = &mut self.pending {
            p.delay = p.delay.saturating_sub(1);
        }
        if accepted {
            ensure!(self.pending.is_none(), "second outstanding device request");
            let request = request.expect("accepted request");
            let mut clock_effect = None;
            let mut pointer_effect = None;
            let mut input_effect = None;
            let mut bitmap_effect = None;
            let mut file_effect = None;
            let value = if let Some(clocks) = self
                .clocks
                .as_ref()
                .filter(|_| (0x200..0x220).contains(&request.address))
            {
                clocks.preview(request).map(|(value, effect)| {
                    clock_effect = effect;
                    value
                })
            } else if let Some(pointer) = self.pointer.as_ref().filter(|_| {
                (0x300..0x310).contains(&request.address)
                    || (0x400..0x410).contains(&request.address)
            }) {
                pointer.preview(request).map(|(value, effect)| {
                    pointer_effect = effect;
                    value
                })
            } else if let Some(input) = self
                .input
                .as_ref()
                .filter(|_| (0x310..0x328).contains(&request.address))
            {
                input.preview(request).map(|(value, effect)| {
                    input_effect = effect;
                    value
                })
            } else if (0x410..0x448).contains(&request.address)
                || (0x510..0x548).contains(&request.address)
            {
                let cursor = request.address < 0x500;
                let base = if cursor { 0x410 } else { 0x510 };
                let bitmap = if cursor {
                    self.cursor_bitmap.as_ref()
                } else {
                    self.display_bitmap.as_ref()
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
            } else if let Some(files) = self
                .files
                .as_ref()
                .filter(|_| (0x700..0x750).contains(&request.address))
            {
                files.preview(request).map(|(value, effect)| {
                    file_effect = effect;
                    value
                })
            } else if let Some(events) = &self.events {
                match (request.address, request.write) {
                    (0x100, false) => Some(events.status()),
                    (0x104, true) if request.data & !15 == 0 => Some(0),
                    (0x100 | 0x104, _) => None,
                    _ => self.registers.get(&request.address).copied(),
                }
            } else {
                self.registers.get(&request.address).copied()
            };
            let reply = Reply {
                data: if request.write { 0 } else { value.unwrap_or(0) },
                error: self.fail_next || value.is_none(),
            };
            self.fail_next = false;
            self.pending = Some(Pending {
                request,
                reply,
                delay: self.timing.memory_latency,
                clock_effect,
                pointer_effect,
                input_effect,
                bitmap_effect,
                file_effect,
            });
            if !self.discard_history {
                self.requests.push(request);
            }
            self.wait = 0;
        } else if request.is_some() {
            self.wait += 1;
        } else {
            self.wait = 0;
        }
        if self.clocks.as_mut().is_some_and(|clocks| clocks.tick()) {
            let events = self
                .events
                .as_mut()
                .ok_or_else(|| eyre::eyre!("timer requires event controller"))?;
            events.counts[1] = events.counts[1]
                .checked_add(1)
                .ok_or_else(|| eyre::eyre!("timer event counter overflow"))?;
        }
        let mut arrivals = 0;
        if let Some(pointer) = &mut self.pointer {
            let clock = if self.input.is_some() {
                let clocks = self
                    .clocks
                    .as_ref()
                    .ok_or_else(|| eyre::eyre!("pointer sampling requires a clock"))?;
                Some((clocks.monotonic_ms, clocks.day_milliseconds()))
            } else {
                None
            };
            if let Some(packet) = pointer.tick(clock) {
                arrivals += u32::from(self.input.as_mut().unwrap().push(packet));
            }
        }
        if let Some(input) = &mut self.input {
            arrivals += input.tick();
            let events = self
                .events
                .as_mut()
                .ok_or_else(|| eyre::eyre!("input requires event controller"))?;
            events.counts[0] = events.counts[0]
                .checked_add(arrivals)
                .ok_or_else(|| eyre::eyre!("input event counter overflow"))?;
        }
        if let Some(events) = &mut self.events {
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

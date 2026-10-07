//! A bounded external FIFO of timestamped input packets. This peripheral knows
//! no guest objects or input-word format; the processor converts its packets.
use super::Request;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InputKind {
    Motion = 1,
    KeyDown = 2,
    KeyUp = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputPacket {
    pub kind: InputKind,
    pub timestamp_ms: u32,
    /// Motion: X. Key transition: device key code.
    pub value: i32,
    /// Motion: Y. Unused for key transitions.
    pub extra: i32,
}
#[derive(Clone, Copy)]
pub(super) enum Effect {
    Pop,
}

pub struct Input {
    pub schedule: BTreeMap<u64, Vec<InputPacket>>,
    pub ticks: u64,
    pub overruns: u64,
    capacity: usize,
    queue: VecDeque<InputPacket>,
}
impl Default for Input {
    fn default() -> Self {
        Self::new(32)
    }
}
impl Input {
    pub fn new(capacity: usize) -> Self {
        assert!((1..=65535).contains(&capacity));
        Self {
            schedule: BTreeMap::new(),
            ticks: 0,
            overruns: 0,
            capacity,
            queue: VecDeque::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.queue.len()
    }
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
    /// Non-backpressurable physical input reports overflow rather than silently
    /// replacing an unread packet. Only accepted packets raise notifications.
    pub(super) fn push(&mut self, packet: InputPacket) -> bool {
        if self.queue.len() == self.capacity {
            self.overruns += 1;
            return false;
        }
        self.queue.push_back(packet);
        true
    }
    pub(super) fn preview(&self, request: Request) -> Option<(u32, Option<Effect>)> {
        Some(match (request.address, request.write) {
            (0x310, false) => (
                self.queue.len() as u32 | (u32::from(self.overruns != 0) << 31),
                None,
            ),
            (0x314, false) => (self.queue.front()?.kind as u32, None),
            (0x318, false) => (self.queue.front()?.value as u32, None),
            (0x31c, false) => (self.queue.front()?.extra as u32, None),
            (0x320, false) => (self.queue.front()?.timestamp_ms, None),
            (0x324, true) if request.data == 1 && !self.queue.is_empty() => (0, Some(Effect::Pop)),
            _ => return None,
        })
    }
    pub(super) fn commit(&mut self, effect: Effect) {
        match effect {
            Effect::Pop => {
                self.queue.pop_front().expect("accepted input pop");
            }
        }
    }
    pub(super) fn tick(&mut self) -> u32 {
        let mut accepted = 0;
        if let Some(packets) = self.schedule.remove(&self.ticks) {
            for packet in packets {
                accepted += u32::from(self.push(packet));
            }
        }
        self.ticks += 1;
        accepted
    }
}

//! External clock/timer device model. All values are physical device data;
//! epoch conversion and guest semaphore handling belong to machine code.
use super::Request;

#[derive(Clone, Copy)]
pub(super) enum Effect {
    LatchUtc(u64),
    LatchMonotonic(u64),
    DeadlineLow(u32),
    DeadlineHigh(u32),
    Command(bool),
}

pub struct Clocks {
    pub utc_seconds: u64,
    pub monotonic_ms: u64,
    pub deadline: Option<u64>,
    cycles_per_ms: u64,
    phase: u64,
    utc_fraction: u16,
    utc_latch: u64,
    monotonic_latch: u64,
    staged_deadline: u64,
}
impl Clocks {
    pub fn new(utc_seconds: u64, monotonic_ms: u64, cycles_per_ms: u64) -> Self {
        assert!(cycles_per_ms != 0, "clock divider must be nonzero");
        Self {
            utc_seconds,
            monotonic_ms,
            cycles_per_ms,
            deadline: None,
            phase: 0,
            utc_fraction: 0,
            utc_latch: 0,
            monotonic_latch: 0,
            staged_deadline: 0,
        }
    }
    /// Wall-clock milliseconds since UTC midnight, for timestamped peripherals.
    pub fn day_milliseconds(&self) -> u32 {
        (self.utc_seconds % 86400) as u32 * 1000 + u32::from(self.utc_fraction)
    }
    /// Capture a coherent sample when the request is accepted. The pending
    /// reply owns it through backpressure; errors cannot publish latch changes.
    pub(super) fn preview(&self, request: Request) -> Option<(u32, Option<Effect>)> {
        let (data, effect) = match (request.address, request.write) {
            (0x200, false) => (
                self.utc_seconds as u32,
                Some(Effect::LatchUtc(self.utc_seconds)),
            ),
            (0x204, false) => ((self.utc_latch >> 32) as u32, None),
            (0x208, false) => (
                self.monotonic_ms as u32,
                Some(Effect::LatchMonotonic(self.monotonic_ms)),
            ),
            (0x20c, false) => ((self.monotonic_latch >> 32) as u32, None),
            (0x210, true) => (0, Some(Effect::DeadlineLow(request.data))),
            (0x214, true) => (0, Some(Effect::DeadlineHigh(request.data))),
            (0x218, true) if request.data <= 1 => (0, Some(Effect::Command(request.data != 0))),
            _ => return None,
        };
        Some((data, effect))
    }
    /// Returns true when an arm/cancel command clears the previous expiry.
    pub(super) fn commit(&mut self, effect: Effect) -> bool {
        match effect {
            Effect::LatchUtc(value) => self.utc_latch = value,
            Effect::LatchMonotonic(value) => self.monotonic_latch = value,
            Effect::DeadlineLow(value) => {
                self.staged_deadline = (self.staged_deadline & !0xffff_ffff) | u64::from(value)
            }
            Effect::DeadlineHigh(value) => {
                self.staged_deadline =
                    (self.staged_deadline & 0xffff_ffff) | (u64::from(value) << 32)
            }
            Effect::Command(arm) => {
                self.deadline = arm.then_some(self.staged_deadline);
                return true;
            }
        }
        false
    }
    pub(super) fn tick(&mut self) -> bool {
        self.phase += 1;
        if self.phase == self.cycles_per_ms {
            self.phase = 0;
            self.monotonic_ms = self.monotonic_ms.wrapping_add(1);
            self.utc_fraction += 1;
            if self.utc_fraction == 1000 {
                self.utc_fraction = 0;
                self.utc_seconds = self.utc_seconds.wrapping_add(1);
            }
        }
        if self
            .deadline
            .is_some_and(|deadline| self.monotonic_ms >= deadline)
        {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}

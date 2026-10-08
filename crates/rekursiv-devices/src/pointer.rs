//! External pointing-device registers. Coordinates and sampling configuration
//! are device data; this adapter never constructs guest Points or reads a heap.
use super::{Clocks, InputKind, InputPacket, Request};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum Effect {
    LatchY(i32),
    StageX(i32),
    StageY(i32),
    Publish,
    Link(bool),
    Interval(u32),
}

#[derive(Default)]
pub struct Pointer {
    pub mouse: (i32, i32),
    pub cursor: (i32, i32),
    pub linked: bool,
    pub sample_interval_ms: u32,
    pub schedule: BTreeMap<u64, (i32, i32)>,
    pub ticks: u64,
    latched_y: i32,
    staged: (i32, i32),
    sampled: Option<(i32, i32)>,
    sample_time: u64,
}
impl Pointer {
    pub(super) fn preview(&self, request: Request) -> Option<(u32, Option<Effect>)> {
        Some(match (request.address, request.write) {
            (0x300, false) => (self.mouse.0 as u32, Some(Effect::LatchY(self.mouse.1))),
            (0x304, false) => (self.latched_y as u32, None),
            (0x30c, true) => (0, Some(Effect::Interval(request.data))),
            (0x400, true) => (0, Some(Effect::StageX(request.data as i32))),
            (0x404, true) => (0, Some(Effect::StageY(request.data as i32))),
            (0x408, true) if request.data == 1 => (0, Some(Effect::Publish)),
            (0x40c, true) if request.data <= 1 => (0, Some(Effect::Link(request.data != 0))),
            _ => return None,
        })
    }
    pub(super) fn commit(&mut self, effect: Effect) {
        match effect {
            Effect::LatchY(y) => self.latched_y = y,
            Effect::StageX(x) => self.staged.0 = x,
            Effect::StageY(y) => self.staged.1 = y,
            Effect::Publish => {
                self.cursor = self.staged;
                if self.linked {
                    self.mouse = self.cursor;
                }
            }
            Effect::Link(linked) => {
                self.linked = linked;
                if linked {
                    self.cursor = self.mouse;
                }
            }
            Effect::Interval(value) => self.sample_interval_ms = value,
        }
    }
    pub(super) fn tick(&mut self, clock: Option<&Clocks>) -> Option<InputPacket> {
        if let Some(position) = self.schedule.remove(&self.ticks) {
            self.mouse = position;
            if self.linked {
                self.cursor = position;
            }
        }
        self.ticks += 1;
        let clock = clock?;
        let now = clock.monotonic_ms;
        let Some(previous) = self.sampled else {
            self.sampled = Some(self.mouse);
            self.sample_time = now;
            return None;
        };
        if previous != self.mouse
            && now.wrapping_sub(self.sample_time) >= u64::from(self.sample_interval_ms)
        {
            self.sampled = Some(self.mouse);
            self.sample_time = now;
            Some(InputPacket {
                kind: InputKind::Motion,
                value: self.mouse.0,
                extra: self.mouse.1,
                timestamp_ms: clock.day_milliseconds(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock(now: u64, timestamp: u32) -> Clocks {
        let mut clock = Clocks::new(0, now, 1000);
        clock.set_time(u64::from(timestamp / 1000), (timestamp % 1000) as u16, now);
        clock
    }
    #[test]
    fn sampling_coalesces_motion_until_the_minimum_interval_without_repeating_stationary_positions()
    {
        let mut p = Pointer {
            sample_interval_ms: 2,
            ..Default::default()
        };
        assert_eq!(p.tick(Some(&clock(0, 100))), None);
        p.mouse = (1, 2);
        assert_eq!(p.tick(Some(&clock(1, 101))), None);
        p.mouse = (3, 4);
        let packet = p.tick(Some(&clock(2, 102))).unwrap();
        assert_eq!(
            (packet.kind, packet.value, packet.extra, packet.timestamp_ms),
            (InputKind::Motion, 3, 4, 102)
        );
        assert_eq!(p.tick(Some(&clock(100, 200))), None);
        p.mouse = (5, 6);
        assert!(p.tick(Some(&clock(101, 201))).is_some());
        p.mouse = (7, 8);
        assert_eq!(p.tick(Some(&clock(102, 202))), None);
        p.sample_interval_ms = 0;
        assert!(p.tick(Some(&clock(102, 202))).is_some());
    }
}

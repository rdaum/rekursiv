//! Peripheral graphics work, independent of guest language and window redraws.
use rekursiv_devices::{Bitmap, Device};
use std::{fmt::Write, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BitmapCounters {
    pub updates: u64,
    pub words: u64,
}
impl BitmapCounters {
    fn capture(bitmap: Option<&Bitmap>) -> Self {
        bitmap.map_or(Self::default(), |b| Self {
            updates: b.publications,
            words: b.published_words,
        })
    }
    fn since(self, before: Self) -> Self {
        Self {
            updates: self.updates.saturating_sub(before.updates),
            words: self.words.saturating_sub(before.words),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsCounters {
    pub display: BitmapCounters,
    pub cursor: BitmapCounters,
}
impl GraphicsCounters {
    pub fn capture(device: &Device) -> Self {
        Self {
            display: BitmapCounters::capture(device.display_bitmap.as_ref()),
            cursor: BitmapCounters::capture(device.cursor_bitmap.as_ref()),
        }
    }
    pub fn since(self, before: Self) -> Self {
        Self {
            display: self.display.since(before.display),
            cursor: self.cursor.since(before.cursor),
        }
    }
    pub fn report(self, active: Duration, elapsed: Duration) -> String {
        let mut out = String::new();
        for (name, counters) in [("display", self.display), ("cursor", self.cursor)] {
            writeln!(out,
                "Graphics {name}: {} updates, {} published words; {:.1} updates/s, {:.0} words/s active ({:.1} updates/s, {:.0} words/s elapsed)",
                counters.updates, counters.words,
                rate(counters.updates, active), rate(counters.words, active),
                rate(counters.updates, elapsed), rate(counters.words, elapsed)).unwrap();
        }
        out
    }
}

/// Counts per second; an empty measurement interval has no rate.
pub fn rate(count: u64, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        0.0
    } else {
        count as f64 / elapsed.as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_keep_display_and_cursor_work_separate() {
        let mut device = Device::workstation(0, 1000);
        let before = GraphicsCounters::capture(&device);
        let display = device.display_bitmap.as_mut().unwrap();
        display.publications = 7;
        display.published_words = 19;
        display.pixel_writes = 100; // Pending/abandoned traffic is not published work.
        let cursor = device.cursor_bitmap.as_mut().unwrap();
        cursor.publications = 2;
        cursor.published_words = 32;
        let after = GraphicsCounters::capture(&device);
        assert_eq!(
            after.since(before).display,
            BitmapCounters {
                updates: 7,
                words: 19
            }
        );
        assert_eq!(
            after.since(before).cursor,
            BitmapCounters {
                updates: 2,
                words: 32
            }
        );
        assert_eq!(after.since(after), GraphicsCounters::default());
        assert_eq!(rate(7, Duration::from_secs(2)), 3.5);
        assert_eq!(rate(7, Duration::ZERO), 0.0);
        assert!(!after.report(Duration::ZERO, Duration::ZERO).contains("NaN"));
    }
}

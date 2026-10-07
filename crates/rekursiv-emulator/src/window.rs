//! Native window adapter. Only peripheral state crosses this boundary.
use crate::frontend;
use eyre::Result;
use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use rekursiv_emulator::Machine;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    rc::Rc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// US physical keyboard profile. ASCII codes describe unshifted keys; the
/// guest handles Shift/Ctrl. Special codes follow Xerox V2 InputState class
/// initialize: octal 210/211/212/213. This mapping belongs to the frontend,
/// not the CPU or device FIFO. Unknown physical keys are not synthesized.
fn key_code(key: Key) -> Option<u16> {
    use Key::*;
    Some(match key {
        A => b'a' as u16,
        B => b'b' as u16,
        C => b'c' as u16,
        D => b'd' as u16,
        E => b'e' as u16,
        F => b'f' as u16,
        G => b'g' as u16,
        H => b'h' as u16,
        I => b'i' as u16,
        J => b'j' as u16,
        K => b'k' as u16,
        L => b'l' as u16,
        M => b'm' as u16,
        N => b'n' as u16,
        O => b'o' as u16,
        P => b'p' as u16,
        Q => b'q' as u16,
        R => b'r' as u16,
        S => b's' as u16,
        T => b't' as u16,
        U => b'u' as u16,
        V => b'v' as u16,
        W => b'w' as u16,
        X => b'x' as u16,
        Y => b'y' as u16,
        Z => b'z' as u16,
        Key0 => 48,
        Key1 => 49,
        Key2 => 50,
        Key3 => 51,
        Key4 => 52,
        Key5 => 53,
        Key6 => 54,
        Key7 => 55,
        Key8 => 56,
        Key9 => 57,
        Space => 32,
        Enter => 13,
        Tab => 9,
        Backspace => 8,
        Escape => 27,
        Delete => 127,
        Apostrophe => 39,
        Comma => 44,
        Minus => 45,
        Period => 46,
        Slash => 47,
        Semicolon => 59,
        Equal => 61,
        LeftBracket => 91,
        Backslash => 92,
        RightBracket => 93,
        Backquote => 96,
        LeftShift => 136,
        RightShift => 137,
        LeftCtrl | RightCtrl => 138,
        CapsLock => 139,
        _ => return None,
    })
}
/// Preserve press/release pairs even when both arrive between two frames.
struct KeyEvents(Rc<RefCell<Vec<(Key, bool)>>>);
impl minifb::InputCallback for KeyEvents {
    fn add_char(&mut self, _: u32) {}
    fn set_key_state(&mut self, key: Key, down: bool) {
        self.0.borrow_mut().push((key, down));
    }
}
#[derive(Default)]
struct Keyboard {
    held: BTreeSet<Key>,
    locked: bool,
}
impl Keyboard {
    fn transition(&mut self, key: Key, down: bool) -> Option<(u16, bool)> {
        if !(if down {
            self.held.insert(key)
        } else {
            self.held.remove(&key)
        }) {
            return None;
        }
        let code = key_code(key)?;
        if key == Key::CapsLock {
            if !down {
                return None;
            }
            self.locked = !self.locked;
            return Some((code, self.locked));
        }
        if code == 138
            && self.held.contains(&if key == Key::LeftCtrl {
                Key::RightCtrl
            } else {
                Key::LeftCtrl
            })
        {
            return None;
        }
        Some((code, down))
    }
}
#[derive(Default)]
struct Timing {
    calls: u64,
    total: Duration,
    longest: Duration,
}
impl Timing {
    fn add(&mut self, elapsed: Duration) {
        self.calls += 1;
        self.total += elapsed;
        self.longest = self.longest.max(elapsed);
    }
    fn report(&self, name: &str) {
        eprintln!(
            "  {name}: {} calls, {:.3} s total, {:.3} ms mean, {:.3} ms max",
            self.calls,
            self.total.as_secs_f64(),
            if self.calls == 0 {
                0.0
            } else {
                self.total.as_secs_f64() * 1000.0 / self.calls as f64
            },
            self.longest.as_secs_f64() * 1000.0
        );
    }
}
#[derive(Default)]
struct WindowTiming {
    creation: Timing,
    snapshot: Timing,
    conversion: Timing,
    submission: Timing,
    events: Timing,
    input: Timing,
    controls: Timing,
    pacing: Timing,
    source_bytes: u64,
}
impl WindowTiming {
    fn report(&self) {
        eprintln!("Window timings (concurrent with CPU execution):");
        for (name, timing) in [
            ("creation", &self.creation),
            ("snapshot/repaint check", &self.snapshot),
            ("pixel conversion", &self.conversion),
            ("frame submission + events", &self.submission),
            ("events without upload", &self.events),
            ("input processing", &self.input),
            ("title/cursor controls", &self.controls),
            ("pacing sleep", &self.pacing),
        ] {
            timing.report(name);
        }
        eprintln!(
            "  source pixels submitted: {:.3} MiB (before backend scaling/protocol/compression)",
            self.source_bytes as f64 / 1048576.0
        );
    }
}

pub fn run(
    machine: &mut Machine,
    step: &mut (impl FnMut(&mut Machine) -> Result<bool> + Send),
    frame_limit: Option<u64>,
    execution_time: &mut Duration,
) -> Result<()> {
    let exchange = frontend::Exchange::default();
    // Physical events are ordered and bounded. Presentation has no queue: it
    // consumes the newest immutable snapshot whenever the window is ready.
    let (input, receive) = std::sync::mpsc::sync_channel(128);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| frontend::execute(machine, step, &exchange, receive));
        let stop = frontend::StopOnDrop(&exchange);
        let mut timing = WindowTiming::default();
        let result = render(&exchange, input, frame_limit, &mut timing);
        drop(stop);
        let worker = worker
            .join()
            .map_err(|_| eyre::eyre!("CPU worker panicked"))?;
        *execution_time = worker.timing.execution;
        timing.report();
        eprintln!("CPU worker overhead: {:.3} s input/clock, {:.3} s snapshots ({} published), {} input overruns",
            worker.timing.input_clock.as_secs_f64(), worker.timing.snapshots.as_secs_f64(),
            worker.timing.snapshots_published, worker.timing.input_overruns);
        result.and(worker.result)
    })
}

fn render(
    exchange: &frontend::Exchange,
    input: std::sync::mpsc::SyncSender<frontend::InputBatch>,
    frame_limit: Option<u64>,
    timing: &mut WindowTiming,
) -> Result<()> {
    let creation = Instant::now();
    let mut window = Window::new(
        "Rekursiv",
        640,
        480,
        WindowOptions {
            resize: true,
            scale_mode: minifb::ScaleMode::AspectRatioStretch,
            ..Default::default()
        },
    )?;
    window.set_target_fps(0); // measure our own pacing separately from window calls
    let repaint = crate::repaint::Repaint::new(&window)?;
    timing.creation.add(creation.elapsed());
    let events = Rc::new(RefCell::new(Vec::new()));
    window.set_input_callback(Box::new(KeyEvents(events.clone())));
    let mut keyboard = Keyboard::default();
    let mut previous_keys = BTreeSet::new();
    let mut previous_pointer = None;
    let mut screen: Option<frontend::Screen> = None;
    let mut pixels = Vec::new();
    let mut last_size = (0, 0);
    let mut was_active = false;
    let mut cursor_visible = None;
    let mut stopped = false;
    let mut frames = 0;
    let mut reported_at = Instant::now();
    let mut reported_execution = Duration::ZERO;
    let mut reported_instructions = 0.0;
    while window.is_open() && frame_limit.is_none_or(|limit| frames < limit) {
        let iteration = Instant::now();
        let snapshot_start = Instant::now();
        let snapshot = exchange.snapshot.lock().unwrap().clone();
        let exposed = repaint.requested();
        timing.snapshot.add(snapshot_start.elapsed());
        let (width, height) = snapshot.screen.dimensions();
        let size = window.get_size();
        let active = window.is_active();
        let changed = screen
            .as_ref()
            .is_none_or(|last| !last.same_pixels(&snapshot.screen));
        let mode_changed = screen
            .as_ref()
            .is_none_or(|last| last.dimensions() != (width, height));
        let mut upload = exposed || size != last_size || (active && !was_active) || mode_changed;
        if changed {
            let conversion = Instant::now();
            let converted = snapshot.screen.pixels();
            upload |= converted != pixels;
            pixels = converted;
            timing.conversion.add(conversion.elapsed());
        }
        // Keep a composed buffer for exposes/resizes. Publication of identical
        // pixels and cursor moves outside the display do not require uploads.
        screen = Some(snapshot.screen.clone());
        last_size = size;
        was_active = active;
        let controls = Instant::now();
        let visible = snapshot.screen.cursor.is_none();
        if cursor_visible != Some(visible) {
            window.set_cursor_visibility(visible);
            cursor_visible = Some(visible);
        }
        if !snapshot.running && !stopped {
            window.set_title(&snapshot.error.as_ref().map_or_else(
                || format!("Rekursiv — stopped at micro-PC {}", snapshot.pc),
                |error| format!("Rekursiv — {error}"),
            ));
            stopped = true;
        } else if snapshot.running && reported_at.elapsed() >= Duration::from_secs(1) {
            let instructions =
                snapshot.stats.retired as f64 + snapshot.stats.collector_retired as f64;
            let seconds = (snapshot.execution_time - reported_execution).as_secs_f64();
            let rate = if seconds > 0.0 {
                (instructions - reported_instructions) / seconds
            } else {
                0.0
            };
            window.set_title(&format!(
                "Rekursiv — {:.2} M microinstructions/s",
                rate / 1_000_000.0
            ));
            reported_at = Instant::now();
            reported_execution = snapshot.execution_time;
            reported_instructions = instructions;
        }
        timing.controls.add(controls.elapsed());
        let update = Instant::now();
        if upload {
            // minifb's submission also pumps events, so that work is included
            // in this measurement. An unchanged frame uses events-only update.
            window.update_with_buffer(&pixels, width, height)?;
            timing.submission.add(update.elapsed());
            timing.source_bytes += pixels.len() as u64 * 4;
        } else {
            window.update();
            timing.events.add(update.elapsed());
        }
        let input_start = Instant::now();
        let mut pointer = None;
        if window.is_active() {
            if let Some((x, y)) = window.get_unscaled_mouse_pos(MouseMode::Discard) {
                let (ww, wh) = window.get_size();
                let scale = (ww as f32 / width as f32).min(wh as f32 / height as f32);
                if scale > 0.0 {
                    let px = ((x - (ww as f32 - width as f32 * scale) / 2.0) / scale)
                        .clamp(0.0, width.saturating_sub(1) as f32)
                        as i32;
                    let py = ((y - (wh as f32 - height as f32 * scale) / 2.0) / scale)
                        .clamp(0.0, height.saturating_sub(1) as f32)
                        as i32;
                    if previous_pointer != Some((px, py)) {
                        pointer = Some((px, py));
                        previous_pointer = pointer;
                    }
                }
            }
        }
        let mut transitions = Vec::new();
        for (key, down) in events.borrow_mut().drain(..) {
            if window.is_active() {
                transitions.extend(keyboard.transition(key, down));
            }
        }
        if !window.is_active() {
            previous_pointer = None;
            for key in keyboard.held.clone() {
                transitions.extend(keyboard.transition(key, false));
            }
        }
        let mut keys = BTreeSet::new();
        if window.is_active() {
            for (button, code) in [
                (MouseButton::Left, 130),
                (MouseButton::Middle, 129),
                (MouseButton::Right, 128),
            ] {
                if window.get_mouse_down(button) {
                    keys.insert(code);
                }
            }
        }
        for (set, other, down) in [
            (&previous_keys, &keys, false),
            (&keys, &previous_keys, true),
        ] {
            transitions.extend(set.difference(other).map(|&code| (code, down)));
        }
        previous_keys = keys;
        if snapshot.running && (pointer.is_some() || !transitions.is_empty()) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let timestamp_ms =
                ((now.as_secs() % 86400) * 1000 + u64::from(now.subsec_millis())) as u32;
            match input.try_send(frontend::InputBatch {
                pointer,
                keys: transitions,
                timestamp_ms,
            }) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {} // worker just stopped
                Err(std::sync::mpsc::TrySendError::Full(_)) => eyre::bail!(
                    "frontend input queue full; stopping to avoid losing key transitions"
                ),
            }
        }
        timing.input.add(input_start.elapsed());
        frames += 1;
        // Only the presentation thread sleeps. Slow X11 calls naturally reduce
        // presentation frequency while the CPU continues on its own thread.
        let remaining = frontend::FRAME_INTERVAL.saturating_sub(iteration.elapsed());
        if !remaining.is_zero() {
            let sleep = Instant::now();
            std::thread::sleep(remaining);
            timing.pacing.add(sleep.elapsed());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_keeps_short_presses_aggregates_control_and_toggles_lock() {
        let mut k = Keyboard::default();
        assert_eq!(k.transition(Key::A, true), Some((97, true)));
        assert_eq!(k.transition(Key::A, false), Some((97, false)));
        assert_eq!(k.transition(Key::LeftCtrl, true), Some((138, true)));
        assert_eq!(k.transition(Key::RightCtrl, true), None);
        assert_eq!(k.transition(Key::LeftCtrl, false), None);
        assert_eq!(k.transition(Key::RightCtrl, false), Some((138, false)));
        assert_eq!(k.transition(Key::CapsLock, true), Some((139, true)));
        assert_eq!(k.transition(Key::CapsLock, false), None);
        assert_eq!(k.transition(Key::CapsLock, true), Some((139, false)));
    }
    #[test]
    fn physical_keys_leave_modifiers_to_the_guest() {
        assert_eq!(key_code(Key::A), Some(97));
        assert_eq!(key_code(Key::LeftShift), Some(136));
        assert_eq!(key_code(Key::RightCtrl), Some(138));
        assert_eq!(key_code(Key::Escape), Some(27));
        assert_eq!(key_code(Key::F12), None);
    }
}

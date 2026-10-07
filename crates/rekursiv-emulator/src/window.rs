//! Native window adapter. Only peripheral state crosses this boundary.
use eyre::Result;
use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use rekursiv_emulator::{presentation, Machine};
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
pub fn run(
    machine: &mut Machine,
    step: &mut impl FnMut(&mut Machine) -> Result<bool>,
    frame_limit: Option<u64>,
    execution_time: &mut Duration,
) -> Result<()> {
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
    // Execution itself fills the interval between presentations. A separate
    // frame limiter would sleep away CPU time after each execution slice.
    window.set_target_fps(0);
    let start = Instant::now();
    let events = Rc::new(RefCell::new(Vec::new()));
    window.set_input_callback(Box::new(KeyEvents(events.clone())));
    let mut keyboard = Keyboard::default();
    let mut previous_keys = BTreeSet::new();
    let mut frames = 0;
    let mut running = true;
    let mut failure = None;
    let mut reported_overrun = false;
    let mut reported_at = Instant::now();
    let mut reported_execution = *execution_time;
    let mut reported_instructions = 0.0;
    while window.is_open() && frame_limit.is_none_or(|limit| frames < limit) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        if let Some(clock) = &mut machine.devices.clocks {
            clock.set_time(
                now.as_secs(),
                now.subsec_millis() as u16,
                start.elapsed().as_millis() as u64,
            );
        }
        if running {
            // A wall-time budget keeps slow microcode and collection responsive.
            // CPU execution stays on this thread, so input arrives only between
            // instructions and never observes a half-retired architectural state.
            let budget = Instant::now();
            for n in 0u64.. {
                match step(machine) {
                    Ok(true) => (),
                    Ok(false) => {
                        running = false;
                        break;
                    }
                    Err(error) => {
                        window.set_title(&format!("Rekursiv — {error}"));
                        failure = Some(error);
                        running = false;
                        break;
                    }
                }
                if n % 256 == 0 && budget.elapsed() >= Duration::from_secs_f64(1.0 / 60.0) {
                    break;
                }
            }
            *execution_time += budget.elapsed();
            if running && reported_at.elapsed() >= Duration::from_secs(1) {
                let instructions =
                    machine.stats.retired as f64 + machine.stats.collector_retired as f64;
                let seconds = (*execution_time - reported_execution).as_secs_f64();
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
                reported_execution = *execution_time;
                reported_instructions = instructions;
            }
            if !running {
                // A halted or faulted CPU has no work to fill the frame budget.
                window.set_target_fps(60);
            }
            if !running && failure.is_none() {
                window.set_title(&format!(
                    "Rekursiv — stopped at micro-PC {}",
                    machine.cpu.pc
                ));
            }
        }
        let display = machine
            .devices
            .display_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref());
        let (width, height) = display.map_or((640, 480), |f| (f.width as usize, f.height as usize));
        let cursor = machine
            .devices
            .cursor_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref())
            .zip(machine.devices.pointer.as_ref().map(|p| p.cursor));
        let buffer = display.map_or_else(
            || vec![0xffffff; width * height],
            |f| presentation::pixels(f, cursor),
        );
        window.set_cursor_visibility(cursor.is_none());
        window.update_with_buffer(&buffer, width, height)?;
        if window.is_active() {
            // Map the resizable window to the aspect-preserved scanout rectangle.
            // This also handles a display mode smaller than the initial window.
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
                    presentation::pointer(&mut machine.devices, px, py);
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
            for key in keyboard.held.clone() {
                transitions.extend(keyboard.transition(key, false));
            }
        }
        let mut keys = BTreeSet::new();
        if window.is_active() {
            // Red/left = bit 2, yellow/middle = bit 1, blue/right = bit 0.
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
        for (code, down) in transitions {
            if !presentation::key(&mut machine.devices, code, down)? && !reported_overrun {
                eprintln!("input FIFO overrun; guest did not consume input quickly enough");
                reported_overrun = true;
            }
        }
        previous_keys = keys;
        frames += 1;
    }
    failure.map_or(Ok(()), Err)
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

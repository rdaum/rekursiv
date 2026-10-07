//! Event-driven window adapter. Only immutable peripheral state crosses here.
//! The CPU worker never waits for presentation or owns a platform window.
use crate::frontend;
use eyre::Result;
use rekursiv_emulator::Machine;
use softbuffer::{Context, Surface};
use std::{
    collections::BTreeSet,
    num::NonZeroU32,
    sync::{mpsc::SyncSender, Arc},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

/// US physical keyboard profile. ASCII codes describe unshifted keys; the
/// guest handles Shift/Ctrl. Special codes follow Xerox V2 InputState class
/// initialize: octal 210/211/212/213. This mapping belongs to the frontend,
/// not the CPU or device FIFO. Unknown physical keys are not synthesized.
fn key_code(key: KeyCode) -> Option<u16> {
    use KeyCode::*;
    Some(match key {
        KeyA => b'a' as u16,
        KeyB => b'b' as u16,
        KeyC => b'c' as u16,
        KeyD => b'd' as u16,
        KeyE => b'e' as u16,
        KeyF => b'f' as u16,
        KeyG => b'g' as u16,
        KeyH => b'h' as u16,
        KeyI => b'i' as u16,
        KeyJ => b'j' as u16,
        KeyK => b'k' as u16,
        KeyL => b'l' as u16,
        KeyM => b'm' as u16,
        KeyN => b'n' as u16,
        KeyO => b'o' as u16,
        KeyP => b'p' as u16,
        KeyQ => b'q' as u16,
        KeyR => b'r' as u16,
        KeyS => b's' as u16,
        KeyT => b't' as u16,
        KeyU => b'u' as u16,
        KeyV => b'v' as u16,
        KeyW => b'w' as u16,
        KeyX => b'x' as u16,
        KeyY => b'y' as u16,
        KeyZ => b'z' as u16,
        Digit0 => 48,
        Digit1 => 49,
        Digit2 => 50,
        Digit3 => 51,
        Digit4 => 52,
        Digit5 => 53,
        Digit6 => 54,
        Digit7 => 55,
        Digit8 => 56,
        Digit9 => 57,
        Space => 32,
        Enter => 13,
        Tab => 9,
        Backspace => 8,
        Escape => 27,
        Delete => 127,
        Quote => 39,
        Comma => 44,
        Minus => 45,
        Period => 46,
        Slash => 47,
        Semicolon => 59,
        Equal => 61,
        BracketLeft => 91,
        Backslash => 92,
        BracketRight => 93,
        Backquote => 96,
        ShiftLeft => 136,
        ShiftRight => 137,
        ControlLeft | ControlRight => 138,
        CapsLock => 139,
        _ => return None,
    })
}
#[derive(Default)]
struct Keyboard {
    held: BTreeSet<KeyCode>,
    locked: bool,
}
impl Keyboard {
    fn transition(&mut self, key: KeyCode, down: bool) -> Option<(u16, bool)> {
        if !(if down {
            self.held.insert(key)
        } else {
            self.held.remove(&key)
        }) {
            return None;
        }
        let code = key_code(key)?;
        if key == KeyCode::CapsLock {
            if !down {
                return None;
            }
            self.locked = !self.locked;
            return Some((code, self.locked));
        }
        if code == 138
            && self.held.contains(&if key == KeyCode::ControlLeft {
                KeyCode::ControlRight
            } else {
                KeyCode::ControlLeft
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
    waiting: Timing,
    scaling: Timing,
    checks: u64,
    unchanged: u64,
    source_bytes: u64,
}
impl WindowTiming {
    fn report(&self) {
        eprintln!("Window timings (concurrent with CPU execution):");
        for (name, timing) in [
            ("creation", &self.creation),
            ("snapshot check", &self.snapshot),
            ("pixel conversion", &self.conversion),
            ("buffer acquisition + submission", &self.submission),
            ("window event callbacks (includes redraw)", &self.events),
            ("input processing", &self.input),
            ("title/cursor controls", &self.controls),
            ("frame scaling/copy", &self.scaling),
            ("event-loop wait", &self.waiting),
        ] {
            timing.report(name);
        }
        eprintln!(
            "  presentation checks: {}, unchanged: {}",
            self.checks, self.unchanged
        );
        eprintln!(
            "  surface pixels submitted: {:.3} MiB (before protocol/compression)",
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
        let result = (|| -> Result<()> {
            let created = Instant::now();
            let event_loop = EventLoop::new()?;
            timing.creation.add(created.elapsed());
            let mut app = App::new(&exchange, input, frame_limit, &mut timing);
            event_loop.run_app(&mut app)?;
            app.error.map_or(Ok(()), Err)
        })();
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

// Softbuffer errors can contain non-Send raw platform handles. Keep those on
// the window thread, and propagate their diagnostic text through eyre.
fn surface_error(error: softbuffer::SoftBufferError) -> eyre::Report {
    eyre::eyre!("framebuffer: {error}")
}

/// One viewport calculation serves both drawing and input. Coordinates are
/// physical window pixels, including on a scaled/high-DPI desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Viewport {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
impl Viewport {
    fn fit(size: PhysicalSize<u32>, source: (usize, usize)) -> Option<Self> {
        if size.width == 0 || size.height == 0 {
            return None;
        }
        let (sw, sh) = (source.0 as u64, source.1 as u64);
        let (ww, wh) = (u64::from(size.width), u64::from(size.height));
        let (width, height) = if ww * sh <= wh * sw {
            (size.width, (ww * sh / sw).max(1) as u32)
        } else {
            ((wh * sw / sh).max(1) as u32, size.height)
        };
        Some(Self {
            x: (size.width - width) / 2,
            y: (size.height - height) / 2,
            width,
            height,
        })
    }
    fn pointer(self, position: PhysicalPosition<f64>, source: (usize, usize)) -> (i32, i32) {
        let x = ((position.x - f64::from(self.x)) * source.0 as f64 / f64::from(self.width))
            .floor()
            .clamp(0.0, source.0.saturating_sub(1) as f64);
        let y = ((position.y - f64::from(self.y)) * source.1 as f64 / f64::from(self.height))
            .floor()
            .clamp(0.0, source.1.saturating_sub(1) as f64);
        (x as i32, y as i32)
    }
    fn draw(self, source: &[u32], dimensions: (usize, usize), target: &mut [u32], stride: usize) {
        target.fill(0); // letterbox outside the guest display
        for y in 0..self.height as usize {
            let sy = y * dimensions.1 / self.height as usize;
            let row = (self.y as usize + y) * stride + self.x as usize;
            for x in 0..self.width as usize {
                let sx = x * dimensions.0 / self.width as usize;
                target[row + x] = source[sy * dimensions.0 + sx];
            }
        }
    }
}

fn mouse_code(button: MouseButton) -> Option<u16> {
    match button {
        MouseButton::Left => Some(130),
        MouseButton::Middle => Some(129),
        MouseButton::Right => Some(128),
        _ => None,
    }
}

#[derive(Default)]
struct InputState {
    keyboard: Keyboard,
    buttons: BTreeSet<u16>,
}
impl InputState {
    fn button(&mut self, button: MouseButton, down: bool) -> Option<(u16, bool)> {
        let code = mouse_code(button)?;
        let changed = if down {
            self.buttons.insert(code)
        } else {
            self.buttons.remove(&code)
        };
        changed.then_some((code, down))
    }
    fn release(&mut self) -> Vec<(u16, bool)> {
        let mut transitions = Vec::new();
        for key in self.keyboard.held.clone() {
            transitions.extend(self.keyboard.transition(key, false));
        }
        transitions.extend(
            std::mem::take(&mut self.buttons)
                .into_iter()
                .map(|code| (code, false)),
        );
        transitions
    }
}

struct App<'a> {
    exchange: &'a frontend::Exchange,
    input: SyncSender<frontend::InputBatch>,
    // The surface retains its window/display handles; drop it before Window.
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    window: Option<Arc<Window>>,
    size: PhysicalSize<u32>,
    screen: Option<frontend::Screen>,
    pixels: Vec<u32>,
    running: bool,
    focused: bool,
    cursor_visible: Option<bool>,
    input_state: InputState,
    mouse: Option<PhysicalPosition<f64>>,
    previous_pointer: Option<(i32, i32)>,
    pending_pointer: Option<(i32, i32)>,
    next_frame: Instant,
    frame_limit: Option<u64>,
    frames: u64,
    reported_at: Instant,
    reported_execution: Duration,
    reported_instructions: f64,
    stopped: bool,
    waiting_since: Option<Instant>,
    timing: &'a mut WindowTiming,
    error: Option<eyre::Report>,
}
impl<'a> App<'a> {
    fn new(
        exchange: &'a frontend::Exchange,
        input: SyncSender<frontend::InputBatch>,
        frame_limit: Option<u64>,
        timing: &'a mut WindowTiming,
    ) -> Self {
        Self {
            exchange,
            input,
            surface: None,
            window: None,
            size: PhysicalSize::<u32>::from(rekursiv_emulator::boot::DEFAULT_DISPLAY_SIZE),
            screen: None,
            pixels: Vec::new(),
            running: true,
            focused: false,
            cursor_visible: None,
            input_state: InputState::default(),
            mouse: None,
            previous_pointer: None,
            pending_pointer: None,
            next_frame: Instant::now(),
            frame_limit,
            frames: 0,
            reported_at: Instant::now(),
            reported_execution: Duration::ZERO,
            reported_instructions: 0.0,
            stopped: false,
            waiting_since: None,
            timing,
            error: None,
        }
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: eyre::Report) {
        self.error.get_or_insert(error);
        event_loop.exit();
    }
    fn dimensions(&self) -> (usize, usize) {
        self.screen.as_ref().map_or_else(
            || frontend::Screen::default().dimensions(),
            frontend::Screen::dimensions,
        )
    }
    fn map_pointer(&mut self) {
        if !self.focused {
            return;
        }
        if let (Some(mouse), Some(viewport)) =
            (self.mouse, Viewport::fit(self.size, self.dimensions()))
        {
            let pointer = viewport.pointer(mouse, self.dimensions());
            if self.previous_pointer != Some(pointer) {
                self.pending_pointer = Some(pointer);
                self.previous_pointer = Some(pointer);
            }
        }
    }
    fn send_input(&mut self, keys: Vec<(u16, bool)>) -> Result<()> {
        let pointer = self.pending_pointer.take();
        if !self.running || (pointer.is_none() && keys.is_empty()) {
            return Ok(());
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let timestamp_ms = ((now.as_secs() % 86400) * 1000 + u64::from(now.subsec_millis())) as u32;
        match self.input.try_send(frontend::InputBatch {
            pointer,
            keys,
            timestamp_ms,
        }) {
            Ok(()) | Err(std::sync::mpsc::TrySendError::Disconnected(_)) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                eyre::bail!("frontend input queue full; stopping to avoid losing key transitions")
            }
        }
    }
    fn sample(&mut self) -> bool {
        let started = Instant::now();
        let snapshot = self.exchange.snapshot.lock().unwrap().clone();
        self.timing.snapshot.add(started.elapsed());
        let changed = self
            .screen
            .as_ref()
            .is_none_or(|s| !s.same_pixels(&snapshot.screen));
        let mode_changed = self.dimensions() != snapshot.screen.dimensions();
        let mut redraw = self.screen.is_none() || mode_changed;
        if changed {
            let started = Instant::now();
            let pixels = snapshot.screen.pixels();
            redraw |= pixels != self.pixels;
            self.pixels = pixels;
            self.timing.conversion.add(started.elapsed());
        }
        self.screen = Some(snapshot.screen);
        self.running = snapshot.running;
        let started = Instant::now();
        let window = self.window.as_ref().unwrap();
        let visible = self.screen.as_ref().unwrap().cursor.is_none();
        if self.cursor_visible != Some(visible) {
            window.set_cursor_visible(visible);
            self.cursor_visible = Some(visible);
        }
        if !snapshot.running && !self.stopped {
            window.set_title(&snapshot.error.as_ref().map_or_else(
                || format!("Rekursiv — stopped at micro-PC {}", snapshot.pc),
                |error| format!("Rekursiv — {error}"),
            ));
            self.stopped = true;
        } else if snapshot.running && self.reported_at.elapsed() >= Duration::from_secs(1) {
            let instructions =
                snapshot.stats.retired as f64 + snapshot.stats.collector_retired as f64;
            let seconds = (snapshot.execution_time - self.reported_execution).as_secs_f64();
            let rate = if seconds > 0.0 {
                (instructions - self.reported_instructions) / seconds
            } else {
                0.0
            };
            window.set_title(&format!(
                "Rekursiv — {:.2} M microinstructions/s",
                rate / 1_000_000.0
            ));
            self.reported_at = Instant::now();
            self.reported_execution = snapshot.execution_time;
            self.reported_instructions = instructions;
        }
        self.timing.controls.add(started.elapsed());
        if !redraw {
            self.timing.unchanged += 1;
        }
        self.timing.checks += 1;
        self.frames += 1;
        if mode_changed {
            self.map_pointer();
        }
        redraw
    }
    fn draw(&mut self) -> Result<()> {
        if self.screen.is_none() {
            self.sample();
            self.next_frame = Instant::now() + frontend::FRAME_INTERVAL;
        }
        let dimensions = self.dimensions();
        let Some(viewport) = Viewport::fit(self.size, dimensions) else {
            return Ok(());
        };
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        let started = Instant::now();
        surface
            .resize(
                NonZeroU32::new(self.size.width).unwrap(),
                NonZeroU32::new(self.size.height).unwrap(),
            )
            .map_err(surface_error)?;
        let mut buffer = surface.buffer_mut().map_err(surface_error)?;
        let acquisition = started.elapsed();
        let started = Instant::now();
        // Fill the whole returned buffer: buffer age may change on Wayland,
        // and an OS repaint must restore pixels even without a new guest frame.
        viewport.draw(
            &self.pixels,
            dimensions,
            &mut buffer,
            self.size.width as usize,
        );
        self.timing.scaling.add(started.elapsed());
        self.window.as_ref().unwrap().pre_present_notify();
        let started = Instant::now();
        buffer.present().map_err(surface_error)?;
        self.timing.submission.add(acquisition + started.elapsed());
        self.timing.source_bytes += u64::from(self.size.width) * u64::from(self.size.height) * 4;
        Ok(())
    }
    fn event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw()?,
            WindowEvent::Resized(size) => {
                self.size = size;
                self.map_pointer();
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if focused {
                    self.map_pointer();
                    self.window.as_ref().unwrap().request_redraw();
                } else {
                    self.previous_pointer = None;
                    let releases = self.input_state.release();
                    self.send_input(releases)?;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = Some(position);
                self.map_pointer();
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse = None;
            }
            WindowEvent::KeyboardInput { event, .. } if self.focused => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    if let Some(transition) = self
                        .input_state
                        .keyboard
                        .transition(key, event.state.is_pressed())
                    {
                        self.send_input(vec![transition])?;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } if self.focused => {
                if let Some(transition) = self
                    .input_state
                    .button(button, state == ElementState::Pressed)
                {
                    self.send_input(vec![transition])?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if self.frame_limit == Some(0) {
            event_loop.exit();
            return;
        }
        let started = Instant::now();
        let result = (|| -> Result<()> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Rekursiv")
                        .with_inner_size(PhysicalSize::<u32>::from(
                            rekursiv_emulator::boot::DEFAULT_DISPLAY_SIZE,
                        )),
                )?,
            );
            let context = Context::new(window.clone()).map_err(surface_error)?;
            self.surface = Some(Surface::new(&context, window.clone()).map_err(surface_error)?);
            self.size = window.inner_size();
            self.focused = window.has_focus();
            self.window = Some(window);
            self.next_frame = Instant::now();
            Ok(())
        })();
        self.timing.creation.add(started.elapsed());
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn new_events(&mut self, _: &ActiveEventLoop, _: StartCause) {
        if let Some(started) = self.waiting_since.take() {
            self.timing.waiting.add(started.elapsed());
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        let started = Instant::now();
        if let Err(error) = self.event(event_loop, event) {
            self.fail(event_loop, error);
        }
        self.timing.events.add(started.elapsed());
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }
        if self.window.is_some() && Instant::now() >= self.next_frame {
            if self.frame_limit.is_some_and(|limit| self.frames >= limit) {
                event_loop.exit();
                return;
            }
            if self.sample() {
                self.window.as_ref().unwrap().request_redraw();
            }
            self.next_frame = Instant::now() + frontend::FRAME_INTERVAL;
        }
        let started = Instant::now();
        if let Err(error) = self.send_input(Vec::new()) {
            self.fail(event_loop, error);
        }
        self.timing.input.add(started.elapsed());
        // This timer checks local snapshots; it makes no pointer query or
        // window submission. Input events wake the loop before the deadline.
        event_loop.set_control_flow(if self.window.is_some() {
            ControlFlow::WaitUntil(self.next_frame)
        } else {
            ControlFlow::Wait
        });
        self.waiting_since = Some(Instant::now());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_keeps_short_presses_aggregates_control_and_toggles_lock() {
        let mut k = Keyboard::default();
        assert_eq!(k.transition(KeyCode::KeyA, true), Some((97, true)));
        assert_eq!(k.transition(KeyCode::KeyA, false), Some((97, false)));
        assert_eq!(k.transition(KeyCode::ControlLeft, true), Some((138, true)));
        assert_eq!(k.transition(KeyCode::ControlRight, true), None);
        assert_eq!(k.transition(KeyCode::ControlLeft, false), None);
        assert_eq!(
            k.transition(KeyCode::ControlRight, false),
            Some((138, false))
        );
        assert_eq!(k.transition(KeyCode::CapsLock, true), Some((139, true)));
        assert_eq!(k.transition(KeyCode::CapsLock, false), None);
        assert_eq!(k.transition(KeyCode::CapsLock, true), Some((139, false)));
    }
    #[test]
    fn physical_keys_leave_modifiers_to_the_guest() {
        assert_eq!(key_code(KeyCode::KeyA), Some(97));
        assert_eq!(key_code(KeyCode::ShiftLeft), Some(136));
        assert_eq!(key_code(KeyCode::ControlRight), Some(138));
        assert_eq!(key_code(KeyCode::Escape), Some(27));
        assert_eq!(key_code(KeyCode::F12), None);
    }
    #[test]
    fn button_transitions_and_focus_loss_release_physical_state_once() {
        let mut input = InputState::default();
        assert_eq!(input.button(MouseButton::Left, true), Some((130, true)));
        assert_eq!(input.button(MouseButton::Left, false), Some((130, false)));
        assert_eq!(input.button(MouseButton::Left, false), None);
        assert_eq!(input.button(MouseButton::Right, true), Some((128, true)));
        assert_eq!(
            input.keyboard.transition(KeyCode::ControlLeft, true),
            Some((138, true))
        );
        assert_eq!(input.keyboard.transition(KeyCode::ControlRight, true), None);
        assert_eq!(
            input.keyboard.transition(KeyCode::KeyA, true),
            Some((97, true))
        );
        assert_eq!(input.keyboard.transition(KeyCode::KeyA, true), None); // key repeat
        let mut releases = input.release();
        releases.sort();
        assert_eq!(releases, [(97, false), (128, false), (138, false)]);
        assert!(input.release().is_empty());
    }

    #[test]
    fn scaled_pixels_and_mouse_coordinates_use_the_same_viewport() {
        let source = [1, 2, 3, 4, 5, 6];
        for size in [
            PhysicalSize::new(8, 8),
            PhysicalSize::new(12, 3),
            PhysicalSize::new(1, 1),
        ] {
            let viewport = Viewport::fit(size, (3, 2)).unwrap();
            let mut pixels = vec![99; (size.width * size.height) as usize];
            viewport.draw(&source, (3, 2), &mut pixels, size.width as usize);
            for y in 0..size.height {
                for x in 0..size.width {
                    let pixel = pixels[(y * size.width + x) as usize];
                    if (viewport.x..viewport.x + viewport.width).contains(&x)
                        && (viewport.y..viewport.y + viewport.height).contains(&y)
                    {
                        let (sx, sy) = viewport
                            .pointer(PhysicalPosition::new(f64::from(x), f64::from(y)), (3, 2));
                        assert_eq!(pixel, source[sy as usize * 3 + sx as usize]);
                    } else {
                        assert_eq!(pixel, 0);
                    }
                }
            }
            assert_eq!(
                viewport.pointer(PhysicalPosition::new(-100.0, -100.0), (3, 2)),
                (0, 0)
            );
            assert_eq!(
                viewport.pointer(PhysicalPosition::new(100.0, 100.0), (3, 2)),
                (2, 1)
            );
        }
        assert!(Viewport::fit(PhysicalSize::new(0, 480), (640, 480)).is_none());
        assert!(Viewport::fit(PhysicalSize::new(640, 0), (640, 480)).is_none());
    }
}

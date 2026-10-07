//! CPU/window exchange. No live CPU or heap state is shared with the window.
//!
//! Only the worker owns Machine. The window receives immutable peripheral
//! snapshots, and sends physical input in order. A single snapshot slot keeps
//! memory bounded when presentation stalls; intermediate frames are replaced.
use eyre::Result;
use rekursiv_devices::{Bitmap, BitmapFrame, InputKind, InputPacket};
use rekursiv_emulator::{boot::DEFAULT_DISPLAY_SIZE, presentation, Machine, Statistics};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
const CPU_SLICE: Duration = Duration::from_millis(2);

#[derive(Clone, Default)]
pub struct Screen {
    pub display: Option<Arc<BitmapFrame>>,
    pub cursor: Option<Arc<BitmapFrame>>,
    pub position: (i32, i32),
}
impl Screen {
    pub fn dimensions(&self) -> (usize, usize) {
        self.display.as_ref().map_or(
            (
                DEFAULT_DISPLAY_SIZE.0 as usize,
                DEFAULT_DISPLAY_SIZE.1 as usize,
            ),
            |f| (f.width as usize, f.height as usize),
        )
    }
    pub fn same_pixels(&self, other: &Self) -> bool {
        fn same(a: &Option<Arc<BitmapFrame>>, b: &Option<Arc<BitmapFrame>>) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
        }
        same(&self.display, &other.display)
            && same(&self.cursor, &other.cursor)
            && (self.cursor.is_none() || self.position == other.position)
    }
    pub fn pixels(&self) -> Vec<u32> {
        let (width, height) = self.dimensions();
        self.display.as_ref().map_or_else(
            || vec![0xffffff; width * height],
            |f| presentation::pixels(f, self.cursor.as_deref().map(|c| (c, self.position))),
        )
    }
}

#[derive(Clone)]
pub struct Snapshot {
    pub screen: Screen,
    pub stats: Statistics,
    pub execution_time: Duration,
    pub running: bool,
    pub pc: u16,
    pub error: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            screen: Screen::default(),
            stats: Statistics::default(),
            execution_time: Duration::ZERO,
            running: true,
            pc: 0,
            error: None,
        }
    }
}

pub struct Exchange {
    pub snapshot: Mutex<Snapshot>,
    pub stop: AtomicBool,
}
impl Default for Exchange {
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(Snapshot::default()),
            stop: AtomicBool::new(false),
        }
    }
}
// Install after spawning, so every window exit (including unwinding) stops
// the worker before a scoped-thread join can wait for it.
pub struct StopOnDrop<'a>(pub &'a Exchange);
impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Relaxed);
    }
}

pub struct InputBatch {
    pub pointer: Option<(i32, i32)>,
    pub keys: Vec<(u16, bool)>,
    pub timestamp_ms: u32,
}
impl InputBatch {
    fn apply(self, machine: &mut Machine) -> Result<u64> {
        if let Some((x, y)) = self.pointer {
            presentation::pointer(&mut machine.devices, x, y);
        }
        let mut overruns = 0;
        for (code, down) in self.keys {
            let accepted = machine.devices.push_input(InputPacket {
                kind: if down {
                    InputKind::KeyDown
                } else {
                    InputKind::KeyUp
                },
                timestamp_ms: self.timestamp_ms,
                value: i32::from(code),
                extra: 0,
            })?;
            overruns += u64::from(!accepted);
        }
        Ok(overruns)
    }
}

#[derive(Default)]
struct BitmapCache {
    generation: Option<u64>,
    frame: Option<Arc<BitmapFrame>>,
}
impl BitmapCache {
    fn capture(&mut self, bitmap: Option<&Bitmap>) -> Option<Arc<BitmapFrame>> {
        let generation = bitmap.map(|b| b.publications);
        if self.generation != generation {
            let visible = bitmap.and_then(|b| b.visible.as_ref());
            // A guest can publish identical pixels. Keep the Arc identity so
            // the window skips conversion and upload in that case too.
            if self.frame.as_deref() != visible {
                self.frame = visible.cloned().map(Arc::new);
            }
            self.generation = generation;
        }
        self.frame.clone()
    }
}

#[derive(Default)]
pub struct WorkerTiming {
    pub execution: Duration,
    pub input_clock: Duration,
    pub snapshots: Duration,
    pub snapshots_published: u64,
    pub input_overruns: u64,
}
pub struct WorkerExit {
    pub timing: WorkerTiming,
    pub result: Result<()>,
}

pub fn execute(
    machine: &mut Machine,
    step: &mut impl FnMut(&mut Machine) -> Result<bool>,
    exchange: &Exchange,
    input: Receiver<InputBatch>,
) -> WorkerExit {
    let mut timing = WorkerTiming::default();
    let start = Instant::now();
    let mut published_at = start;
    let mut display = BitmapCache::default();
    let mut cursor = BitmapCache::default();
    let result = (|| -> Result<()> {
        let mut running = true;
        while running && !exchange.stop.load(Ordering::Relaxed) {
            let input_start = Instant::now();
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            if let Some(clock) = &mut machine.devices.clocks {
                clock.set_time(
                    now.as_secs(),
                    now.subsec_millis() as u16,
                    start.elapsed().as_millis() as u64,
                );
            }
            for batch in input.try_iter() {
                timing.input_overruns += batch.apply(machine)?;
            }
            timing.input_clock += input_start.elapsed();
            let budget = Instant::now();
            let mut failure = None;
            for n in 0u64.. {
                match step(machine) {
                    Ok(true) => {}
                    Ok(false) => {
                        running = false;
                        break;
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
                if n % 256 == 0 && budget.elapsed() >= CPU_SLICE {
                    break;
                }
            }
            timing.execution += budget.elapsed();
            if let Some(error) = failure {
                return Err(error);
            }
            if published_at.elapsed() >= FRAME_INTERVAL {
                publish(
                    machine,
                    exchange,
                    &mut display,
                    &mut cursor,
                    &mut timing,
                    running,
                    None,
                );
                published_at = Instant::now();
            }
        }
        Ok(())
    })();
    publish(
        machine,
        exchange,
        &mut display,
        &mut cursor,
        &mut timing,
        false,
        result.as_ref().err().map(|e| format!("{e:#}")),
    );
    WorkerExit { timing, result }
}

fn publish(
    machine: &Machine,
    exchange: &Exchange,
    display: &mut BitmapCache,
    cursor: &mut BitmapCache,
    timing: &mut WorkerTiming,
    running: bool,
    error: Option<String>,
) {
    let started = Instant::now();
    // Clone pixel storage outside the mutex. The window holds this lock only
    // long enough to clone Arcs; it never holds it during any window call.
    let screen = Screen {
        display: display.capture(machine.devices.display_bitmap.as_ref()),
        cursor: if machine.devices.pointer.is_some() {
            cursor.capture(machine.devices.cursor_bitmap.as_ref())
        } else {
            None
        },
        position: machine
            .devices
            .pointer
            .as_ref()
            .map_or((0, 0), |p| p.cursor),
    };
    *exchange.snapshot.lock().unwrap() = Snapshot {
        screen,
        stats: machine.stats,
        execution_time: timing.execution,
        running,
        pc: machine.cpu.pc,
        error,
    };
    timing.snapshots += started.elapsed();
    timing.snapshots_published += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rekursiv_emulator::{boot, Step};
    use std::sync::mpsc;

    #[test]
    fn published_frames_are_immutable_and_identical_publications_share_storage() {
        let mut bitmap = Bitmap::new(32, 1);
        bitmap.visible = Some(BitmapFrame {
            width: 32,
            height: 1,
            stride: 1,
            words: vec![0],
        });
        bitmap.publications = 1;
        let mut cache = BitmapCache::default();
        let first = cache.capture(Some(&bitmap)).unwrap();
        bitmap.publications += 1;
        let identical = cache.capture(Some(&bitmap)).unwrap();
        assert!(Arc::ptr_eq(&first, &identical));
        bitmap.visible.as_mut().unwrap().words[0] = 0x8000_0000;
        bitmap.publications += 1;
        let changed = cache.capture(Some(&bitmap)).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_eq!(first.words, [0]);
        assert_eq!(changed.words, [0x8000_0000]);
        assert!(cache.capture(None).is_none());
    }

    #[test]
    fn pointer_motion_only_changes_pixels_when_a_cursor_is_present() {
        let mut screen = Screen::default();
        let first = screen.clone();
        screen.position = (10, 20);
        assert!(screen.same_pixels(&first));
        screen.cursor = Some(Arc::new(BitmapFrame {
            width: 1,
            height: 1,
            stride: 1,
            words: vec![0x8000_0000],
        }));
        let with_cursor = screen.clone();
        screen.position = (11, 20);
        assert!(!screen.same_pixels(&with_cursor));
    }

    #[test]
    fn workstation_cursor_motion_repaints_without_a_new_bitmap_and_respects_unlink() -> Result<()> {
        let mut machine =
            boot::microcode("d=0x40c, r=Bus, rb=0, ldrb\nd=0, ra=0, io=Write\nhalt", 512)?.machine;
        machine.devices = presentation::workstation(0, 1000);
        for (bitmap, width, height, words) in [
            (
                machine.devices.display_bitmap.as_mut().unwrap(),
                2,
                2,
                vec![0, 0],
            ),
            (
                machine.devices.cursor_bitmap.as_mut().unwrap(),
                1,
                1,
                vec![0x8000_0000],
            ),
        ] {
            bitmap.visible = Some(BitmapFrame {
                width,
                height,
                stride: 1,
                words,
            });
            bitmap.publications = 1;
        }
        let exchange = Exchange::default();
        let mut display = BitmapCache::default();
        let mut cursor = BitmapCache::default();
        let mut timing = WorkerTiming::default();
        let mut capture = |machine: &Machine| {
            publish(
                machine,
                &exchange,
                &mut display,
                &mut cursor,
                &mut timing,
                true,
                None,
            );
            exchange.snapshot.lock().unwrap().screen.clone()
        };
        let first = capture(&machine);
        assert_eq!(first.pixels(), [0, 0xffffff, 0xffffff, 0xffffff]);
        presentation::pointer(&mut machine.devices, 1, 1);
        let moved = capture(&machine);
        assert!(!first.same_pixels(&moved));
        assert!(Arc::ptr_eq(
            first.cursor.as_ref().unwrap(),
            moved.cursor.as_ref().unwrap()
        ));
        assert_eq!(moved.pixels(), [0xffffff, 0xffffff, 0xffffff, 0]);
        // A guest can deliberately keep its cursor separate from the mouse.
        while machine.step()? != Step::Halted {}
        presentation::pointer(&mut machine.devices, 0, 1);
        let unlinked = capture(&machine);
        assert_eq!(machine.devices.pointer.as_ref().unwrap().mouse, (0, 1));
        assert!(moved.same_pixels(&unlinked));
        assert_eq!(unlinked.pixels(), moved.pixels());
        Ok(())
    }

    #[test]
    fn worker_finishes_without_a_render_consumer_and_matches_direct_execution() -> Result<()> {
        let source = include_str!("../../../microcode/collection.uc");
        let mut direct = boot::microcode_with_pager(source, 512, 16)?.machine;
        while !matches!(direct.step()?, Step::Halted | Step::Service(_)) {}
        let mut threaded = boot::microcode_with_pager(source, 512, 16)?.machine;
        let exchange = Exchange::default();
        let (_input, receive) = mpsc::sync_channel(1);
        let (done, finished) = mpsc::channel();
        // Keep an old snapshot alive, and do not consume any new snapshots.
        // Rendering can stall indefinitely without queuing frames or stopping
        // the CPU. The timeout is only a deadlock guard, not a speed assertion.
        let stale = exchange.snapshot.lock().unwrap().clone();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let result = execute(
                    &mut threaded,
                    &mut |m| Ok(!matches!(m.step()?, Step::Halted | Step::Service(_))),
                    &exchange,
                    receive,
                );
                done.send(()).unwrap();
                result
            });
            let stop = StopOnDrop(&exchange);
            finished
                .recv_timeout(Duration::from_secs(10))
                .expect("CPU blocked on absent renderer");
            worker.join().unwrap().result.unwrap();
            drop(stop);
        });
        assert_eq!(threaded.cpu, direct.cpu);
        assert_eq!(threaded.objekt, direct.objekt);
        assert_eq!(threaded.stats.retired, direct.stats.retired);
        assert_eq!(
            threaded.stats.collector_retired,
            direct.stats.collector_retired
        );
        assert_eq!(threaded.stats.collections, direct.stats.collections);
        assert!(stale.running);
        let latest = exchange.snapshot.lock().unwrap();
        assert!(!latest.running);
        assert_eq!(latest.pc, threaded.cpu.pc);
        assert_eq!(latest.stats.retired, threaded.stats.retired);
        Ok(())
    }

    #[test]
    fn window_exit_stops_a_running_worker() -> Result<()> {
        let mut machine = boot::microcode("loop: seq=Jump, brch=loop", 512)?.machine;
        let exchange = Exchange::default();
        let (_input, receive) = mpsc::sync_channel(1);
        let (started, running) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut first = true;
                let result = execute(
                    &mut machine,
                    &mut |m| {
                        m.step()?;
                        if first {
                            started.send(()).unwrap();
                            first = false;
                        }
                        Ok(true)
                    },
                    &exchange,
                    receive,
                );
                done.send(()).unwrap();
                result
            });
            let stop = StopOnDrop(&exchange);
            running.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(stop);
            finished
                .recv_timeout(Duration::from_secs(5))
                .expect("worker ignored window shutdown");
            worker.join().unwrap().result.unwrap();
        });
        assert!(machine.stats.retired > 0);
        assert!(!exchange.snapshot.lock().unwrap().running);
        Ok(())
    }

    #[test]
    fn shutdown_and_faults_publish_final_state() -> Result<()> {
        let mut machine = boot::microcode("halt", 512)?.machine;
        let exchange = Exchange::default();
        drop(StopOnDrop(&exchange));
        let (_input, receive) = mpsc::sync_channel(1);
        execute(
            &mut machine,
            &mut |_| panic!("stepped after stop"),
            &exchange,
            receive,
        )
        .result?;
        assert!(!exchange.snapshot.lock().unwrap().running);
        let exchange = Exchange::default();
        let (_input, receive) = mpsc::sync_channel(1);
        let result = execute(
            &mut machine,
            &mut |_| eyre::bail!("test fault"),
            &exchange,
            receive,
        );
        assert!(result.result.is_err());
        let snapshot = exchange.snapshot.lock().unwrap();
        assert!(!snapshot.running);
        assert_eq!(snapshot.error.as_deref(), Some("test fault"));
        Ok(())
    }

    #[test]
    fn queued_input_preserves_key_order_and_capture_timestamp() -> Result<()> {
        // Read both packets through the machine's ordinary device interface.
        let mut machine = boot::microcode(
            "\
            d=0x314, r=Bus, rb=0, ldrb
            ra=0, io=Read
            d=Device, r=Bus, rb=1, ldrb
            d=0x320, r=Bus, rb=0, ldrb
            ra=0, io=Read
            d=Device, r=Bus, rb=2, ldrb
            d=0x324, r=Bus, rb=0, ldrb
            d=1, ra=0, io=Write
            d=0x314, r=Bus, rb=0, ldrb
            ra=0, io=Read
            d=Device, r=Bus, rb=3, ldrb
            d=0x318, r=Bus, rb=0, ldrb
            ra=0, io=Read
            d=Device, r=Bus, rb=4, ldrb
            halt",
            512,
        )?
        .machine;
        machine.devices = presentation::workstation(0, u64::MAX);
        let exchange = Exchange::default();
        let (input, receive) = mpsc::sync_channel(1);
        input
            .send(InputBatch {
                pointer: Some((12, 34)),
                keys: vec![(97, true), (97, false)],
                timestamp_ms: 12345,
            })
            .unwrap();
        let result = execute(
            &mut machine,
            &mut |m| Ok(!matches!(m.step()?, Step::Halted | Step::Service(_))),
            &exchange,
            receive,
        );
        result.result?;
        assert_eq!(machine.cpu.rf[1], InputKind::KeyDown as u32);
        assert_eq!(machine.cpu.rf[2], 12345);
        assert_eq!(machine.cpu.rf[3], InputKind::KeyUp as u32);
        assert_eq!(machine.cpu.rf[4], 97);
        assert_eq!(machine.devices.pointer.as_ref().unwrap().mouse, (12, 34));
        assert_eq!(result.timing.input_overruns, 0);
        Ok(())
    }
}

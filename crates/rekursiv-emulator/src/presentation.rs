//! Framebuffer presentation and physical input, without access to guest memory.
use eyre::Result;
use rekursiv_devices::{BitmapFrame, Clocks, Device, InputKind, InputPacket};

pub fn workstation(utc_seconds: u64, ticks_per_ms: u64) -> Device {
    Device::workstation(utc_seconds, ticks_per_ms)
}
/// Convert the published one-bit scanout buffer. Set bits are black.
/// Cursor pixels invert the scanout, with clipping at every display edge.
pub fn pixels(frame: &BitmapFrame, cursor: Option<(&BitmapFrame, (i32, i32))>) -> Vec<u32> {
    let mut rgb = vec![0xffffff; (frame.width * frame.height) as usize];
    for y in 0..frame.height {
        for x in 0..frame.width {
            if frame.words[(y * frame.stride + x / 32) as usize] & (1 << (31 - x % 32)) != 0 {
                rgb[(y * frame.width + x) as usize] = 0;
            }
        }
    }
    if let Some((cursor, (cx, cy))) = cursor {
        for y in 0..cursor.height {
            for x in 0..cursor.width {
                let (dx, dy) = (i64::from(cx) + i64::from(x), i64::from(cy) + i64::from(y));
                if dx >= 0
                    && dy >= 0
                    && dx < i64::from(frame.width)
                    && dy < i64::from(frame.height)
                    && cursor.words[(y * cursor.stride + x / 32) as usize] & (1 << (31 - x % 32))
                        != 0
                {
                    rgb[dy as usize * frame.width as usize + dx as usize] ^= 0xffffff;
                }
            }
        }
    }
    rgb
}
pub fn pointer(device: &mut Device, x: i32, y: i32) {
    if let Some(pointer) = &mut device.pointer {
        pointer.mouse = (x, y);
        if pointer.linked {
            pointer.cursor = (x, y);
        }
    }
}
pub fn key(device: &mut Device, code: u16, down: bool) -> Result<bool> {
    let timestamp_ms = device.clocks.as_ref().map_or(0, Clocks::day_milliseconds);
    device.push_input(InputPacket {
        kind: if down {
            InputKind::KeyDown
        } else {
            InputKind::KeyUp
        },
        timestamp_ms,
        value: i32::from(code),
        extra: 0,
    })
}
pub fn save_ppm(frame: &BitmapFrame, path: &std::path::Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "P6\n{} {}\n255", frame.width, frame.height)?;
    for pixel in pixels(frame, None) {
        file.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8])?;
    }
    file.flush()?;
    Ok(())
}

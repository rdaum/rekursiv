//! Framebuffer presentation and physical input, without access to guest memory.
use eyre::Result;
use rekursiv_devices::{BitmapFrame, Clocks, CursorMode, Device, InputKind, InputPacket};

pub fn workstation(utc_seconds: u64, ticks_per_ms: u64) -> Device {
    Device::workstation(utc_seconds, ticks_per_ms)
}
/// Convert external scanout data, without interpreting guest objects.
/// Indexed pixels use the published palette. RGB555 expands each channel to
/// eight bits. Cursor set bits use the published composition mode and origin;
/// the peripheral contract, not the guest language, determines presentation.
pub fn pixels(frame: &BitmapFrame, cursor: Option<(&BitmapFrame, (i32, i32))>) -> Vec<u32> {
    let mut rgb = vec![0xffffff; (frame.width * frame.height) as usize];
    let per_word = 32 / frame.depth;
    let mask = u32::MAX >> (32 - frame.depth);
    for y in 0..frame.height {
        for x in 0..frame.width {
            let word = frame.words[(y * frame.stride + x / per_word) as usize];
            let value = (word >> (32 - frame.depth * (x % per_word + 1))) & mask;
            rgb[(y * frame.width + x) as usize] = match frame.depth {
                1 | 2 | 4 | 8 => frame.palette[value as usize],
                16 => {
                    let expand = |v: u32| ((v & 31) << 3) | ((v & 31) >> 2);
                    (expand(value >> 10) << 16) | (expand(value >> 5) << 8) | expand(value)
                }
                32 => value & 0xffffff,
                _ => unreachable!("validated scanout depth"),
            };
        }
    }
    if let Some((cursor, (cx, cy))) = cursor {
        for y in 0..cursor.height {
            for x in 0..cursor.width {
                let (dx, dy) = (
                    i64::from(cx) + i64::from(cursor.offset.0) + i64::from(x),
                    i64::from(cy) + i64::from(cursor.offset.1) + i64::from(y),
                );
                if dx >= 0
                    && dy >= 0
                    && dx < i64::from(frame.width)
                    && dy < i64::from(frame.height)
                    && cursor.words[(y * cursor.stride + x / 32) as usize] & (1 << (31 - x % 32))
                        != 0
                {
                    let pixel = &mut rgb[dy as usize * frame.width as usize + dx as usize];
                    match cursor.cursor_mode {
                        CursorMode::Invert => *pixel ^= 0xffffff,
                        CursorMode::Paint => *pixel = cursor.palette[1],
                    }
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

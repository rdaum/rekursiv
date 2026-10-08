//! External packed-pixel scanout endpoint. Pixels are most significant first.
//! Indexed depths use a programmable RGB palette; 16-bit pixels are RGB555
//! and 32-bit pixels are XRGB8888. Guest layout and raster work belong to the CPU.
use super::Request;

/// How a monochrome cursor combines set bits with the displayed pixel.
/// Clear bits are transparent in both modes. The guest selects the mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorMode {
    /// XOR the displayed RGB value with white (the original monochrome mode).
    #[default]
    Invert,
    /// Replace the displayed RGB value with cursor palette entry one.
    Paint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub words: Vec<u32>,
    /// Bits per pixel: 1, 2, 4, 8, 16 or 32.
    pub depth: u32,
    /// RGB888 colours for indexed pixels. Supplied by the guest device driver.
    pub palette: Vec<u32>,
    /// Signed origin offset; cursor placement adds this to the pointer position.
    pub offset: (i32, i32),
    /// Composition used when this frame is presented as a cursor.
    pub cursor_mode: CursorMode,
}
impl Default for BitmapFrame {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            stride: 0,
            words: Vec::new(),
            depth: 1,
            palette: default_palette(),
            offset: (0, 0),
            cursor_mode: CursorMode::Invert,
        }
    }
}
fn default_palette() -> Vec<u32> {
    let mut colours = vec![0; 256];
    colours[0] = 0xffffff;
    colours
}
enum UploadData {
    Full { words: Vec<u32>, written: Vec<bool> },
    // Sparse replacement words are invisible until Publish. This models a
    // generic framebuffer transaction, not a guest raster operation. There is
    // no full-frame clone for a small update; duplicate offsets apply in order.
    Patch(Vec<(usize, u32)>),
}
struct Upload {
    width: u32,
    height: u32,
    stride: u32,
    offset: usize,
    data: UploadData,
    depth: u32,
    palette: Vec<u32>,
    origin: (i32, i32),
    cursor_mode: CursorMode,
}
impl Upload {
    fn len(&self) -> usize {
        (self.height * self.stride) as usize
    }
    fn complete(&self) -> bool {
        match &self.data {
            UploadData::Full { written, .. } => written.iter().all(|w| *w),
            UploadData::Patch(_) => true,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Effect {
    Width(u32),
    Height(u32),
    Stride(u32),
    Depth(u32),
    PaletteIndex(u32),
    PaletteData(u32),
    OriginX(i32),
    OriginY(i32),
    CursorMode(CursorMode),
    Begin {
        width: u32,
        height: u32,
        stride: u32,
        patch: bool,
    },
    Offset(usize),
    Data(usize, u32),
    Publish,
}
pub struct Bitmap {
    pub max_width: u32,
    pub max_height: u32,
    pub visible: Option<BitmapFrame>,
    pub publications: u64,
    /// Accepted pixel writes, independent of full-frame or sparse upload mode.
    pub pixel_writes: u64,
    width: u32,
    height: u32,
    stride: u32,
    upload: Option<Upload>,
    depth: u32,
    palette: Vec<u32>,
    palette_index: u32,
    origin: (i32, i32),
    cursor_mode: CursorMode,
}
impl Bitmap {
    pub fn new(max_width: u32, max_height: u32) -> Self {
        assert!((1..=16383).contains(&max_width) && (1..=16383).contains(&max_height));
        Self {
            max_width,
            max_height,
            visible: None,
            publications: 0,
            pixel_writes: 0,
            width: 0,
            height: 0,
            stride: 0,
            upload: None,
            depth: 1,
            palette: default_palette(),
            palette_index: 0,
            origin: (0, 0),
            cursor_mode: CursorMode::Invert,
        }
    }
    /// Begin=0 requires a complete replacement. Begin=2 starts a sparse patch
    /// of an existing frame with identical geometry. Both publish atomically.
    pub(super) fn preview(&self, request: Request, base: u32) -> Option<(u32, Option<Effect>)> {
        Some(match (request.address.checked_sub(base)?, request.write) {
            (0, false) => (self.max_width, None),
            (4, false) => (self.max_height, None),
            (8, false) => (self.visible.as_ref().map_or(0, |f| f.width), None),
            (12, false) => (self.visible.as_ref().map_or(0, |f| f.height), None),
            (16, false) => (self.visible.as_ref().map_or(0, |f| f.stride), None),
            (32, false) => (self.visible.as_ref().map_or(1, |f| f.depth), None),
            (32, true) if matches!(request.data, 1 | 2 | 4 | 8 | 16 | 32) => {
                (0, Some(Effect::Depth(request.data)))
            }
            (36, true) if request.data < 256 => (0, Some(Effect::PaletteIndex(request.data))),
            (40, true) if self.palette_index < 256 => {
                (0, Some(Effect::PaletteData(request.data & 0xffffff)))
            }
            (44, true) => (0, Some(Effect::OriginX(request.data as i32))),
            (48, true) => (0, Some(Effect::OriginY(request.data as i32))),
            (52, true) if request.data <= 1 => (
                0,
                Some(Effect::CursorMode(if request.data == 0 {
                    CursorMode::Invert
                } else {
                    CursorMode::Paint
                })),
            ),
            (8, true) => (0, Some(Effect::Width(request.data))),
            (12, true) => (0, Some(Effect::Height(request.data))),
            (16, true) => (0, Some(Effect::Stride(request.data))),
            (20, true)
                if matches!(request.data, 0 | 2)
                    && self.width > 0
                    && self.width <= self.max_width
                    && self.height > 0
                    && self.height <= self.max_height
                    && self.stride >= (self.width * self.depth).div_ceil(32)
                    && self.stride <= (self.max_width * self.depth).div_ceil(32)
                    && (request.data == 0
                        || self.visible.as_ref().is_some_and(|f| {
                            (f.width, f.height, f.stride, f.depth)
                                == (self.width, self.height, self.stride, self.depth)
                        })) =>
            {
                (
                    0,
                    Some(Effect::Begin {
                        width: self.width,
                        height: self.height,
                        stride: self.stride,
                        patch: request.data == 2,
                    }),
                )
            }
            (20, true)
                if request.data == 1 && self.upload.as_ref().is_some_and(Upload::complete) =>
            {
                (0, Some(Effect::Publish))
            }
            (24, true) if (request.data as usize) < self.upload.as_ref()?.len() => {
                (0, Some(Effect::Offset(request.data as usize)))
            }
            (28, true) if self.upload.as_ref()?.offset < self.upload.as_ref()?.len() => (
                0,
                Some(Effect::Data(self.upload.as_ref()?.offset, request.data)),
            ),
            _ => return None,
        })
    }
    pub(super) fn commit(&mut self, effect: Effect) {
        match effect {
            Effect::Width(n) => self.width = n,
            Effect::Height(n) => self.height = n,
            Effect::Stride(n) => self.stride = n,
            Effect::Depth(n) => self.depth = n,
            Effect::PaletteIndex(n) => self.palette_index = n,
            Effect::PaletteData(n) => {
                self.palette[self.palette_index as usize] = n;
                self.palette_index += 1;
            }
            Effect::OriginX(n) => self.origin.0 = n,
            Effect::OriginY(n) => self.origin.1 = n,
            Effect::CursorMode(mode) => self.cursor_mode = mode,
            Effect::Begin {
                width,
                height,
                stride,
                patch,
            } => {
                let count = (height * stride) as usize;
                self.upload = Some(Upload {
                    width,
                    height,
                    stride,
                    offset: 0,
                    depth: self.depth,
                    palette: self.palette.clone(),
                    origin: self.origin,
                    cursor_mode: self.cursor_mode,
                    data: if patch {
                        UploadData::Patch(Vec::new())
                    } else {
                        UploadData::Full {
                            words: vec![0; count],
                            written: vec![false; count],
                        }
                    },
                });
            }
            Effect::Offset(n) => self.upload.as_mut().unwrap().offset = n,
            Effect::Data(n, word) => {
                let upload = self.upload.as_mut().unwrap();
                match &mut upload.data {
                    UploadData::Full { words, written } => {
                        words[n] = word;
                        written[n] = true;
                    }
                    UploadData::Patch(writes) => writes.push((n, word)),
                }
                upload.offset = n + 1;
                self.pixel_writes += 1;
            }
            Effect::Publish => {
                let upload = self.upload.take().unwrap();
                match upload.data {
                    UploadData::Full { words, .. } => {
                        self.visible = Some(BitmapFrame {
                            width: upload.width,
                            height: upload.height,
                            stride: upload.stride,
                            words,
                            depth: upload.depth,
                            palette: upload.palette,
                            offset: upload.origin,
                            cursor_mode: upload.cursor_mode,
                        })
                    }
                    UploadData::Patch(writes) => {
                        let frame = self.visible.as_mut().unwrap();
                        frame.palette = upload.palette;
                        frame.offset = upload.origin;
                        frame.cursor_mode = upload.cursor_mode;
                        for (offset, word) in writes {
                            frame.words[offset] = word;
                        }
                    }
                }
                self.publications += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write(bitmap: &mut Bitmap, offset: u32, value: u32) -> bool {
        let Some((_, effect)) = bitmap.preview(
            Request {
                address: 0x510 + offset,
                write: true,
                data: value,
            },
            0x510,
        ) else {
            return false;
        };
        if let Some(effect) = effect {
            bitmap.commit(effect);
        }
        true
    }
    #[test]
    fn colour_and_origin_are_latched_at_begin_and_published_atomically() {
        let mut b = Bitmap::new(32, 2);
        for (a, v) in [
            (8, 4),
            (12, 1),
            (16, 1),
            (32, 8),
            (36, 1),
            (40, 0x123456),
            (44, u32::MAX),
            (52, 1),
            (20, 0),
        ] {
            assert!(write(&mut b, a, v));
        }
        assert!(!write(&mut b, 20, 1)); // incomplete frame
        assert!(b.visible.is_none());
        // Changes after Begin belong to the next transaction.
        assert!(write(&mut b, 36, 1));
        assert!(write(&mut b, 40, 0xabcdef));
        assert!(write(&mut b, 44, 0));
        assert!(write(&mut b, 52, 0));
        assert!(!write(&mut b, 52, 2));
        assert!(write(&mut b, 28, 0x01010101));
        assert!(write(&mut b, 20, 1));
        let first = b.visible.clone().unwrap();
        assert_eq!(first.palette[1], 0x123456);
        assert_eq!(first.offset, (-1, 0));
        assert_eq!(first.depth, 8);
        assert_eq!(first.cursor_mode, CursorMode::Paint);
        assert!(write(&mut b, 32, 16));
        assert!(!write(&mut b, 20, 2)); // same stride does not imply matching depth
        assert_eq!(b.visible.as_ref(), Some(&first));
        assert!(write(&mut b, 32, 8));
        assert!(write(&mut b, 20, 2));
        assert_eq!(b.visible.as_ref(), Some(&first));
        assert!(write(&mut b, 20, 1));
        let next = b.visible.unwrap();
        assert_eq!(next.words, first.words);
        assert_eq!(next.palette[1], 0xabcdef);
        assert_eq!(next.offset, (0, 0));
        assert_eq!(next.cursor_mode, CursorMode::Invert);
    }
    #[test]
    fn sparse_upload_preserves_other_words_and_replaces_abandoned_work() {
        let mut b = Bitmap::new(64, 64);
        let old = BitmapFrame {
            width: 33,
            height: 2,
            stride: 2,
            words: vec![1, 2, 3, 4],
            ..BitmapFrame::default()
        };
        for (a, v) in [(8, 33), (12, 2), (16, 2)] {
            assert!(write(&mut b, a, v));
        }
        assert!(!write(&mut b, 20, 2)); // no published frame to patch
        b.visible = Some(old.clone());
        assert!(write(&mut b, 20, 2));
        assert!(write(&mut b, 24, 3));
        assert!(write(&mut b, 28, 99));
        assert!(!write(&mut b, 28, 88)); // sequential write beyond the last word
        assert!(!write(&mut b, 24, 4));
        assert_eq!(b.visible, Some(old.clone()));
        assert!(write(&mut b, 8, 64));
        // Different visible geometry.
        assert!(!write(&mut b, 20, 2));
        // The rejected Begin preserves the old pending patch.
        assert!(write(&mut b, 20, 1));
        assert_eq!(b.visible.as_ref().unwrap().words, vec![1, 2, 3, 99]);
        assert!(write(&mut b, 8, 33));
        assert!(write(&mut b, 20, 2));
        assert!(write(&mut b, 28, 55));
        assert!(write(&mut b, 20, 2)); // abandon 55
        assert!(write(&mut b, 24, 1));
        assert!(write(&mut b, 28, 66));
        assert!(write(&mut b, 24, 1));
        assert!(write(&mut b, 28, 77)); // last write wins
        assert!(write(&mut b, 20, 1));
        assert_eq!(b.visible.as_ref().unwrap().words, vec![1, 77, 3, 99]);
        assert_eq!(b.publications, 2);
        assert_eq!(b.pixel_writes, 4);
    }
}

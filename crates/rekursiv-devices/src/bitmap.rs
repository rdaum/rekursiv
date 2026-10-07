//! External bitmap upload endpoint. Words contain 32 monochrome pixels, most
//! significant pixel first. The processor owns guest layouts and raster work.
use super::Request;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitmapFrame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub words: Vec<u32>,
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
            (8, true) => (0, Some(Effect::Width(request.data))),
            (12, true) => (0, Some(Effect::Height(request.data))),
            (16, true) => (0, Some(Effect::Stride(request.data))),
            (20, true)
                if matches!(request.data, 0 | 2)
                    && self.width > 0
                    && self.width <= self.max_width
                    && self.height > 0
                    && self.height <= self.max_height
                    && self.stride >= self.width.div_ceil(32)
                    && self.stride <= self.max_width.div_ceil(32)
                    && (request.data == 0
                        || self.visible.as_ref().is_some_and(|f| {
                            (f.width, f.height, f.stride) == (self.width, self.height, self.stride)
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
                        })
                    }
                    UploadData::Patch(writes) => {
                        let frame = self.visible.as_mut().unwrap();
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
    fn sparse_upload_preserves_other_words_and_replaces_abandoned_work() {
        let mut b = Bitmap::new(64, 64);
        let old = BitmapFrame {
            width: 33,
            height: 2,
            stride: 2,
            words: vec![1, 2, 3, 4],
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

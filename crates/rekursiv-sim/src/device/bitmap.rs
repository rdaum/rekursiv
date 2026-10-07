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
struct Upload {
    frame: BitmapFrame,
    written: Vec<bool>,
    offset: usize,
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
            width: 0,
            height: 0,
            stride: 0,
            upload: None,
        }
    }
    /// `base` selects a register bank; neither address nor data names a guest
    /// class. Publication requires every pixel word of the staged frame.
    pub(super) fn preview(&self, request: Request, base: u32) -> Option<(u32, Option<Effect>)> {
        Some(match (request.address.checked_sub(base)?, request.write) {
            (0, false) => (self.max_width, None),
            (4, false) => (self.max_height, None),
            (8, true) => (0, Some(Effect::Width(request.data))),
            (12, true) => (0, Some(Effect::Height(request.data))),
            (16, true) => (0, Some(Effect::Stride(request.data))),
            (20, true)
                if request.data == 0
                    && self.width > 0
                    && self.width <= self.max_width
                    && self.height > 0
                    && self.height <= self.max_height
                    && self.stride >= self.width.div_ceil(32)
                    && self.stride <= self.max_width.div_ceil(32) =>
            {
                (
                    0,
                    Some(Effect::Begin {
                        width: self.width,
                        height: self.height,
                        stride: self.stride,
                    }),
                )
            }
            (20, true)
                if request.data == 1
                    && self
                        .upload
                        .as_ref()
                        .is_some_and(|u| u.written.iter().all(|w| *w)) =>
            {
                (0, Some(Effect::Publish))
            }
            (24, true) if (request.data as usize) < self.upload.as_ref()?.frame.words.len() => {
                (0, Some(Effect::Offset(request.data as usize)))
            }
            (28, true)
                if self.upload.as_ref()?.offset < self.upload.as_ref()?.frame.words.len() =>
            {
                (
                    0,
                    Some(Effect::Data(self.upload.as_ref()?.offset, request.data)),
                )
            }
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
            } => {
                let count = (height * stride) as usize;
                self.upload = Some(Upload {
                    frame: BitmapFrame {
                        width,
                        height,
                        stride,
                        words: vec![0; count],
                    },
                    written: vec![false; count],
                    offset: 0,
                });
            }
            Effect::Offset(n) => self.upload.as_mut().unwrap().offset = n,
            Effect::Data(n, word) => {
                let upload = self.upload.as_mut().unwrap();
                upload.frame.words[n] = word;
                upload.written[n] = true;
                upload.offset = n + 1;
            }
            Effect::Publish => {
                self.visible = Some(self.upload.take().unwrap().frame);
                self.publications += 1;
            }
        }
    }
}

//! Checked reader for the pre-Spur, 32-bit Squeak format 6502.
//!
//! Source evidence: Squeak1.1.changes, ObjectMemory's header constants,
//! `fetchClassOf:`, `formatOf:`, `sizeBitsOf:` and Interpreter's method accessors.
//! Addresses remain source oops. No guest code runs while resolving the graph.
use super::layout::{self, special, MethodHeader};
use crate::{checksum, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

/// Byte order of numeric image words. Byte arrays keep their physical ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    fn word(self, bytes: &[u8], offset: usize) -> Result<u32> {
        let data: [u8; 4] = bytes
            .get(offset..offset.saturating_add(4))
            .ok_or_else(|| Error(format!("truncated Squeak word at byte {offset}")))?
            .try_into()
            .unwrap();
        Ok(match self {
            Self::Little => u32::from_le_bytes(data),
            Self::Big => u32::from_be_bytes(data),
        })
    }
    fn words(self, bytes: &[u8]) -> Result<Vec<u32>> {
        (0..bytes.len())
            .step_by(4)
            .map(|i| self.word(bytes, i))
            .collect()
    }
}

/// Header fields needed for relocation, boot, identity generation and display.
#[derive(Clone, Debug, Serialize)]
pub struct Header {
    pub endian: Endian,
    pub header_bytes: u32,
    pub heap_bytes: u32,
    pub old_base: u32,
    pub special_objects: u32,
    pub last_hash: u32,
    /// Width in the high half, height in the low half of the saved window word.
    pub window_size: (u16, u16),
    pub fullscreen: u32,
}

/// Source contents with pointer-bearing words separated from opaque data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Pointers(Vec<u32>),
    /// Includes Bitmap words and Float's two binary64 words, without conversion.
    Words(Vec<u32>),
    Bytes(Vec<u8>),
    Method {
        header: MethodHeader,
        literals: Vec<u32>,
        /// All bytes after the literals, including the source-location trailer.
        bytes: Vec<u8>,
    },
}

/// One allocated object. GC bookkeeping bits and header padding are not fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub oop: u32,
    pub class: u32,
    /// Historical 12-bit identity hash; distinct objects may share this value.
    pub hash: u16,
    /// Original four-bit format, including unused byte padding count.
    pub format: u8,
    pub body: Body,
}

impl Object {
    /// Read an actual guest pointer field, with a contextual diagnostic on error.
    pub fn pointer(&self, index: usize) -> Result<u32> {
        if let Body::Pointers(fields) = &self.body {
            if let Some(&value) = fields.get(index) {
                return Ok(value);
            }
        }
        Err(Error(format!(
            "Squeak oop 0x{:08x}: missing pointer field {index}",
            self.oop
        )))
    }
    /// Enumerate references and tagged integers; opaque bytes/words never appear.
    pub fn references(&self) -> Vec<u32> {
        let mut refs = vec![self.class];
        match &self.body {
            Body::Pointers(fields) => refs.extend(fields),
            Body::Method { literals, .. } => refs.extend(literals),
            _ => {}
        }
        refs
    }
}

/// Entire validated heap. Free chunks are counted but never converted to objects.
#[derive(Clone, Debug)]
pub struct Image {
    pub sha256: String,
    pub header: Header,
    pub objects: BTreeMap<u32, Object>,
    pub free_bytes: usize,
}

impl Image {
    /// Read a 6502 snapshot in either byte order and resolve every class and edge.
    /// Other image versions are rejected even if some header fields look similar.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let endian = if Endian::Little.word(bytes, 0)? == layout::IMAGE_VERSION {
            Endian::Little
        } else if Endian::Big.word(bytes, 0)? == layout::IMAGE_VERSION {
            Endian::Big
        } else {
            return Err(Error(
                "unsupported Squeak image: expected format 6502".into(),
            ));
        };
        let w = |i: usize| endian.word(bytes, i * 4);
        let window = w(6)?;
        let header = Header {
            endian,
            header_bytes: w(1)?,
            heap_bytes: w(2)?,
            old_base: w(3)?,
            special_objects: w(4)?,
            last_hash: w(5)?,
            window_size: ((window >> 16) as u16, window as u16),
            fullscreen: w(7)?,
        };
        let start = header.header_bytes as usize;
        let size = header.heap_bytes as usize;
        if start < 32
            || !start.is_multiple_of(4)
            || size == 0
            || !size.is_multiple_of(4)
            || start.checked_add(size) != Some(bytes.len())
            || !header.old_base.is_multiple_of(4)
            || header.old_base.checked_add(header.heap_bytes).is_none()
        {
            return Err(Error("invalid Squeak image extent or base address".into()));
        }
        let heap = &bytes[start..];
        let mut objects = BTreeMap::new();
        let mut compact = Vec::new();
        let mut offset = 0usize;
        let mut free_bytes = 0;
        while offset < heap.len() {
            let first = endian.word(heap, offset)?;
            let typ = first & 3;
            if typ == 2 {
                let size = (first & 0x1fff_fffc) as usize;
                if size < 4 || size > heap.len() - offset {
                    return Err(Error(format!("invalid free chunk at heap byte {offset}")));
                }
                free_bytes += size;
                offset += size;
                continue;
            }
            let extra = match typ {
                0 => 8,
                1 => 4,
                _ => 0,
            };
            let base = offset + extra;
            let bits = endian.word(heap, base)?;
            let size = (if typ == 0 { first & !3 } else { bits & 252 }) as usize;
            if bits & 3 != typ || size < 4 || size > heap.len().saturating_sub(base) {
                return Err(Error(format!(
                    "invalid object header/extent at heap byte {offset}"
                )));
            }
            let cc = ((bits >> 12) & 31) as usize;
            let class = if extra != 0 {
                let c = endian.word(heap, base - 4)?;
                if c & 3 != typ {
                    return Err(Error(
                        "Squeak class header has inconsistent type bits".into(),
                    ));
                }
                c & !3
            } else if cc == 0 {
                return Err(Error("short Squeak header has no compact class".into()));
            } else {
                0
            };
            let oop = header.old_base + base as u32;
            let format = ((bits >> 8) & 15) as u8;
            let data = &heap[base + 4..base + size];
            let body = match format {
                0..=3 => Body::Pointers(endian.words(data)?),
                6 => Body::Words(endian.words(data)?),
                8..=15 => {
                    let end = data
                        .len()
                        .checked_sub(usize::from(format & 3))
                        .ok_or_else(|| Error("byte padding exceeds Squeak body size".into()))?;
                    let data = &data[..end];
                    if format < 12 {
                        Body::Bytes(data.to_vec())
                    } else {
                        let header = MethodHeader::parse(endian.word(data, 0)?)?;
                        let prefix = (1 + header.literal_count()) * 4;
                        if prefix > data.len() {
                            return Err(Error(format!(
                                "method 0x{oop:08x}: truncated literal frame"
                            )));
                        }
                        Body::Method {
                            header,
                            literals: endian.words(&data[4..prefix])?,
                            bytes: data[prefix..].to_vec(),
                        }
                    }
                }
                _ => return Err(Error(format!("unsupported Squeak object format {format}"))),
            };
            objects.insert(
                oop,
                Object {
                    oop,
                    class,
                    hash: ((bits >> 17) & 4095) as u16,
                    format,
                    body,
                },
            );
            if cc != 0 {
                compact.push((oop, cc));
            }
            offset = base + size;
        }
        let mut image = Self {
            sha256: checksum(bytes),
            header,
            objects,
            free_bytes,
        };
        let array = image.object(image.special(special::COMPACT_CLASSES)?)?;
        let Body::Pointers(classes) = &array.body else {
            return Err(Error(
                "Squeak compact classes must be a pointer Array".into(),
            ));
        };
        let classes = classes.clone();
        for (oop, index) in compact {
            let class = *classes
                .get(index - 1)
                .ok_or_else(|| Error(format!("invalid compact class index {index}")))?;
            image.objects.get_mut(&oop).unwrap().class = class;
        }
        image.validate()?;
        Ok(image)
    }

    /// Resolve an allocated object, rejecting integers, interiors and free chunks.
    pub fn object(&self, oop: u32) -> Result<&Object> {
        self.objects
            .get(&oop)
            .ok_or_else(|| Error(format!("missing Squeak object 0x{oop:08x}")))
    }

    /// Resolve an entry from specialObjectsArray, rather than assuming fixed oops.
    pub fn special(&self, index: usize) -> Result<u32> {
        self.object(self.header.special_objects)?.pointer(index)
    }

    /// Check the graph independently of storage order; forward edges and cycles work.
    pub fn validate(&self) -> Result<()> {
        for object in self.objects.values() {
            self.object(object.class)?;
            for value in object.references() {
                if value & 1 == 0 {
                    self.object(value)?;
                }
            }
        }
        self.special(layout::SPECIAL_OBJECT_COUNT - 1)?;
        for index in [
            special::NIL,
            special::FALSE,
            special::TRUE,
            special::INTEGER_CLASS,
        ] {
            self.object(self.special(index)?)?;
        }
        Ok(())
    }

    /// Follow Processor association -> scheduler -> active process -> saved context.
    pub fn initial_context(&self) -> Result<u32> {
        let scheduler = self.object(self.special(special::SCHEDULER)?)?.pointer(1)?;
        let process = self.object(scheduler)?.pointer(1)?;
        let context = self.object(process)?.pointer(1)?;
        self.object(context)?;
        Ok(context)
    }
}

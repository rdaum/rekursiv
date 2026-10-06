//! Xerox Version 2 big-endian interchange format, manual Part 1 pp. 2-3.
//! Parse all records before resolving pointers so cycles and forward references
//! need no fixups. Reference counts are historical GC state, not liveness.
use crate::{checksum, layout, Error, Result};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Pointers(Vec<u16>),
    Words(Vec<u16>),
    Bytes(Vec<u8>),
    Method {
        header: layout::MethodHeader,
        literals: Vec<u16>,
        /// All bytes after the literals, including source-location trailer.
        bytes: Vec<u8>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub oop: u16,
    pub class: u16,
    pub body: Body,
}
#[derive(Clone, Debug)]
pub struct Image {
    pub sha256: String,
    pub object_space_words: usize,
    pub object_table_words: usize,
    pub objects: BTreeMap<u16, Object>,
}

fn word(bytes: &[u8], offset: usize) -> Result<u16> {
    let b = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| Error(format!("truncated 16-bit word at byte offset {offset}")))?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
fn words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect()
}

impl Object {
    /// Class and pointer-bearing body fields only. Bytecode bytes must never
    /// enter this iterator even when two adjacent bytes resemble a valid oop.
    pub fn references(&self) -> Vec<u16> {
        let mut refs = vec![self.class];
        match &self.body {
            Body::Pointers(fields) => refs.extend(fields),
            Body::Method {
                header, literals, ..
            } => {
                refs.push(header.0);
                refs.extend(literals);
            }
            _ => {}
        }
        refs
    }
    pub fn pointer(&self, index: usize) -> Result<u16> {
        match &self.body {
            Body::Pointers(fields) => fields.get(index).copied().ok_or_else(|| {
                Error(format!(
                    "oop 0x{:04x}: missing pointer field {index}",
                    self.oop
                ))
            }),
            _ => Err(Error(format!(
                "oop 0x{:04x}: expected pointer object",
                self.oop
            ))),
        }
    }
    pub fn primitive(&self) -> Result<Option<u8>> {
        if let Body::Method {
            header, literals, ..
        } = &self.body
        {
            if header.flag() == 7 {
                let extension = literals
                    .get(literals.len().checked_sub(2).ok_or_else(|| {
                        Error(format!(
                            "method 0x{:04x}: missing header extension",
                            self.oop
                        ))
                    })?)
                    .ok_or_else(|| Error("missing method header extension".into()))?;
                layout::small_integer(*extension)?;
                return Ok(Some(((extension >> 1) & 255) as u8));
            }
        }
        Ok(None)
    }
}

impl Image {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 512 {
            return Err(Error(
                "truncated interchange header (requires 512 bytes)".into(),
            ));
        }
        if bytes[8..512].iter().any(|b| *b != 0) {
            return Err(Error("unsupported image format: expected zero-filled interchange header; internal, Stretch and LOOM images are not supported".into()));
        }
        let heap_words = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let table_words = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if heap_words == 0
            || heap_words > 1 << 20
            || !(4..=65536).contains(&table_words)
            || !table_words.is_multiple_of(2)
        {
            return Err(Error("unsupported image sizes or byte order (requires 20-bit heap and 16-bit oops in big-endian interchange format)".into()));
        }
        let heap_end = 512 + heap_words * 2;
        let table_start = heap_end.div_ceil(512) * 512;
        let end = table_start + table_words * 2;
        // The tape can pad the final record to 2048 bytes. Nothing else may
        // follow the table; silently accepting another image variant is unsafe.
        if bytes.len() != end && bytes.len() != end.div_ceil(2048) * 2048 {
            return Err(Error(format!(
                "image length {} does not match header length {end} (or its tape padding)",
                bytes.len()
            )));
        }
        if bytes.len() < end
            || bytes[heap_end..table_start]
                .iter()
                .chain(bytes[end..].iter())
                .any(|b| *b != 0)
        {
            return Err(Error("nonzero or truncated interchange padding".into()));
        }
        let heap = &bytes[512..heap_end];
        let table = &bytes[table_start..end];
        let mut raw = BTreeMap::new();
        let mut ranges = Vec::new();
        for index in (0..table_words).step_by(2) {
            let oop = index as u16;
            let flags = word(table, index * 2)?;
            let location = word(table, index * 2 + 2)?;
            if flags & 32 != 0 {
                if flags != 32 || location != 0 {
                    return Err(Error(format!(
                        "oop 0x{oop:04x}: unsupported nonempty free entry"
                    )));
                }
                continue;
            }
            if oop == 0 || flags & 16 != 0 {
                return Err(Error(format!(
                    "oop 0x{oop:04x}: reserved oop or object-table flag"
                )));
            }
            let address = (usize::from(flags & 15) << 16) | usize::from(location);
            let size = usize::from(word(heap, address * 2)?);
            if size < 2 || address + size > heap_words {
                return Err(Error(format!(
                    "oop 0x{oop:04x}: object extent outside object space"
                )));
            }
            let class = word(heap, address * 2 + 2)?;
            let odd = usize::from(flags & 128 != 0);
            if (size == 2 && odd != 0) || (flags & 64 != 0 && odd != 0) {
                return Err(Error(format!("oop 0x{oop:04x}: invalid odd-byte flag")));
            }
            ranges.push((address, address + size));
            raw.insert(
                oop,
                (
                    class,
                    flags,
                    &heap[address * 2 + 4..(address + size) * 2 - odd],
                ),
            );
        }
        ranges.sort_unstable();
        let mut cursor = 0;
        for (start, end) in ranges {
            if start != cursor {
                return Err(Error(
                    "object extents overlap or leave a gap in interchange object space".into(),
                ));
            }
            cursor = end;
        }
        if cursor != heap_words {
            return Err(Error(
                "object table does not cover the complete object space".into(),
            ));
        }
        let mut objects = BTreeMap::new();
        for (&oop, &(class, flags, data)) in &raw {
            let &(_, class_flags, class_data) = raw
                .get(&class)
                .ok_or_else(|| Error(format!("oop 0x{oop:04x}: missing class 0x{class:04x}")))?;
            if class_flags & 64 == 0 || class_data.len() < 6 {
                return Err(Error(format!(
                    "oop 0x{oop:04x}: class has no instance specification"
                )));
            }
            let spec = word(class_data, layout::class::INSTANCE_SPECIFICATION * 2)?;
            layout::small_integer(spec)?;
            if (flags & 64 != 0) != (spec & 0x8000 != 0) {
                return Err(Error(format!(
                    "oop 0x{oop:04x}: pointer flag disagrees with class format"
                )));
            }
            let body = if class == layout::COMPILED_METHOD_CLASS {
                if flags & 64 != 0 || spec & 0x4000 != 0 {
                    return Err(Error(format!("method 0x{oop:04x}: expected byte format")));
                }
                let header = layout::MethodHeader::parse(word(data, 0)?)?;
                let byte_start = 2 * (1 + header.literal_count());
                if byte_start > data.len() {
                    return Err(Error(format!(
                        "method 0x{oop:04x}: literal frame exceeds body"
                    )));
                }
                Body::Method {
                    header,
                    literals: words(&data[2..byte_start]),
                    bytes: data[byte_start..].to_vec(),
                }
            } else if flags & 64 != 0 {
                Body::Pointers(words(data))
            } else if spec & 0x4000 != 0 {
                if flags & 128 != 0 {
                    return Err(Error(format!(
                        "oop 0x{oop:04x}: word object has odd byte length"
                    )));
                }
                Body::Words(words(data))
            } else {
                Body::Bytes(data.to_vec())
            };
            objects.insert(oop, Object { oop, class, body });
        }
        let result = Self {
            sha256: checksum(bytes),
            object_space_words: heap_words,
            object_table_words: table_words,
            objects,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn object(&self, oop: u16) -> Result<&Object> {
        self.objects
            .get(&oop)
            .ok_or_else(|| Error(format!("missing object 0x{oop:04x}")))
    }

    pub fn validate(&self) -> Result<()> {
        for (&oop, object) in &self.objects {
            if oop == 0 || oop & 1 != 0 || object.oop != oop || object.class & 1 != 0 {
                return Err(Error(format!("invalid object/class oop 0x{oop:04x}")));
            }
            for reference in object.references() {
                if reference & 1 == 0 && !self.objects.contains_key(&reference) {
                    return Err(Error(format!(
                        "oop 0x{oop:04x}: dangling reference 0x{reference:04x}"
                    )));
                }
            }
            if let Body::Method {
                header, literals, ..
            } = &object.body
            {
                layout::MethodHeader::parse(header.0)?;
                if header.literal_count() != literals.len() {
                    return Err(Error(format!("method 0x{oop:04x}: literal count mismatch")));
                }
            }
            object.primitive()?;
        }
        Ok(())
    }

    /// Resume data is read, never executed, by the converter. Stage 2+ owns
    /// restoration of these guest fields into the machine's runtime registers.
    pub fn initial_context(&self) -> Result<u16> {
        let scheduler = self.object(layout::SCHEDULER_ASSOCIATION)?.pointer(1)?;
        let process = self
            .object(scheduler)?
            .pointer(layout::scheduler::ACTIVE_PROCESS)?;
        let context = self
            .object(process)?
            .pointer(layout::scheduler::SUSPENDED_CONTEXT)?;
        let class = self.object(context)?.class;
        if class != layout::METHOD_CONTEXT_CLASS && class != layout::BLOCK_CONTEXT_CLASS {
            return Err(Error("active process has no resumable context".into()));
        }
        Ok(context)
    }
}

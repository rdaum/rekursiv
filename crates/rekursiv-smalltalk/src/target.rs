//! Language-owned representation built entirely from existing machine words.
//! All values that the target needs are in bodies, class metadata, or boot
//! metadata. Host-only source oops are diagnostics, not a runtime lookup table.
use crate::{layout, source, Error, Result};
use rekursiv_asm::Word;
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT_VERSION: u32 = 1;
pub const NEXT_IDENTITY: u64 = 32768;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Pointers = 0,
    Words = 1,
    Bytes = 2,
    Method = 3,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Record {
    pub source_oop: u16,
    #[serde(serialize_with = "serialize_word")]
    pub reference: Word,
    #[serde(serialize_with = "serialize_word")]
    pub class: Word,
    pub cond: bool,
    #[serde(serialize_with = "serialize_words")]
    pub body: Vec<Word>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Image {
    pub format_version: u32,
    pub source_sha256: String,
    /// Allocator initialization requirement, not a physical object address.
    pub next_identity: u64,
    #[serde(serialize_with = "serialize_words")]
    pub roots: [Word; 32],
    /// Only compact code 2 is a guest value: class SmallInteger. Machine nil,
    /// compact Boolean and unsigned codes do not represent guest objects.
    #[serde(serialize_with = "serialize_classes")]
    pub compact_classes: [Option<Word>; 4],
    pub records: Vec<Record>,
    /// An inventory for later microcode implementation, not host dispatch.
    pub primitives: BTreeMap<u8, usize>,
}

fn serialize_word<S: Serializer>(
    word: &Word,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_u64(word.bits())
}
fn serialize_words<S: Serializer>(
    words: &[Word],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    words
        .iter()
        .map(|w| w.bits())
        .collect::<Vec<_>>()
        .serialize(serializer)
}
fn serialize_classes<S: Serializer>(
    classes: &[Option<Word>; 4],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    classes.map(|c| c.map(|w| w.bits())).serialize(serializer)
}

pub fn value(oop: u16) -> Result<Word> {
    if oop & 1 != 0 {
        Ok(Word::signed(layout::small_integer(oop)?))
    } else {
        layout::reference(oop)
    }
}

/// Reverse a converted guest value. Machine-only nil and raw values are not
/// legal guest pointer fields. Newly allocated identities require a future
/// snapshot/export policy; this function audits the original imported graph.
pub fn source_oop(value: Word) -> Result<u16> {
    if value.is_reference() {
        let id = value.identity().unwrap();
        if id < NEXT_IDENTITY && value == Word::reference(id, true).unwrap() {
            return Ok((id as u16) << 1);
        }
    } else if let Ok((2, payload)) = value.compact_parts() {
        return layout::integer_oop(payload as i32);
    }
    Err(Error(format!("{value} is not a converted guest oop")))
}

fn raw(value: usize) -> Word {
    // Source objects are limited to a 16-bit size in 16-bit words.
    Word::raw(value as u64).unwrap()
}

impl Record {
    fn convert(object: &source::Object) -> Result<Self> {
        let mut body = vec![Word::ZERO];
        let (kind, byte_length) = match &object.body {
            source::Body::Pointers(fields) => {
                for field in fields {
                    body.push(value(*field)?);
                }
                (Kind::Pointers, fields.len() * 2)
            }
            source::Body::Words(words) => {
                body.extend(words.iter().map(|w| raw(usize::from(*w))));
                (Kind::Words, words.len() * 2)
            }
            source::Body::Bytes(bytes) => {
                body.extend(bytes.iter().map(|b| raw(usize::from(*b))));
                (Kind::Bytes, bytes.len())
            }
            source::Body::Method {
                header,
                literals,
                bytes,
            } => {
                body.push(value(header.0)?);
                for literal in literals {
                    body.push(value(*literal)?);
                }
                body.extend(bytes.iter().map(|b| raw(usize::from(*b))));
                (Kind::Method, (1 + literals.len()) * 2 + bytes.len())
            }
        };
        body[0] = raw((byte_length << 2) | kind as usize);
        Ok(Self {
            source_oop: object.oop,
            reference: layout::reference(object.oop)?,
            class: layout::reference(object.class)?,
            cond: false,
            body,
        })
    }

    /// Independently decode the emitted physical representation for lossless
    /// conversion checks. This performs no guest execution or collector work.
    pub fn decode(&self) -> Result<source::Object> {
        let oop = source_oop(self.reference)?;
        if oop != self.source_oop || !self.class.is_reference() {
            return Err(Error(
                "record identity/class does not match a stored guest object".into(),
            ));
        }
        let descriptor = self
            .body
            .first()
            .ok_or_else(|| Error("empty physical record".into()))?
            .bits();
        if descriptor >= 1 << 39 {
            return Err(Error("format descriptor is not raw data".into()));
        }
        let length = (descriptor >> 2) as usize;
        let payload = &self.body[1..];
        let scalars = |limit: u64| -> Result<Vec<u16>> {
            payload
                .iter()
                .map(|w| {
                    if w.bits() > limit {
                        Err(Error("non-scalar word in opaque guest contents".into()))
                    } else {
                        Ok(w.bits() as u16)
                    }
                })
                .collect()
        };
        let body = match descriptor & 3 {
            0 => {
                if payload.len() * 2 != length {
                    return Err(Error("pointer length mismatch".into()));
                }
                source::Body::Pointers(
                    payload
                        .iter()
                        .map(|w| source_oop(*w))
                        .collect::<Result<_>>()?,
                )
            }
            1 => {
                if payload.len() * 2 != length {
                    return Err(Error("word length mismatch".into()));
                }
                source::Body::Words(scalars(65535)?)
            }
            2 => {
                if payload.len() != length {
                    return Err(Error("byte length mismatch".into()));
                }
                source::Body::Bytes(scalars(255)?.into_iter().map(|v| v as u8).collect())
            }
            3 => {
                let header = layout::MethodHeader::parse(source_oop(
                    *payload
                        .first()
                        .ok_or_else(|| Error("missing method header".into()))?,
                )?)?;
                let pointers = 1 + header.literal_count();
                if payload.len() < pointers || payload.len() + pointers != length {
                    return Err(Error("method length mismatch".into()));
                }
                let literals = payload[1..pointers]
                    .iter()
                    .map(|w| source_oop(*w))
                    .collect::<Result<Vec<_>>>()?;
                let bytes = payload[pointers..]
                    .iter()
                    .map(|w| {
                        u8::try_from(w.bits())
                            .map_err(|_| Error("non-byte in method byte contents".into()))
                    })
                    .collect::<Result<Vec<_>>>()?;
                source::Body::Method {
                    header,
                    literals,
                    bytes,
                }
            }
            _ => unreachable!(),
        };
        Ok(source::Object {
            oop,
            class: source_oop(self.class)?,
            body,
        })
    }
}

impl Image {
    pub fn convert(source: &source::Image, roots: &[u16]) -> Result<Self> {
        source.validate()?;
        if roots.len() > 32 {
            return Err(Error("more than 32 explicit machine roots".into()));
        }
        source.object(layout::SMALL_INTEGER_CLASS)?;
        let mut root_words = [Word::ZERO; 32];
        for (slot, &oop) in roots.iter().enumerate() {
            source.object(oop)?;
            root_words[slot] = layout::reference(oop)?;
        }
        let mut primitives = BTreeMap::new();
        let mut records = Vec::new();
        for object in source.objects.values() {
            if let Some(primitive) = object.primitive()? {
                *primitives.entry(primitive).or_insert(0) += 1;
            }
            records.push(Record::convert(object)?);
        }
        Ok(Self {
            format_version: FORMAT_VERSION,
            source_sha256: source.sha256.clone(),
            next_identity: NEXT_IDENTITY,
            roots: root_words,
            compact_classes: [
                None,
                None,
                Some(layout::reference(layout::SMALL_INTEGER_CLASS)?),
                None,
            ],
            records,
            primitives,
        })
    }

    pub fn verify_against(&self, source: &source::Image) -> Result<()> {
        if self.records.len() != source.objects.len() {
            return Err(Error("object count changed during conversion".into()));
        }
        let mut seen = BTreeSet::new();
        for record in &self.records {
            if !seen.insert(record.source_oop) {
                return Err(Error("duplicate output object".into()));
            }
            if record.decode()? != *source.object(record.source_oop)? {
                return Err(Error(format!(
                    "oop 0x{:04x}: conversion changed guest contents",
                    record.source_oop
                )));
            }
            // Every tagged edge the generic collector sees must correspond
            // to an original guest edge. Scalar data cannot add false roots.
            let mut expected: Vec<_> = source
                .object(record.source_oop)?
                .references()
                .into_iter()
                .filter(|o| o & 1 == 0)
                .collect();
            let mut actual = vec![source_oop(record.class)?];
            for word in &record.body {
                if word.is_reference() {
                    actual.push(source_oop(*word)?);
                }
            }
            expected.sort_unstable();
            actual.sort_unstable();
            if expected != actual {
                return Err(Error("conversion changed collector-visible edges".into()));
            }
        }
        Ok(())
    }

    pub fn write_json(&self, output: impl std::io::Write) -> Result<()> {
        serde_json::to_writer(output, self).map_err(|e| Error(format!("write image bundle: {e}")))
    }
}

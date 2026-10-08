//! Offline Squeak conversion to ordinary scanned OBJEKT records.
//!
//! Component 1 holds `(guest_byte_length << 2) | kind` (pointer/word/byte/method).
//! Component 2 holds the saved 12-bit identity hash and original format in bits
//! 12..15. Guest fields start at component 3. Hashes remain independent of object
//! identities, so the guest's existing dictionaries need no rehash during import.
//!
//! This is a separate ABI from the Xerox converter. It preserves 32-bit words,
//! binary64 bits, method headers, literals and guest IPs without narrowing them.
//! No execution, primitive emulation or collection happens in this module.
use super::{
    image::{Body, Image as Source, Object},
    layout::{self, special, MethodHeader},
};
use crate::{Error, Result};
use rekursiv_asm::Word;
use serde::Serialize;
use std::collections::BTreeSet;

/// Explicit source profile prevents accidental use with Xerox microcode.
pub const PROFILE: &str = "squeak-1.1";

/// Raw 40-bit words, serialized as unsigned integers for loading while halted.
#[derive(Clone, Debug, Serialize)]
pub struct Record {
    pub source_oop: u32,
    pub reference: u64,
    pub class: u64,
    pub body: Vec<u64>,
}

/// Converted graph and initialization metadata. No source-address table is needed
/// at runtime: object fields and classes already contain complete machine tags.
#[derive(Clone, Debug, Serialize)]
pub struct Image {
    pub format_version: u32,
    pub profile: String,
    pub source_sha256: String,
    pub next_identity: u64,
    /// Root 0: specialObjectsArray, 1: saved context, 2: raw hash generator state.
    pub roots: [u64; 32],
    pub small_integer_class: u64,
    pub records: Vec<Record>,
}

/// Source addresses map to stable identities; no identity bits encode hashes.
pub fn reference(oop: u32) -> Result<Word> {
    if oop == 0 || !oop.is_multiple_of(4) {
        return Err(Error(format!("unaligned Squeak reference 0x{oop:08x}")));
    }
    Ok(Word::reference(u64::from(oop >> 2), true).unwrap())
}

/// Preserve SmallInteger sign and all 31 value bits in the generic compact type.
pub fn value(oop: u32) -> Result<Word> {
    if oop & 1 != 0 {
        Ok(Word::signed(layout::small_integer(oop)?))
    } else {
        reference(oop)
    }
}

fn source_value(bits: u64) -> Result<u32> {
    let word = Word::from_bits(bits).map_err(|_| Error("invalid target word".into()))?;
    if word.is_reference() {
        let id = word.identity().unwrap();
        if id <= u64::from(u32::MAX >> 2) && word == Word::reference(id, true).unwrap() {
            return Ok((id as u32) << 2);
        }
    } else if let Ok((2, payload)) = word.compact_parts() {
        let n = payload as i32;
        if (-(1 << 30)..(1 << 30)).contains(&n) {
            return Ok((payload << 1) | 1);
        }
    }
    Err(Error("target value is not an imported Squeak oop".into()))
}

impl Record {
    fn convert(object: &Object) -> Result<Self> {
        let mut body = vec![0, u64::from(object.hash) | (u64::from(object.format) << 12)];
        let (kind, length) = match &object.body {
            Body::Pointers(fields) => {
                for &field in fields {
                    body.push(value(field)?.bits());
                }
                (0, fields.len() * 4)
            }
            Body::Words(words) => {
                body.extend(words.iter().map(|w| u64::from(*w)));
                (1, words.len() * 4)
            }
            Body::Bytes(bytes) => {
                body.extend(bytes.iter().map(|b| u64::from(*b)));
                (2, bytes.len())
            }
            Body::Method {
                header,
                literals,
                bytes,
            } => {
                body.push(Word::signed(header.0 as i32).bits());
                for &literal in literals {
                    body.push(value(literal)?.bits());
                }
                body.extend(bytes.iter().map(|b| u64::from(*b)));
                (3, (literals.len() + 1) * 4 + bytes.len())
            }
        };
        body[0] = ((length as u64) << 2) | kind;
        Ok(Self {
            source_oop: object.oop,
            reference: reference(object.oop)?.bits(),
            class: reference(object.class)?.bits(),
            body,
        })
    }

    /// Decode the emitted ABI independently of the forward conversion.
    pub fn decode(&self) -> Result<Object> {
        if self.body.len() < 2 || self.body[0] >= (1 << 39) || self.body[1] > 65535 {
            return Err(Error(
                "invalid Squeak target descriptor or hash/format".into(),
            ));
        }
        let oop = source_value(self.reference)?;
        let class = source_value(self.class)?;
        if oop != self.source_oop || !oop.is_multiple_of(4) || !class.is_multiple_of(4) {
            return Err(Error("invalid Squeak target identity or class".into()));
        }
        let length = (self.body[0] >> 2) as usize;
        let payload = &self.body[2..];
        let scalars = |words: &[u64]| -> Result<Vec<u32>> {
            words
                .iter()
                .map(|w| {
                    u32::try_from(*w).map_err(|_| Error("non-word in Squeak word body".into()))
                })
                .collect()
        };
        let bytes = |words: &[u64]| -> Result<Vec<u8>> {
            words
                .iter()
                .map(|w| u8::try_from(*w).map_err(|_| Error("non-byte in Squeak byte body".into())))
                .collect()
        };
        let body = match self.body[0] & 3 {
            0 if payload.len() * 4 == length => Body::Pointers(
                payload
                    .iter()
                    .map(|w| source_value(*w))
                    .collect::<Result<_>>()?,
            ),
            1 if payload.len() * 4 == length => Body::Words(scalars(payload)?),
            2 if payload.len() == length => Body::Bytes(bytes(payload)?),
            3 => {
                let header = MethodHeader::parse(source_value(
                    *payload
                        .first()
                        .ok_or_else(|| Error("missing method header".into()))?,
                )?)?;
                let prefix = header.literal_count() + 1;
                if payload.len() < prefix || payload.len() + 3 * prefix != length {
                    return Err(Error("Squeak target method length mismatch".into()));
                }
                Body::Method {
                    header,
                    literals: payload[1..prefix]
                        .iter()
                        .map(|w| source_value(*w))
                        .collect::<Result<_>>()?,
                    bytes: bytes(&payload[prefix..])?,
                }
            }
            _ => return Err(Error("Squeak target body length mismatch".into())),
        };
        Ok(Object {
            oop,
            class,
            hash: (self.body[1] & 4095) as u16,
            format: (self.body[1] >> 12) as u8,
            body,
        })
    }
}

impl Image {
    /// Convert every allocated object, including objects outside the root graph.
    pub fn convert(source: &Source) -> Result<Self> {
        source.validate()?;
        let mut roots = [0; 32];
        roots[0] = reference(source.header.special_objects)?.bits();
        roots[1] = reference(source.initial_context()?)?.bits();
        roots[2] = u64::from(source.header.last_hash);
        let image = Self {
            format_version: 1,
            profile: PROFILE.into(),
            source_sha256: source.sha256.clone(),
            next_identity: u64::from(
                source
                    .objects
                    .keys()
                    .last()
                    .ok_or_else(|| Error("empty Squeak heap".into()))?
                    >> 2,
            ) + 1,
            roots,
            small_integer_class: reference(source.special(special::INTEGER_CLASS)?)?.bits(),
            records: source
                .objects
                .values()
                .map(Record::convert)
                .collect::<Result<_>>()?,
        };
        image.verify_against(source)?;
        Ok(image)
    }

    /// Check exact guest contents and every edge visible to the generic collector.
    pub fn verify_against(&self, source: &Source) -> Result<()> {
        if self.records.len() != source.objects.len()
            || self.profile != PROFILE
            || self.format_version != 1
            || self.source_sha256 != source.sha256
        {
            return Err(Error(
                "Squeak target profile/object count/checksum mismatch".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        for record in &self.records {
            let expected = source.object(record.source_oop)?;
            if !seen.insert(record.source_oop) || record.decode()? != *expected {
                return Err(Error(format!(
                    "Squeak conversion changed oop 0x{:08x}",
                    record.source_oop
                )));
            }
            let expected_edges: Vec<_> = expected
                .references()
                .into_iter()
                .filter(|v| v & 1 == 0)
                .collect();
            let actual_edges = std::iter::once(&record.class)
                .chain(&record.body)
                .filter(|w| Word::from_bits(**w).is_ok_and(|w| w.is_reference()))
                .map(|w| source_value(*w))
                .collect::<Result<Vec<_>>>()?;
            if actual_edges != expected_edges {
                return Err(Error("Squeak conversion changed collector edges".into()));
            }
        }
        let mut roots = [0; 32];
        roots[0] = reference(source.header.special_objects)?.bits();
        roots[1] = reference(source.initial_context()?)?.bits();
        roots[2] = u64::from(source.header.last_hash);
        let next = u64::from(*source.objects.keys().last().unwrap() >> 2) + 1;
        if self.roots != roots
            || self.next_identity != next
            || self.small_integer_class
                != reference(source.special(special::INTEGER_CLASS)?)?.bits()
        {
            return Err(Error("Squeak conversion changed boot metadata".into()));
        }
        Ok(())
    }
}

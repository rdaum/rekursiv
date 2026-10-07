//! Offline primitive inventory derived from the image's actual methods and
//! dictionaries. This reports declarations, not runtime reachability or proof
//! that a primitive's fallback succeeds.
use crate::{layout, source, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Method {
    pub oop: u16,
    pub arguments: u16,
    pub bytecodes: usize,
    pub bindings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Inventory {
    pub source_sha256: String,
    pub initial_context: u16,
    pub initial_method: u16,
    /// Zero means an extended method header without a primitive. Retain it
    /// explicitly rather than miscounting those methods as unsupported calls.
    pub primitives: BTreeMap<u8, Vec<Method>>,
}

fn bytes(image: &source::Image, oop: u16) -> Option<String> {
    let source::Body::Bytes(value) = &image.object(oop).ok()?.body else {
        return None;
    };
    String::from_utf8(value.clone()).ok()
}
fn class_name(image: &source::Image, oop: u16) -> Option<String> {
    let field = image.object(oop).ok()?.pointer(6).ok()?;
    bytes(image, field).or_else(|| {
        let instance_name = image.object(field).ok()?.pointer(6).ok()?;
        Some(format!("{} class", bytes(image, instance_name)?))
    })
}

pub fn inventory(image: &source::Image) -> Result<Inventory> {
    let mut bindings: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
    // Every instantiated class, its superclass chain, and metaclass instance
    // classes. The latter includes SmallInteger, whose instances are compact,
    // and classes with no stored instances in this particular image.
    let mut classes: BTreeSet<u16> = image.objects.values().map(|o| o.class).collect();
    let mut pending: Vec<_> = classes.iter().copied().collect();
    while let Some(class) = pending.pop() {
        let object = image.object(class)?;
        if let Ok(instance) = object.pointer(6) {
            if image.object(instance).is_ok_and(|object| {
                object.class == class && matches!(object.body, source::Body::Pointers(_))
            }) && classes.insert(instance)
            {
                pending.push(instance);
            }
        }
        let parent = object.pointer(layout::class::SUPERCLASS)?;
        if parent != layout::NIL && classes.insert(parent) {
            pending.push(parent);
        }
        let dict = object.pointer(layout::class::METHOD_DICTIONARY)?;
        if dict == layout::NIL {
            continue;
        }
        let dict = image.object(dict)?;
        let methods = image.object(dict.pointer(layout::method_dictionary::METHOD_ARRAY)?)?;
        let source::Body::Pointers(methods) = &methods.body else {
            continue;
        };
        let name = class_name(image, class).unwrap_or_else(|| format!("class@{class:04x}"));
        for (slot, method) in methods.iter().copied().enumerate() {
            if method == layout::NIL {
                continue;
            }
            let selector = dict.pointer(slot + layout::method_dictionary::SELECTOR_START)?;
            if let Some(selector) = bytes(image, selector) {
                bindings
                    .entry(method)
                    .or_default()
                    .insert(format!("{name}>>{selector}"));
            }
        }
    }
    let initial_context = image.initial_context()?;
    let initial_method = image
        .object(initial_context)?
        .pointer(layout::context::METHOD_OR_ARGUMENT_COUNT)?;
    let mut primitives: BTreeMap<u8, Vec<Method>> = BTreeMap::new();
    for object in image.objects.values() {
        let Some(primitive) = object.primitive()? else {
            continue;
        };
        let source::Body::Method {
            literals, bytes, ..
        } = &object.body
        else {
            unreachable!()
        };
        let extension = literals[literals.len() - 2];
        primitives.entry(primitive).or_default().push(Method {
            oop: object.oop,
            arguments: (extension >> 9) & 31,
            bytecodes: bytes.len(),
            bindings: bindings
                .remove(&object.oop)
                .unwrap_or_default()
                .into_iter()
                .collect(),
        });
    }
    Ok(Inventory {
        source_sha256: image.sha256.clone(),
        initial_context,
        initial_method,
        primitives,
    })
}

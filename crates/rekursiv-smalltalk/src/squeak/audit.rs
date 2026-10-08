//! Offline compatibility evidence from the actual saved Squeak object graph.
//!
//! Primitive declarations and bytecode occurrences are inventories, not proof of
//! runtime reachability or successful fallback. In particular, a shared primitive
//! number does not imply the Xerox implementation has Squeak semantics.
use super::{
    image::{Body, Header, Image},
    layout::{self, special},
};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// A method that declares a primitive, including codes above 255 for quick returns.
#[derive(Debug, Serialize)]
pub struct Method {
    pub oop: u32,
    pub arguments: usize,
    pub temporaries: usize,
    pub literals: usize,
    pub bindings: Vec<String>,
}

/// Guest state at the saved execution point. Offsets use Squeak's 32-bit prefix.
#[derive(Debug, Serialize)]
pub struct Startup {
    pub context: u32,
    pub method: u32,
    pub bindings: Vec<String>,
    pub instruction_pointer: usize,
    pub stack_pointer: usize,
    pub next_bytecode: u8,
}

/// The saved Form may be smaller than the window requested in the image header.
#[derive(Debug, Serialize)]
pub struct Display {
    pub form: u32,
    pub bitmap: u32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub bitmap_words: usize,
}

/// Serializable report, suitable for comparing this profile with the runtime.
#[derive(Debug, Serialize)]
pub struct Inventory {
    pub source_sha256: String,
    pub header: Header,
    pub objects: usize,
    pub free_bytes: usize,
    pub formats: BTreeMap<u8, usize>,
    pub methods: usize,
    pub floats: usize,
    pub startup: Startup,
    pub display: Option<Display>,
    pub primitives: BTreeMap<u16, Vec<Method>>,
}

fn text(image: &Image, oop: u32) -> Option<String> {
    let Body::Bytes(bytes) = &image.object(oop).ok()?.body else {
        return None;
    };
    Some(bytes.iter().map(|b| char::from(*b)).collect())
}

fn class_name(image: &Image, oop: u32) -> Option<String> {
    let name_or_instance = image.object(oop).ok()?.pointer(6).ok()?;
    text(image, name_or_instance).or_else(|| {
        let name = image.object(name_or_instance).ok()?.pointer(6).ok()?;
        Some(format!("{} class", text(image, name)?))
    })
}

/// Discover dictionary bindings through classes, including classes without instances.
pub fn method_bindings(image: &Image) -> Result<BTreeMap<u32, BTreeSet<String>>> {
    let nil = image.special(special::NIL)?;
    let mut bindings: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    let mut classes: BTreeSet<_> = image.objects.values().map(|o| o.class).collect();
    let mut pending: Vec<_> = classes.iter().copied().collect();
    while let Some(class) = pending.pop() {
        let object = image.object(class)?;
        if let Ok(instance) = object.pointer(6) {
            if image
                .object(instance)
                .is_ok_and(|o| o.class == class && matches!(o.body, Body::Pointers(_)))
                && classes.insert(instance)
            {
                pending.push(instance);
            }
        }
        let parent = object.pointer(0)?;
        if parent != nil && classes.insert(parent) {
            pending.push(parent);
        }
        let dict = object.pointer(1)?;
        if dict == nil {
            continue;
        }
        let dict = image.object(dict)?;
        let array = image.object(dict.pointer(1)?)?;
        let Body::Pointers(methods) = &array.body else {
            return Err(Error(
                "Squeak method dictionary has a nonpointer method Array".into(),
            ));
        };
        let name = class_name(image, class).unwrap_or_else(|| format!("class@{class:08x}"));
        for (slot, &method) in methods.iter().enumerate() {
            let selector = dict.pointer(slot + 2)?;
            if selector == nil {
                continue;
            }
            if !matches!(image.object(method)?.body, Body::Method { .. }) {
                return Err(Error(
                    "Squeak method dictionary entry is not a method".into(),
                ));
            }
            let selector = text(image, selector)
                .ok_or_else(|| Error("Squeak method selector is not a byte object".into()))?;
            bindings
                .entry(method)
                .or_default()
                .insert(format!("{name}>>{selector}"));
        }
    }
    Ok(bindings)
}

/// Check and describe the saved activation, display and primitive declarations.
pub fn inventory(image: &Image) -> Result<Inventory> {
    let bindings = method_bindings(image)?;
    let names = |oop| {
        bindings
            .get(&oop)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    };
    let context = image.initial_context()?;
    let activation = image.object(context)?;
    let home = if activation.pointer(3)? & 1 != 0 {
        image.object(activation.pointer(5)?)?
    } else {
        activation
    };
    let method = home.pointer(3)?;
    let Body::Method { header, bytes, .. } = &image.object(method)?.body else {
        return Err(Error(
            "Squeak saved context has a nonmethod method field".into(),
        ));
    };
    let ip = usize::try_from(layout::small_integer(activation.pointer(1)?)?)
        .map_err(|_| Error("negative Squeak saved IP".into()))?;
    let sp = usize::try_from(layout::small_integer(activation.pointer(2)?)?)
        .map_err(|_| Error("negative Squeak saved SP".into()))?;
    let offset = ip
        .checked_sub(header.initial_ip())
        .ok_or_else(|| Error("Squeak saved IP points into method literals".into()))?;
    let next = *bytes
        .get(offset)
        .ok_or_else(|| Error("Squeak saved IP exceeds method".into()))?;
    let Body::Pointers(fields) = &activation.body else {
        unreachable!()
    };
    if sp > fields.len().saturating_sub(6) {
        return Err(Error("Squeak saved SP exceeds context".into()));
    }
    let startup = Startup {
        context,
        method,
        bindings: names(method),
        instruction_pointer: ip,
        stack_pointer: sp,
        next_bytecode: next,
    };
    let display_oop = image.special(special::DISPLAY)?;
    let display = if display_oop == image.special(special::NIL)? {
        None
    } else {
        let form = image.object(display_oop)?;
        let bitmap = form.pointer(0)?;
        let Body::Words(words) = &image.object(bitmap)?.body else {
            return Err(Error("Squeak display bits are not a word object".into()));
        };
        Some(Display {
            form: display_oop,
            bitmap,
            width: layout::small_integer(form.pointer(1)?)?,
            height: layout::small_integer(form.pointer(2)?)?,
            depth: layout::small_integer(form.pointer(3)?)?,
            bitmap_words: words.len(),
        })
    };
    let mut formats = BTreeMap::new();
    let mut methods = 0;
    let mut floats = 0;
    let mut primitives: BTreeMap<u16, Vec<Method>> = BTreeMap::new();
    let float_class = image.special(special::FLOAT_CLASS)?;
    for object in image.objects.values() {
        *formats.entry(object.format).or_insert(0) += 1;
        if object.class == float_class {
            floats += 1;
        }
        if let Body::Method { header, .. } = object.body {
            methods += 1;
            if header.primitive() != 0 {
                primitives
                    .entry(header.primitive())
                    .or_default()
                    .push(Method {
                        oop: object.oop,
                        arguments: header.argument_count(),
                        temporaries: header.temporary_count(),
                        literals: header.literal_count(),
                        bindings: names(object.oop),
                    });
            }
        }
    }
    Ok(Inventory {
        source_sha256: image.sha256.clone(),
        header: image.header.clone(),
        objects: image.objects.len(),
        free_bytes: image.free_bytes,
        formats,
        methods,
        floats,
        startup,
        display,
        primitives,
    })
}

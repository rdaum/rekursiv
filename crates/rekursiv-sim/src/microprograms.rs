//! Rust-sequenced OBJEKT routines. All machine state travels through commands.
use crate::Harness;
use eyre::Result;
use rekursiv_asm::*;

pub const FRAME_WORDS: u32 = 13;

pub fn execute(h: &mut Harness<'_>, command: Command) -> Result<Word> {
    let response = h.execute(command)?;
    if response.status != Status::Ok {
        return Err(response.status.into());
    }
    Ok(response.data)
}

fn select(h: &mut Harness<'_>, value: Word) -> Result<()> {
    execute(
        h,
        if value.is_reference() {
            Command::fetch(value)
        } else {
            Command::probe(value)
        },
    )?;
    Ok(())
}

fn field(h: &mut Harness<'_>, index: i64) -> Result<Word> {
    execute(h, Command::index(index)?)?;
    execute(h, Command::read_field())
}

/// Select the target and load an index after checking its abstract access type.
/// Stored indices use one opaque word containing a signed 40-bit integer.
fn typed_index(h: &mut Harness<'_>, target: Word, index: Word) -> Result<()> {
    if !target.is_reference() {
        return Err(Status::BadValue.into());
    }
    select(h, target)?;
    let class = execute(h, Command::read(Read::Type))?;
    select(h, class)?;
    let access_type = field(h, 1)?;
    if access_type == Word::NIL {
        return Err(Status::TypeError.into());
    }
    if !access_type.is_reference() {
        return Err(Status::BadValue.into());
    }
    select(h, index)?;
    // The RTL performs the class comparison before it returns the payload.
    let payload = execute(
        h,
        Command {
            expected_type: Some(access_type),
            ..Command::read(Read::Representation)
        },
    )?;
    let numeric = if index.is_compact() {
        match index.compact_parts()?.0 {
            2 => payload.bits() as u32 as i32 as i64,
            3 => payload.bits() as i64,
            _ => return Err(Status::BadValue.into()),
        }
    } else {
        if index.bits() & (1 << 37) != 0 || execute(h, Command::read(Read::Size))?.bits() != 1 {
            return Err(Status::BadValue.into());
        }
        payload.as_index()
    };
    // Fetch also works when the class or index displaced the target's pager slot.
    select(h, target)?;
    execute(h, Command::index(numeric)?)?;
    Ok(())
}

pub fn typed_read(h: &mut Harness<'_>, target: Word, index: Word) -> Result<Word> {
    typed_index(h, target, index)?;
    execute(h, Command::read_field())
}

pub fn typed_write(h: &mut Harness<'_>, target: Word, index: Word, value: Word) -> Result<()> {
    typed_index(h, target, index)?;
    execute(h, Command::write_field(value))?;
    Ok(())
}

/// Alternating key/value fields, odd starting field, power-of-two body size.
/// Nil is the empty-key sentinel. IndexReg retains the starting field.
pub fn lookup(h: &mut Harness<'_>, object: Word, key: Word, start: i64) -> Result<Option<Word>> {
    if !object.is_reference() || key == Word::NIL {
        return Err(Status::BadValue.into());
    }
    select(h, object)?;
    let size = execute(h, Command::read(Read::Size))?.bits();
    if size < 2 || !size.is_power_of_two() || start < 1 || start as u64 > size || start % 2 != 1 {
        return Err(Status::BadValue.into());
    }
    execute(h, Command::index(start)?)?;
    execute(
        h,
        Command {
            register: Register::FromIndex,
            ..Command::default()
        },
    )?;
    for _ in 0..size / 2 {
        let found = execute(h, Command::read_field())?;
        if found == Word::NIL {
            return Ok(None);
        }
        let next = Command {
            index: Index::Next,
            ..Command::default()
        };
        execute(h, next)?;
        if found == key {
            return Ok(Some(execute(h, Command::read_field())?));
        }
        execute(h, next)?;
        let index = execute(h, Command::read(Read::Index))?;
        let start = execute(h, Command::read(Read::IndexReg))?;
        if index == start {
            return Ok(None);
        }
    }
    eyre::bail!("dictionary scan did not return to its starting field")
}

/// OBJEKT execution context, excluding global class mappings and pager residency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub selected: Word,
    pub vr: [Word; 8],
    pub index: Word,
    pub index_reg: Word,
}

pub fn read_context(h: &mut Harness<'_>) -> Result<Context> {
    let selected = execute(h, Command::read(Read::Reference))?;
    let index = execute(h, Command::read(Read::Index))?;
    let index_reg = execute(h, Command::read(Read::IndexReg))?;
    let mut vr = [Word::NIL; 8];
    for (slot, value) in vr.iter_mut().enumerate() {
        *value = execute(
            h,
            Command {
                vr: slot as u8,
                ..Command::read(Read::Vr)
            },
        )?;
    }
    Ok(Context {
        selected,
        vr,
        index,
        index_reg,
    })
}

pub fn save_context(h: &mut Harness<'_>, frame_class: Word) -> Result<Word> {
    let context = read_context(h)?;
    let frame = execute(h, Command::allocate(frame_class, FRAME_WORDS, true)?)?;
    let mut words = vec![context.selected];
    words.extend(context.vr);
    // Compact limbs prevent arbitrary signed index bits from becoming GC edges.
    for index in [context.index, context.index_reg] {
        words.push(Word::unsigned(index.bits() as u32));
        words.push(Word::unsigned((index.bits() >> 32) as u32));
    }
    for (i, value) in words.into_iter().enumerate() {
        execute(h, Command::index(i as i64 + 1)?)?;
        execute(h, Command::write_field(value))?;
    }
    Ok(frame)
}

pub fn restore_context(h: &mut Harness<'_>, frame: Word, frame_class: Word) -> Result<()> {
    if !frame.is_reference() || frame.bits() & (1 << 37) == 0 {
        return Err(Status::BadValue.into());
    }
    select(h, frame)?;
    let size = execute(
        h,
        Command {
            expected_type: Some(frame_class),
            ..Command::read(Read::Size)
        },
    )?;
    if size.bits() != FRAME_WORDS as u64 {
        return Err(Status::BadValue.into());
    }
    let mut words = Vec::new();
    for i in 1..=FRAME_WORDS {
        words.push(field(h, i as i64)?);
    }
    let mut indices = [Word::ZERO; 2];
    for (i, pair) in words[9..].as_chunks::<2>().0.iter().enumerate() {
        let (low_code, low) = pair[0].compact_parts()?;
        let (high_code, high) = pair[1].compact_parts()?;
        if low_code != 3 || high_code != 3 || high > 255 {
            return Err(Status::BadValue.into());
        }
        indices[i] = Word::from_bits((high as u64) << 32 | low as u64)?;
    }
    // Read and validate the complete frame before changing any value register.
    select(h, words[0])?;
    for (vr, &value) in words[1..9].iter().enumerate() {
        execute(
            h,
            Command {
                load_vr: true,
                vr: vr as u8,
                data: value,
                ..Command::default()
            },
        )?;
    }
    execute(h, Command::index(indices[0].as_index())?)?;
    execute(
        h,
        Command {
            register: Register::Load,
            data: indices[1],
            ..Command::default()
        },
    )?;
    Ok(())
}

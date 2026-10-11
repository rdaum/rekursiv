//! Native execution of the resident OBJEKT datapath. Controls are compile-time
//! constants; old operands remain SSA values until all command checks succeed.
use super::emit::{Emit, Mode};
use cranelift_codegen::ir::{
    condcodes::IntCC, types::*, BlockArg, InstBuilder, MemFlagsData, Value,
};
use rekursiv_asm::{
    Command, Entry, Index, Memory, Pager, Read, Register, Status, Word, ADDRESS_LIMIT, ID_MASK,
    WORD_MASK,
};
use rekursiv_model::{Model, PreparedAccess, Slot, State};
use std::mem::{offset_of, size_of};

/// Borrowed vectors, rebuilt at every entry. No generated code assumes Vec's
/// layout or retains pointers across allocation, collection, or external edits.
#[repr(C)]
pub(super) struct MemoryView {
    entries: *mut Slot<Entry>,
    entries_len: usize,
    words: *mut Word,
    words_len: usize,
}
impl MemoryView {
    pub(super) fn new(model: &mut Model) -> Self {
        Self {
            entries: model.entries.as_mut_ptr(),
            entries_len: model.entries.len(),
            words: model.memory.as_mut_ptr(),
            words_len: model.memory.len(),
        }
    }
}
#[derive(Clone, Copy)]
struct Metadata {
    reference: Value,
    class: Value,
    size: Value,
    base: Value,
    representation: Value,
    new: Value,
    modified: Value,
    cond: Value,
}
impl Metadata {
    fn values(self) -> [Value; 8] {
        [
            self.reference,
            self.class,
            self.size,
            self.base,
            self.representation,
            self.new,
            self.modified,
            self.cond,
        ]
    }
    fn from_values(v: &[Value]) -> Self {
        Self {
            reference: v[0],
            class: v[1],
            size: v[2],
            base: v[3],
            representation: v[4],
            new: v[5],
            modified: v[6],
            cond: v[7],
        }
    }
}
impl Emit<'_> {
    fn model(&self) -> Value {
        let Mode::Block { model, .. } = self.mode else {
            unreachable!()
        };
        model
    }
    fn view(&self) -> Value {
        let Mode::Block { memory_view, .. } = self.mode else {
            unreachable!()
        };
        memory_view
    }
    fn constant(&mut self, value: i64) -> Value {
        self.b.ins().iconst(I64, value)
    }
    fn read64(&mut self, base: Value, offset: usize) -> Value {
        self.b
            .ins()
            .load(I64, MemFlagsData::trusted(), base, offset as i32)
    }
    fn read32(&mut self, base: Value, offset: usize) -> Value {
        let value = self
            .b
            .ins()
            .load(I32, MemFlagsData::trusted(), base, offset as i32);
        self.b.ins().uextend(I64, value)
    }
    fn read8(&mut self, base: Value, offset: usize) -> Value {
        let value = self
            .b
            .ins()
            .load(I8, MemFlagsData::trusted(), base, offset as i32);
        self.b.ins().uextend(I64, value)
    }
    fn write64(&mut self, base: Value, offset: usize, value: Value) {
        self.b
            .ins()
            .store(MemFlagsData::trusted(), value, base, offset as i32);
    }
    fn write8(&mut self, base: Value, offset: usize, value: Value) {
        let value = self.b.ins().ireduce(I8, value);
        self.b
            .ins()
            .store(MemFlagsData::trusted(), value, base, offset as i32);
    }
    fn pointer_index(&mut self, value: Value) -> Value {
        if self.pointer == I64 {
            value
        } else {
            self.b.ins().ireduce(self.pointer, value)
        }
    }
    fn view_length(&mut self, offset: usize) -> Value {
        let view = self.view();
        let v = self
            .b
            .ins()
            .load(self.pointer, MemFlagsData::trusted(), view, offset as i32);
        if self.pointer == I64 {
            v
        } else {
            self.b.ins().uextend(I64, v)
        }
    }
    fn pager_slot(&mut self, reference: Value) -> Value {
        let view = self.view();
        let len = self.view_length(offset_of!(MemoryView, entries_len));
        let mask = self.b.ins().iadd_imm_s(len, -1);
        let index = self.b.ins().band(reference, mask);
        let index = self.pointer_index(index);
        let bytes = self
            .b
            .ins()
            .imul_imm_s(index, size_of::<Slot<Entry>>() as i64);
        let base = self.b.ins().load(
            self.pointer,
            MemFlagsData::trusted(),
            view,
            offset_of!(MemoryView, entries) as i32,
        );
        self.b.ins().iadd(base, bytes)
    }
    fn load_metadata(&mut self, base: Value, offset: usize) -> Metadata {
        Metadata {
            reference: self.read64(base, offset + offset_of!(Entry, reference)),
            class: self.read64(base, offset + offset_of!(Entry, class)),
            size: self.read32(base, offset + offset_of!(Entry, size)),
            base: self.read32(base, offset + offset_of!(Entry, base)),
            representation: self.read64(base, offset + offset_of!(Entry, representation)),
            new: self.read8(base, offset + offset_of!(Entry, new)),
            modified: self.read8(base, offset + offset_of!(Entry, modified)),
            cond: self.read8(base, offset + offset_of!(Entry, cond)),
        }
    }
    fn store_selection(&mut self, e: Metadata) {
        let model = self.model();
        let offset =
            offset_of!(Model, state) + offset_of!(State, selected) + offset_of!(Slot<Entry>, value);
        for (field, value) in [
            (offset_of!(Entry, reference), e.reference),
            (offset_of!(Entry, class), e.class),
            (offset_of!(Entry, representation), e.representation),
        ] {
            self.write64(model, offset + field, value);
        }
        for (field, value) in [
            (offset_of!(Entry, size), e.size),
            (offset_of!(Entry, base), e.base),
        ] {
            let value = self.b.ins().ireduce(I32, value);
            self.b.ins().store(
                MemFlagsData::trusted(),
                value,
                model,
                (offset + field) as i32,
            );
        }
        for (field, value) in [
            (offset_of!(Entry, new), e.new),
            (offset_of!(Entry, modified), e.modified),
            (offset_of!(Entry, cond), e.cond),
        ] {
            self.write8(model, offset + field, value);
        }
        let one = self.constant(1);
        self.write8(
            model,
            offset_of!(Model, state)
                + offset_of!(State, selected)
                + offset_of!(Slot<Entry>, present),
            one,
        );
    }
    fn resolve_guard(
        &mut self,
        bad: Value,
        status: Status,
        done: cranelift_codegen::ir::Block,
        zero: Value,
    ) {
        let failed = self.b.create_block();
        let good = self.b.create_block();
        self.b.ins().brif(bad, failed, &[], good, &[]);
        self.b.switch_to_block(failed);
        let status = self.constant(status as i64);
        let mut args: Vec<BlockArg> = vec![zero.into(); 8];
        args.push(status.into());
        self.b.ins().jump(done, &args);
        self.b.switch_to_block(good);
    }
    fn resolved_jump(&mut self, done: cranelift_codegen::ir::Block, e: Metadata, status: Value) {
        let mut args: Vec<BlockArg> = e.values().into_iter().map(Into::into).collect();
        args.push(status.into());
        self.b.ins().jump(done, &args);
    }
    // Return a status rather than raising it: PREPARE records lookup failures
    // for a later consumer, while probes and field accesses report them now.
    fn resolve_native(&mut self, reference: Value) -> (Metadata, Value) {
        let model = self.model();
        let done = self.b.create_block();
        for _ in 0..9 {
            self.b.append_block_param(done, I64);
        }
        let stored = self.b.create_block();
        let compact = self.b.create_block();
        let tag = self.b.ins().ushr_imm_s(reference, 38);
        let is_compact = self.b.ins().icmp_imm_s(IntCC::Equal, tag, 3);
        let zero = self.constant(0);
        self.b.ins().brif(is_compact, compact, &[], stored, &[]);
        self.b.switch_to_block(stored);
        let bad_tag = self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, 2);
        let identity = self.b.ins().band_imm_s(reference, ID_MASK as i64);
        let null = self.b.ins().icmp_imm_s(IntCC::Equal, identity, 0);
        let invalid = self.b.ins().bor(bad_tag, null);
        self.resolve_guard(invalid, Status::InvalidReference, done, zero);
        let slot = self.pager_slot(reference);
        let present = self.read8(slot, offset_of!(Slot<Entry>, present));
        let missing = self.b.ins().icmp_imm_s(IntCC::Equal, present, 0);
        self.resolve_guard(missing, Status::NotResident, done, zero);
        let e = self.load_metadata(slot, offset_of!(Slot<Entry>, value));
        let collision = self.b.ins().icmp(IntCC::NotEqual, reference, e.reference);
        self.resolve_guard(collision, Status::NotResident, done, zero);
        self.resolved_jump(done, e, zero);
        self.b.switch_to_block(compact);
        let code = self.b.ins().ushr_imm_s(reference, 32);
        let code = self.b.ins().band_imm_s(code, 63);
        let payload = self.b.ins().band_imm_s(reference, 0xffff_ffff);
        let bad_code = self.b.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, code, 3);
        self.resolve_guard(bad_code, Status::BadValue, done, zero);
        let small_code = self.b.ins().icmp_imm_s(IntCC::UnsignedLessThan, code, 2);
        let too_large = self.b.ins().icmp(IntCC::UnsignedGreaterThan, payload, code);
        let bad_value = self.b.ins().band(small_code, too_large);
        self.resolve_guard(bad_value, Status::BadValue, done, zero);
        let offset = self
            .b
            .ins()
            .imul_imm_s(code, size_of::<Slot<Word>>() as i64);
        let offset = self.pointer_index(offset);
        let array = self
            .b
            .ins()
            .iadd_imm_s(model, offset_of!(Model, classes) as i64);
        let slot = self.b.ins().iadd(array, offset);
        let present = self.read8(slot, offset_of!(Slot<Word>, present));
        let missing = self.b.ins().icmp_imm_s(IntCC::Equal, present, 0);
        self.resolve_guard(missing, Status::BadValue, done, zero);
        let class = self.read64(slot, offset_of!(Slot<Word>, value));
        self.resolved_jump(
            done,
            Metadata {
                reference,
                class,
                size: zero,
                base: zero,
                representation: payload,
                new: zero,
                modified: zero,
                cond: zero,
            },
            zero,
        );
        self.b.switch_to_block(done);
        let values = self.b.block_params(done).to_vec();
        (Metadata::from_values(&values), values[8])
    }
    fn selected_snapshot(&mut self) -> (Metadata, Value) {
        let offset = offset_of!(Model, state) + offset_of!(State, selected);
        let e = self.load_metadata(self.model(), offset + offset_of!(Slot<Entry>, value));
        let present = self.read8(self.model(), offset + offset_of!(Slot<Entry>, present));
        (e, present)
    }
    fn selected_native(&mut self) -> (Metadata, Value) {
        let (snapshot, present) = self.selected_snapshot();
        let (entry, status) = self.resolve_native(snapshot.reference);
        let absent = self.constant(Status::NoSelection as i64);
        let status = self.b.ins().select(present, status, absent);
        (entry, status)
    }
    fn required_selection(&mut self) -> Metadata {
        let (entry, status) = self.selected_native();
        self.object_failure(status);
        entry
    }
    pub(super) fn native_resident(&mut self, d: Value, c: Command) -> bool {
        let model = self.model();
        let view = self.view();
        if !super::resident::eligible(c) {
            return false;
        }
        if c.pager == Pager::Fetch {
            let (entry, status) = self.resolve_native(d);
            self.reject(status, super::resident::TRANSFER as i64);
            let maintenance = self.read8(model, offset_of!(Model, maintenance));
            self.object_check(maintenance, Status::BadCommand);
            self.store_selection(entry);
            self.finish_object(entry.reference);
            return true;
        }
        let maintenance = self.read8(model, offset_of!(Model, maintenance));
        self.object_check(maintenance, Status::BadCommand);
        let state = offset_of!(Model, state);
        let old_index = self.read64(model, state + offset_of!(State, index));
        let old_reg = self.read64(model, state + offset_of!(State, index_reg));
        let shifted = self.b.ins().ishl_imm_s(d, 24);
        let signed = self.b.ins().sshr_imm_s(shifted, 24);
        let mut selection: Option<(Metadata, Value)> = None;
        let one = self.constant(1);
        let index = match c.index {
            Index::None => old_index,
            Index::Load => signed,
            Index::Clear => self.constant(0),
            Index::One => one,
            Index::Two => self.constant(2),
            Index::Increment => self.b.ins().iadd_imm_s(old_index, 1),
            Index::Decrement => self.b.ins().iadd_imm_s(old_index, -1),
            Index::Step => self.b.ins().iadd(old_index, signed),
            Index::FromReg => old_reg,
            Index::Next => {
                let e = self.required_selection();
                let empty = self.b.ins().icmp_imm_s(IntCC::Equal, e.size, 0);
                let below = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, old_index, 0);
                let above = self
                    .b
                    .ins()
                    .icmp(IntCC::SignedGreaterThan, old_index, e.size);
                let bad = self.b.ins().bor(empty, below);
                let bad = self.b.ins().bor(bad, above);
                self.object_check(bad, Status::BoundsError);
                selection = Some((e, one));
                let end = self.b.ins().icmp(IntCC::Equal, old_index, e.size);
                let next = self.b.ins().iadd_imm_s(old_index, 1);
                self.b.ins().select(end, one, next)
            }
        };
        self.index_check(index);
        let prepared = if c.prepare {
            let (snapshot, present) = self.selected_snapshot();
            let nil = self.constant(Word::NIL.bits() as i64);
            let reference = self.b.ins().select(present, snapshot.reference, nil);
            let (e, lookup_status) = self.selected_native();
            let address_end = self.b.ins().iadd(e.base, index);
            let memory_len = self.view_length(offset_of!(MemoryView, words_len));
            let below = self
                .b
                .ins()
                .icmp_imm_s(IntCC::SignedLessThanOrEqual, index, 0);
            let above = self.b.ins().icmp(IntCC::SignedGreaterThan, index, e.size);
            let outside = self
                .b
                .ins()
                .icmp(IntCC::UnsignedGreaterThan, address_end, memory_len);
            let bad = self.b.ins().bor(below, above);
            let bad = self.b.ins().bor(bad, outside);
            let bounds = self.constant(Status::BoundsError as i64);
            let ok = self.constant(0);
            let checked = self.b.ins().select(bad, bounds, ok);
            let status = self.b.ins().select(lookup_status, lookup_status, checked);
            let address = self.b.ins().iadd_imm_s(address_end, -1);
            let address = self.b.ins().select(status, ok, address);
            Some((reference, address, status))
        } else {
            None
        };
        let reg = match c.register {
            Register::None => old_reg,
            Register::Load => signed,
            Register::Increment => self.b.ins().iadd_imm_s(old_reg, 1),
            Register::Decrement => self.b.ins().iadd_imm_s(old_reg, -1),
            Register::FromIndex => old_index,
        };
        self.index_check(reg);
        let mut output = self.constant(Word::NIL.bits() as i64);
        let vr_offset = state + offset_of!(State, vr) + c.vr as usize * size_of::<Word>();
        if c.pager != Pager::None {
            let reference = match c.pager {
                Pager::ProbeBus => d,
                Pager::ProbeVr => self.read64(model, vr_offset),
                Pager::ProbeType => self.required_selection().class,
                Pager::ProbeRepresentation => self.required_selection().representation,
                _ => unreachable!(),
            };
            let (e, status) = self.resolve_native(reference);
            self.object_failure(status);
            output = e.reference;
            selection = Some((e, one));
        }
        let needs_metadata = matches!(
            c.read,
            Read::Size | Read::Type | Read::Base | Read::Representation | Read::Flags
        ) || c.expected_type.is_some()
            || (c.memory != Memory::None && !c.prepared);
        let old_entry = if needs_metadata {
            let e = self.required_selection();
            if c.pager == Pager::None {
                selection = Some((e, one));
            }
            if let Some(expected) = c.expected_type {
                let bad = self
                    .b
                    .ins()
                    .icmp_imm_s(IntCC::NotEqual, e.class, expected.bits() as i64);
                self.object_check(bad, Status::TypeError);
            }
            Some(e)
        } else {
            None
        };
        output = match c.read {
            Read::None => output,
            Read::Vr => self.read64(model, vr_offset),
            Read::Reference => {
                let (snapshot, present) = self.selected_snapshot();
                let missing = self.b.ins().icmp_imm_s(IntCC::Equal, present, 0);
                self.object_check(missing, Status::NoSelection);
                snapshot.reference
            }
            Read::Index | Read::IndexReg => {
                let old = if c.read == Read::Index {
                    old_index
                } else {
                    old_reg
                };
                self.index_check(old);
                self.b.ins().band_imm_s(old, WORD_MASK as i64)
            }
            Read::Size => old_entry.unwrap().size,
            Read::Base => old_entry.unwrap().base,
            Read::Type => old_entry.unwrap().class,
            Read::Representation => old_entry.unwrap().representation,
            Read::Flags => {
                let e = old_entry.unwrap();
                let modified = self.b.ins().ishl_imm_s(e.modified, 1);
                let cond = self.b.ins().ishl_imm_s(e.cond, 2);
                let flags = self.b.ins().bor(e.new, modified);
                self.b.ins().bor(flags, cond)
            }
            Read::FreeWords | Read::FreeIdentities => {
                let (limit, used) = if c.read == Read::FreeWords {
                    (
                        self.read32(model, offset_of!(Model, allocation_limit)),
                        self.read32(model, offset_of!(Model, body_cursor)),
                    )
                } else {
                    (
                        self.constant((ID_MASK + 1) as i64),
                        self.read64(model, offset_of!(Model, next_identity)),
                    )
                };
                let excess = self.b.ins().icmp(IntCC::UnsignedGreaterThan, used, limit);
                let difference = self.b.ins().isub(limit, used);
                let zero = self.constant(0);
                self.b.ins().select(excess, zero, difference)
            }
        };
        if c.memory != Memory::None {
            let (mut e, index, address) = if c.prepared {
                let offset = state + offset_of!(State, prepared);
                let status = self.read8(model, offset + offset_of!(PreparedAccess, status));
                self.object_failure(status);
                let reference = self.read64(model, offset + offset_of!(PreparedAccess, reference));
                let (e, status) = self.resolve_native(reference);
                self.object_failure(status);
                let index = self.read64(model, offset + offset_of!(PreparedAccess, index));
                let address = self.read32(model, offset + offset_of!(PreparedAccess, address));
                (e, index, address)
            } else {
                let e = old_entry.unwrap();
                let address = self.b.ins().iadd(e.base, old_index);
                (e, old_index, self.b.ins().iadd_imm_s(address, -1))
            };
            let below = self
                .b
                .ins()
                .icmp_imm_s(IntCC::SignedLessThanOrEqual, index, 0);
            let above = self.b.ins().icmp(IntCC::SignedGreaterThan, index, e.size);
            let bad = self.b.ins().bor(below, above);
            self.object_check(bad, Status::BoundsError);
            let len = self.view_length(offset_of!(MemoryView, words_len));
            let outside = self
                .b
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, address, len);
            let overflow = self.b.ins().icmp_imm_s(
                IntCC::UnsignedGreaterThanOrEqual,
                address,
                ADDRESS_LIMIT as i64,
            );
            let bad = self.b.ins().bor(outside, overflow);
            self.object_check(bad, Status::BoundsError);
            let first = self.b.ins().icmp_imm_s(IntCC::Equal, index, 1);
            let address = self.pointer_index(address);
            let bytes = self.b.ins().imul_imm_s(address, size_of::<Word>() as i64);
            let ram = self.b.ins().load(
                self.pointer,
                MemFlagsData::trusted(),
                view,
                offset_of!(MemoryView, words) as i32,
            );
            let location = self.b.ins().iadd(ram, bytes);
            if c.memory == Memory::Read {
                let cached = self.b.create_block();
                let memory = self.b.create_block();
                let done = self.b.create_block();
                self.b.append_block_param(done, I64);
                self.b.ins().brif(first, cached, &[], memory, &[]);
                self.b.switch_to_block(cached);
                self.b.ins().jump(done, &[e.representation.into()]);
                self.b.switch_to_block(memory);
                let word = self.read64(location, 0);
                self.b.ins().jump(done, &[word.into()]);
                self.b.switch_to_block(done);
                output = self.b.block_params(done)[0];
            } else {
                // All failure checks precede RAM or metadata publication.
                self.write64(location, 0, d);
                e.modified = one;
                e.representation = self.b.ins().select(first, d, e.representation);
                let slot = self.pager_slot(e.reference);
                self.write8(
                    slot,
                    offset_of!(Slot<Entry>, value) + offset_of!(Entry, modified),
                    one,
                );
                self.write64(
                    slot,
                    offset_of!(Slot<Entry>, value) + offset_of!(Entry, representation),
                    e.representation,
                );
                let (candidate, present) = selection.unwrap_or_else(|| self.selected_snapshot());
                let same = self
                    .b
                    .ins()
                    .icmp(IntCC::Equal, candidate.reference, e.reference);
                let present_bool = self.b.ins().icmp_imm_s(IntCC::NotEqual, present, 0);
                let update = self.b.ins().band(same, present_bool);
                if selection.is_some() {
                    let values: Vec<_> = e
                        .values()
                        .into_iter()
                        .zip(candidate.values())
                        .map(|(new, old)| self.b.ins().select(update, new, old))
                        .collect();
                    selection = Some((Metadata::from_values(&values), one));
                } else {
                    selection = Some((e, update));
                }
                output = d;
            }
        }
        self.write64(model, state + offset_of!(State, index), index);
        self.write64(model, state + offset_of!(State, index_reg), reg);
        if let Some((e, publish)) = selection {
            let store = self.b.create_block();
            let done = self.b.create_block();
            self.b.ins().brif(publish, store, &[], done, &[]);
            self.b.switch_to_block(store);
            self.store_selection(e);
            self.b.ins().jump(done, &[]);
            self.b.switch_to_block(done);
        }
        if let Some((reference, address, status)) = prepared {
            let offset = state + offset_of!(State, prepared);
            self.write64(
                model,
                offset + offset_of!(PreparedAccess, reference),
                reference,
            );
            self.write64(model, offset + offset_of!(PreparedAccess, index), index);
            let address = self.b.ins().ireduce(I32, address);
            self.b.ins().store(
                MemFlagsData::trusted(),
                address,
                model,
                (offset + offset_of!(PreparedAccess, address)) as i32,
            );
            self.write8(model, offset + offset_of!(PreparedAccess, status), status);
        }
        if c.load_vr {
            self.write64(model, vr_offset, d);
        }
        self.finish_object(output);
        true
    }
    fn finish_object(&mut self, output: Value) {
        if let Mode::Block {
            object_commands, ..
        } = &mut self.mode
        {
            *object_commands += 1;
        }
        self.deferred_object = Some(output);
    }
}

use eyre::{ensure, Result};
use rekursiv_asm::{processor::*, Word};
use rekursiv_emulator::{Machine, Step};
use rekursiv_model::processor::{Image, Processor};
use rekursiv_sim::{Harness, Timing};

fn simultaneous_image() -> Image {
    let mut image = Image::program(&[
        Instruction {
            data: Word::raw(3).unwrap(),
            sp: Pointer::Bus,
            esp: Address::Sp,
            csp: Pointer::Bus,
            load_ap: true,
            apc: Apc::Bus,
            estk: Estk::Bus,
            cstk: Cstk::Bus,
            ..Default::default()
        },
        Instruction {
            data: Word::raw(5).unwrap(),
            r: Source::Bus,
            load_q: true,
            sp: Pointer::Increment,
            esp: Address::Sp,
            csp: Pointer::Decrement,
            estk: Estk::Alu,
            cstk: Cstk::Increment,
            flags: true,
            ..Default::default()
        },
        Instruction {
            data: Word::raw(0xab).unwrap(),
            r: Source::Q,
            fetch: Fetch::Both,
            apc: Apc::Increment,
            esp: Address::Argument,
            branch: 2,
            estk: Estk::Wide,
            cstk: Cstk::Upcor,
            ..Default::default()
        },
        Instruction {
            fetch: Fetch::Both,
            seq: Seq::Dispatch,
            estk: Estk::Read,
            cstk: Cstk::Read,
            ..Default::default()
        },
    ]);
    image.nam[3] = Some((7 << 30) | 19);
    image.nam[4] = Some((8 << 30) | 29);
    image.map[0] = Some(17);
    image.map[7] = Some(8);
    image.code[17] = Some(Instruction::halt());
    image
}

#[test]
fn simultaneous_stack_and_fetch_retirements_match_rtl_and_native_blocks() -> Result<()> {
    let image = simultaneous_image();
    let mut jit = Machine::new(image.clone(), 0, 16, 512)?;
    let mut blocks = Machine::new(image.clone(), 0, 16, 512)?;
    jit.enable_jit_eager()?;
    blocks.enable_jit_eager()?;
    let rt = rekursiv_sim::runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut oracle = Processor::default();
    h.run_processor_observed(&image, &mut oracle, 1000, |_, expected| {
        assert_eq!(jit.step()?, Step::Retired);
        ensure!(jit.cpu == *expected, "state mismatch at {}", expected.pc);
        Ok(())
    })?;
    blocks.run_steps(6)?;
    assert_eq!(jit.cpu, blocks.cpu);
    assert_eq!(jit.stats, blocks.stats);
    assert_eq!(blocks.cpu.estk[0], 3);
    assert_eq!(blocks.cpu.estk[3], 5);
    assert_eq!(blocks.cpu.estk[4], 0xab00000005);
    assert_eq!(blocks.cpu.cstk[3], 4);
    assert_eq!(blocks.cpu.apc, 4);
    assert_eq!(
        (blocks.cpu.opcode, blocks.cpu.namarg, blocks.cpu.ucar),
        (8, 29, 8)
    );
    assert_eq!(blocks.jit_statistics().unwrap().block_instructions, 4);
    Ok(())
}

#[test]
fn fetch_uses_live_tables_and_missing_slots_do_not_publish_writes() -> Result<()> {
    let image = Image::program(&[
        Instruction {
            fetch: Fetch::Both,
            apc: Apc::Increment,
            estk: Estk::Bus,
            data: Word::raw(17)?,
            ..Default::default()
        },
        Instruction {
            fetch: Fetch::Both,
            ..Default::default()
        },
        Instruction::halt(),
    ]);
    let mut m = Machine::new(image, 0, 16, 512)?;
    m.enable_jit_eager()?;
    // Both tables were empty when compiled. Populate/replace their allocations
    // afterward: generated code must never embed their data pointers or values.
    m.image_mut().nam = vec![Some((3 << 30) | 123), Some((5 << 30) | 999)];
    m.image_mut().map = vec![Some(31), None, None, Some(41)];
    m.run_steps(2)?;
    assert_eq!((m.cpu.opcode, m.cpu.namarg, m.cpu.ucar), (5, 999, 41));
    assert_eq!(m.cpu.estk[0], 17);
    for missing_nam in [false, true] {
        m.cpu.pc = 0;
        m.cpu.apc = 0;
        m.cpu.opcode = if missing_nam { 0 } else { usize::MAX };
        m.fault = None;
        m.cpu.halted = false;
        if missing_nam {
            m.image_mut().nam.clear();
        }
        let before = m.cpu.clone();
        assert!(m.run_steps(2).is_err());
        let mut expected = before;
        expected.halted = true;
        assert_eq!(m.cpu, expected);
        assert_eq!(m.fault.unwrap().code, 5);
    }
    Ok(())
}

#[test]
fn blocking_object_failure_discards_prepared_stack_and_fetch_destinations() -> Result<()> {
    let mut image = simultaneous_image();
    let i = image.code[0].as_mut().unwrap();
    i.object = Some(rekursiv_asm::Command::read_field());
    i.fetch = Fetch::Both;
    image.nam[0] = Some((7 << 30) | 123);
    let mut m = Machine::new(image, 0, 16, 512)?;
    m.enable_jit_eager()?;
    let mut expected = m.cpu.clone();
    expected.halted = true;
    assert!(m.step().is_err());
    assert_eq!(m.fault.unwrap().code, 4);
    assert_eq!(m.cpu, expected);
    assert_eq!(m.stats.retired, 0);
    assert_eq!(m.jit_statistics().unwrap().preparations, 1);
    Ok(())
}

#[test]
fn compact_stack_fault_retains_only_earlier_block_retirements() -> Result<()> {
    let code = [
        Instruction {
            data: Word::raw(7)?,
            estk: Estk::Bus,
            esp: Address::Sp,
            sp: Pointer::Increment,
            ..Default::default()
        },
        Instruction {
            data: Word::raw(2)?,
            r: Source::Bus,
            estk: Estk::Compact,
            compact_code: 1,
            cstk: Cstk::Bus,
            csp: Pointer::Increment,
            load_q: true,
            flags: true,
            ..Default::default()
        },
        Instruction::halt(),
    ];
    let mut a = Machine::new(Image::program(&code), 0, 16, 512)?;
    let mut b = Machine::new(Image::program(&code), 0, 16, 512)?;
    b.enable_jit_eager()?;
    a.step()?;
    assert!(a.step().is_err());
    assert!(b.run_steps(3).is_err());
    assert_eq!(a.cpu, b.cpu);
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.fault, b.fault);
    assert_eq!(b.fault.unwrap().code, 1);
    assert_eq!(b.cpu.estk[0], 7);
    assert_eq!(b.cpu.estk[1], 0);
    assert_eq!(b.jit_statistics().unwrap().block_instructions, 1);
    Ok(())
}

#[test]
fn native_nam_tables_preserve_all_value_bits_before_a_later_map_fault() -> Result<()> {
    let mut image = Image::program(&[
        Instruction {
            fetch: Fetch::Nam,
            ..Instruction::default()
        },
        Instruction {
            fetch: Fetch::Map,
            ..Instruction::default()
        },
    ]);
    image.nam = vec![Some(u64::MAX)];
    let mut expected = Machine::new(image.clone(), 0, 16, 512)?;
    let mut actual = Machine::new(image, 0, 16, 512)?;
    actual.enable_jit_eager()?;
    assert!(expected.run_steps(2).is_err());
    assert!(actual.run_steps(2).is_err());
    assert_eq!(actual.cpu, expected.cpu);
    assert_eq!(actual.stats, expected.stats);
    assert_eq!(actual.fault, expected.fault);
    assert_eq!(actual.cpu.opcode, (u64::MAX >> 30) as usize);
    assert_eq!(actual.cpu.namarg, (1 << 30) - 1);
    Ok(())
}

#[test]
fn generated_objekt_register_controls_match_hdl_with_backpressure() -> Result<()> {
    use rekursiv_asm::{Command, Index, Read, Register};
    let commands = [
        Command {
            data: Word::signed(-19),
            load_vr: true,
            vr: 3,
            ..Command::default()
        },
        Command {
            data: Word::raw(73)?,
            load_vr: true,
            vr: 3,
            read: Read::Vr,
            ..Command::default()
        },
        Command {
            data: Word::index(-1)?,
            index: Index::Load,
            register: Register::FromIndex,
            ..Command::default()
        },
        Command {
            index: Index::Increment,
            register: Register::FromIndex,
            read: Read::Index,
            ..Command::default()
        },
        Command {
            index: Index::FromReg,
            register: Register::Increment,
            read: Read::IndexReg,
            ..Command::default()
        },
        Command {
            read: Read::Vr,
            vr: 3,
            ..Command::default()
        },
    ];
    let mut code: Vec<_> = commands
        .into_iter()
        .map(|c| Instruction {
            data: c.data,
            object: Some(c),
            ..Instruction::default()
        })
        .collect();
    code.push(Instruction::halt());
    let image = Image::program(&code);
    let mut native = Machine::new(image.clone(), 0, 16, 512)?;
    native.enable_jit_eager()?;
    native.run_steps(32)?;
    let rt = rekursiv_sim::runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 7,
            response_stall: 5,
        },
        None,
    )?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut oracle = Processor::default();
    h.run_processor(&image, &mut oracle, 2000)?;
    assert_eq!(native.cpu, oracle);
    assert_eq!(native.objekt, h.oracle);
    assert_eq!(
        native.jit_statistics().unwrap().block_instructions,
        commands.len() as u64
    );
    Ok(())
}

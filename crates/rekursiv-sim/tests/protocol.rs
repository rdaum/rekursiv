use eyre::Result;
use rekursiv_asm::*;
use rekursiv_sim::{runtime, Harness, Timing};

fn setup(h: &mut Harness<'_>) -> Result<()> {
    let reference = Word::reference(1, true)?;
    h.install(
        reference,
        Word::reference(8, true)?,
        16,
        &[Word::signed(7), Word::signed(8)],
    )?;
    assert_eq!(h.execute(Command::probe(reference))?.status, Status::Ok);
    assert_eq!(h.execute(Command::index(1)?)?.status, Status::Ok);
    Ok(())
}

#[test]
fn stalled_write_latches_inputs_and_commits_once() -> Result<()> {
    let runtime = runtime()?;
    for fail in [false, true] {
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        setup(&mut h)?;
        h.timing = Timing {
            request_delay: 5,
            memory_latency: 9,
            response_stall: 0,
        };
        let command = Command {
            index: Index::Increment,
            register: Register::Increment,
            memory: Memory::Write,
            load_vr: true,
            vr: 7,
            data: Word::signed(123),
            ..Command::default()
        };
        let ports = command.encode()?;
        let before = h.oracle.state.clone();
        let memory = h.backing.clone();
        let stats = h.stats;
        let transfers = h.transfers.len();
        h.drive(ports)?;
        h.rtl.run_i = 1;
        h.rtl.cmd_valid_i = 1;
        h.fail_next_memory = fail;
        assert!(h.tick()?.command);
        assert_eq!(h.snapshot()?, before);

        // Everything on the input bus may change after acceptance.
        h.drive(Ports {
            pager: 15,
            index: 15,
            data: WORD_MASK,
            ..Ports::default()
        })?;
        h.rtl.run_i = 0;
        h.rtl.svc_valid_i = 1;
        h.rtl.svc_op_i = 1;
        h.rtl.svc_ref_i = Word::reference(1, true)?.bits();
        for _ in 0..100 {
            if h.rtl.rsp_valid_o != 0 {
                break;
            }
            assert_eq!(h.rtl.cmd_ready_o, 0);
            assert_eq!(h.rtl.svc_ready_o, 0);
            assert_eq!(
                h.snapshot()?,
                before,
                "architectural state changed before memory completion"
            );
            assert_eq!(h.backing, memory);
            let edge = h.tick()?;
            assert!(!edge.command && !edge.service && !edge.response);
        }
        assert_eq!(h.rtl.rsp_valid_o, 1);
        let expected = h.oracle.execute(command, fail);
        assert_eq!(h.rtl.rsp_status_o, expected.response.status as u8);
        assert_eq!(h.rtl.rsp_data_o, expected.response.data.bits());
        assert_eq!(&h.transfers[transfers..], &[expected.memory[0]]);
        h.compare_state()?;
        assert_eq!(h.stats.writes, stats.writes + u64::from(!fail));
        assert_eq!(h.stats.errors, stats.errors + u64::from(fail));
        assert_eq!(h.stats.commands, stats.commands + 1);
        assert_eq!(h.stats.services, stats.services);
        assert_eq!(h.stats.responses, stats.responses);
        let committed = h.stats;
        for _ in 0..20 {
            assert_eq!(h.rtl.cmd_ready_o, 0);
            assert_eq!(h.rtl.svc_ready_o, 0);
            assert_eq!(h.rtl.mem_valid_o, 0);
            let edge = h.tick()?;
            assert!(!edge.command && !edge.service && !edge.response);
            h.compare_state()?;
        }
        assert_eq!(h.stats.writes, committed.writes);
        assert_eq!(h.stats.errors, committed.errors);
        assert_eq!(h.transfers.len(), transfers + 1);
        h.rtl.cmd_valid_i = 0;
        h.rtl.svc_valid_i = 0;
        h.rtl.rsp_ready_i = 1;
        assert!(h.tick()?.response);
        assert!(!h.tick()?.response);
        assert_eq!(h.stats.responses, stats.responses + 1);
    }
    Ok(())
}

#[test]
fn service_and_mutator_are_exclusive() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    h.drive(Command::index(77)?.encode()?)?;
    h.rtl.cmd_valid_i = 1;
    h.rtl.run_i = 0;
    for _ in 0..5 {
        assert!(!h.tick()?.command);
    }
    h.compare_state()?;
    h.rtl.cmd_valid_i = 0;
    h.rtl.svc_valid_i = 1;
    h.rtl.svc_op_i = 2;
    h.rtl.svc_code_i = 0;
    h.rtl.svc_class_i = Word::reference(8, true)?.bits();
    h.rtl.run_i = 1;
    for _ in 0..5 {
        assert!(!h.tick()?.service);
    }
    h.rtl.svc_valid_i = 0;
    assert_eq!(
        h.execute(Command::probe(Word::NIL))?.status,
        Status::BadValue
    );
    assert_eq!(h.stats.services, 0);
    Ok(())
}

#[test]
fn full_system_reset_at_each_transaction_phase() -> Result<()> {
    let runtime = runtime()?;
    for phase in 0..3 {
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        setup(&mut h)?;
        h.timing = Timing {
            request_delay: 5,
            memory_latency: 9,
            response_stall: 0,
        };
        h.drive(Command::write_field(Word::signed(999)).encode()?)?;
        h.rtl.run_i = 1;
        h.rtl.cmd_valid_i = 1;
        assert!(h.tick()?.command);
        h.rtl.cmd_valid_i = 0;
        if phase >= 1 {
            for _ in 0..100 {
                if h.rtl.mem_rsp_ready_o != 0 {
                    break;
                }
                h.tick()?;
            }
            assert_eq!(h.rtl.mem_rsp_ready_o, 1);
        }
        if phase == 2 {
            for _ in 0..100 {
                if h.rtl.rsp_valid_o != 0 {
                    break;
                }
                h.tick()?;
            }
            assert_eq!(h.rtl.rsp_valid_o, 1);
        }
        h.reset()?;
        for _ in 0..30 {
            assert_eq!(h.rtl.mem_valid_o, 0);
            assert_eq!(h.rtl.rsp_valid_o, 0);
            assert_eq!(h.rtl.mem_rsp_ready_o, 0);
            let edge = h.tick()?;
            assert!(!edge.command && !edge.service && !edge.response);
        }
        assert_eq!(h.stats.writes, 0);
        h.compare_state()?;
        setup(&mut h)?;
        assert_eq!(h.execute(Command::read_field())?.data, Word::signed(7));
    }
    Ok(())
}

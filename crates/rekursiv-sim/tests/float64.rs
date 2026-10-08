//! The DUT is synthesizable RTL. SoftFloat supplies expected bits and flags only.
use eyre::{ensure, Result};
use marlin::{verilator::VerilatedModelConfig, verilog::prelude::*};
use rekursiv_asm::processor::{FloatOp, Rounding};
use rekursiv_model::float::evaluate64;
use rekursiv_sim::runtime;

#[verilog(src = "../../rtl/numerik_fp64.sv", name = "numerik_fp64")]
pub struct Fp64;
fn tick(d: &mut Fp64<'_>) {
    d.clk_i = 0;
    d.eval();
    d.clk_i = 1;
    d.eval();
    d.clk_i = 0;
    d.eval();
}
fn reset(d: &mut Fp64<'_>) {
    d.rst_i = 1;
    d.valid_i = 0;
    d.ready_i = 0;
    tick(d);
    d.rst_i = 0;
    d.eval();
}
fn check(
    d: &mut Fp64<'_>,
    op: FloatOp,
    round: Rounding,
    a: u64,
    b: u64,
    stall: usize,
) -> Result<()> {
    ensure!(d.ready_o != 0, "DUT did not release request channel");
    d.operation_i = op as u8;
    d.rounding_i = round as u8;
    d.a_i = a;
    d.b_i = b;
    d.valid_i = 1;
    tick(d);
    d.valid_i = 0;
    // Poison every input after acceptance: the backend must have latched them.
    d.a_i = !a;
    d.b_i = !b;
    d.operation_i = 15;
    d.rounding_i = 7;
    let mut cycles = 0;
    while d.valid_o == 0 && cycles < 160 {
        tick(d);
        cycles += 1;
    }
    ensure!(d.valid_o != 0, "operation timed out: {op:?}");
    let expected = evaluate64(op, round, a, b);
    ensure!(
        (d.result_o, d.exceptions_o) == expected,
        "{op:?} {round:?} a={a:016x} b={b:016x}: RTL {:016x}/{:02x}, reference {:016x}/{:02x}",
        d.result_o,
        d.exceptions_o,
        expected.0,
        expected.1
    );
    for _ in 0..stall {
        d.valid_i = 1;
        tick(d);
        ensure!(
            d.ready_o == 0 && d.valid_o != 0 && (d.result_o, d.exceptions_o) == expected,
            "response changed or a second request was accepted under backpressure"
        );
    }
    d.valid_i = 0;
    d.ready_i = 1;
    tick(d);
    d.ready_i = 0;
    ensure!(d.valid_o == 0 && d.ready_o != 0, "response duplicated");
    Ok(())
}
#[test]
fn ieee_edges_all_rounding_modes_and_random_bits() -> Result<()> {
    let rt = runtime()?;
    let mut d = rt
        .create_model::<Fp64>(&VerilatedModelConfig::default())
        .map_err(|e| eyre::eyre!("{e:?}"))?;
    reset(&mut d);
    let operations = [
        FloatOp::Add,
        FloatOp::Subtract,
        FloatOp::Multiply,
        FloatOp::Divide,
        FloatOp::Sqrt,
        FloatOp::Compare,
        FloatOp::FromSigned,
        FloatOp::ToSigned,
        FloatOp::FromUnsigned,
        FloatOp::ToUnsigned,
    ];
    let modes = [
        Rounding::NearestEven,
        Rounding::TowardZero,
        Rounding::Down,
        Rounding::Up,
        Rounding::NearestAway,
    ];
    let values = [
        0,
        0x8000000000000000,
        1,
        0x8000000000000001,
        0x000fffffffffffff,
        0x0010000000000000,
        0x3ff0000000000000,
        0xbff0000000000000,
        0x3fe0000000000000,
        0x3ff8000000000000,
        0x41dfffffffc00000,
        0x41e0000000000000,
        0x41f0000000000000,
        0x7fefffffffffffff,
        0xffefffffffffffff,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000000,
        0x7ff0000000000001,
        0xfff8123456789abc,
        0x3ca0000000000000,
        0xffffffffffffffff,
    ];
    for op in operations {
        for mode in modes {
            for &a in &values {
                for &b in &values {
                    check(&mut d, op, mode, a, b, 2)?;
                }
            }
        }
    }
    let mut bits = 0x137a_bc92_38ff_9121u64;
    for n in 0..10_000 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let a = bits;
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        check(
            &mut d,
            operations[n % 10],
            modes[(n / 10) % 5],
            a,
            bits,
            n % 4,
        )?;
    }
    Ok(())
}
#[test]
fn reset_cancels_division_and_held_completion() -> Result<()> {
    let rt = runtime()?;
    let mut d = rt
        .create_model::<Fp64>(&VerilatedModelConfig::default())
        .map_err(|e| eyre::eyre!("{e:?}"))?;
    for delay in [0, 1, 5, 100] {
        reset(&mut d);
        d.operation_i = 3;
        d.rounding_i = 0;
        d.a_i = 0x3ff0000000000000;
        d.b_i = 0x4008000000000000;
        d.valid_i = 1;
        tick(&mut d);
        d.valid_i = 0;
        for _ in 0..delay {
            tick(&mut d);
        }
        reset(&mut d);
        for _ in 0..100 {
            tick(&mut d);
            ensure!(d.valid_o == 0, "pre-reset result escaped");
        }
        check(
            &mut d,
            FloatOp::Multiply,
            Rounding::NearestEven,
            0x4000000000000000,
            0x4008000000000000,
            3,
        )?;
    }
    Ok(())
}

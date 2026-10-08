//! Guest Float layout and primitive policy, exercised in both execution engines.
use super::*;

fn float(f: &mut Fixture, value: f64) -> u32 {
    let bits = value.to_bits();
    f.object(
        f.special[9],
        Body::Words(vec![(bits >> 32) as u32, bits as u32]),
        6,
    )
}
fn fixture(op: u32, a: f64, b: Option<f64>, scale: Option<i32>) -> Fixture {
    let mut f = Fixture::new();
    let selector = f.selector("floatTest");
    let arity = u32::from(b.is_some() || scale.is_some());
    let method = f.method(&[122], vec![], arity, arity, op); // failure -> false
    f.bind(f.special[9], selector, method);
    f.bind(f.special[5], selector, method);
    let receiver = if op == 40 {
        int(a as i32)
    } else {
        float(&mut f, a)
    };
    if arity == 1 {
        let arg = if let Some(scale) = scale {
            int(scale)
        } else {
            float(&mut f, b.unwrap())
        };
        f.start(&[32, 33, 226, 124], vec![receiver, arg, selector], vec![]);
    } else {
        f.start(&[32, 209, 124], vec![receiver, selector], vec![]);
    }
    f
}
fn expect_float(f: &Fixture, expected: f64) -> Result<()> {
    for jit in [false, true] {
        let m = run(f, jit)?;
        let words = body(&m, m.objekt.state.vr[5])?;
        ensure!(
            words.len() == 4 && words[0].bits() == 33,
            "not a Float: {words:?}"
        );
        let bits = (words[2].bits() << 32) | words[3].bits();
        assert_eq!(bits, expected.to_bits(), "JIT={jit}");
    }
    Ok(())
}
#[test]
fn binary64_arithmetic_keeps_bits_beyond_binary32() -> Result<()> {
    let a = 1.0 + 2f64.powi(-40);
    for (op, b, expected) in [
        (41, 2.0, a + 2.0),
        (42, 1.0, a - 1.0),
        (49, 3.0, a * 3.0),
        (50, 3.0, a / 3.0),
    ] {
        expect_float(&fixture(op, a, Some(b), None), expected)?;
    }
    expect_float(&fixture(40, -1073741824.0, None, None), -1073741824.0)?;
    expect_float(&fixture(41, f64::MAX, Some(f64::MAX), None), f64::INFINITY)?;
    for zero in [0.0, -0.0] {
        let f = fixture(50, 1.0, Some(zero), None);
        result(&f, target::reference(f.special[1])?)?;
    }
    for op in 43..=48 {
        let f = fixture(op, f64::NAN, Some(1.0), None);
        result(
            &f,
            target::reference(f.special[if op == 48 { 2 } else { 1 }])?,
        )?;
    }
    Ok(())
}
#[test]
fn fractions_scaling_exponents_and_integer_boundaries() -> Result<()> {
    for a in [
        -0.0,
        0.0,
        -3.0,
        0.125,
        -0.125,
        123.625,
        -123.625,
        2f64.powi(40) + 0.25,
        f64::MAX,
        f64::INFINITY,
    ] {
        let expected = if a.is_infinite() {
            0.0
        } else {
            a.fract().copysign(a)
        };
        expect_float(&fixture(52, a, None, None), expected)?;
    }
    for (a, scale, expected) in [
        (1.0, -1074, f64::from_bits(1)),
        (1.0, -1075, 0.0),
        (1.5, -1075, f64::from_bits(1)),
        (f64::from_bits(1), 1074, 1.0),
        (f64::from_bits(3), -1, f64::from_bits(2)),
        (-0.0, 10, -0.0),
        (-1.0, 2000, f64::NEG_INFINITY),
        (-1.0, -2000, -0.0),
        (1.0 + 2f64.powi(-40), 20, 2f64.powi(20) + 2f64.powi(-20)),
    ] {
        expect_float(&fixture(54, a, None, Some(scale)), expected)?;
    }
    for (a, e) in [
        (0.0, 0),
        (0.5, 0),
        (0.25, -2),
        (1.0, 0),
        (8.0, 3),
        (f64::from_bits(1), -1074),
    ] {
        result(&fixture(53, a, None, None), Word::signed(e))?;
    }
    for (a, n) in [
        (0.99, 0),
        (-1.99, -1),
        (1073741823.5, 1073741823),
        (-1073741824.5, -1073741824),
    ] {
        result(&fixture(51, a, None, None), Word::signed(n))?;
    }
    for a in [1073741824.0, -1073741825.0, f64::INFINITY, f64::NAN] {
        let f = fixture(51, a, None, None);
        result(&f, target::reference(f.special[1])?)?;
    }
    Ok(())
}

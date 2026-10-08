//! SoftFloat numeric semantics for the RTL oracle and native emulator.
//! RTL execution uses HardFloat and never receives these computed results.
use rekursiv_asm::processor::{FloatOp, Rounding};
use std::sync::Mutex;
static REFERENCE: Mutex<()> = Mutex::new(());
extern "C" {
    fn rekursiv_reference_float(op: u8, rounding: u8, a: u32, b: u32, flags: *mut u8) -> u32;
}
pub fn evaluate(operation: FloatOp, rounding: Rounding, a: u32, b: u32) -> (u32, u8) {
    let _lock = REFERENCE.lock().expect("floating-point reference lock");
    let mut flags = 0;
    // SAFETY: flags points to a writable byte for this call; the lock protects
    // SoftFloat's global state. The C routine retains no pointers.
    let result =
        unsafe { rekursiv_reference_float(operation as u8, rounding as u8, a, b, &mut flags) };
    (result, flags)
}

extern "C" {
    fn rekursiv_reference_float64(op: u8, rounding: u8, a: u64, b: u64, flags: *mut u8) -> u64;
}
/// Binary64 numeric semantics. Integers remain signed/unsigned 32-bit values.
/// Arithmetic returns all 64 IEEE bits; comparisons and integer results occupy
/// the low 32 bits. This models NUMERIK, not any language's Float primitive.
pub fn evaluate64(operation: FloatOp, rounding: Rounding, a: u64, b: u64) -> (u64, u8) {
    let _lock = REFERENCE.lock().expect("floating-point reference lock");
    let mut flags = 0;
    // SAFETY: the writable byte lives through the call; the shared lock protects
    // all SoftFloat state, including calls through the binary32 entry point.
    let result =
        unsafe { rekursiv_reference_float64(operation as u8, rounding as u8, a, b, &mut flags) };
    (result, flags)
}

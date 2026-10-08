// Test-only entry point. The Rust caller serializes SoftFloat's global state.
#include "softfloat.h"
uint32_t rekursiv_reference_float(uint8_t op, uint8_t rounding,
                                 uint32_t a, uint32_t b, uint8_t *flags) {
    float32_t x = { a }, y = { b }, f;
    uint32_t result;
    softfloat_roundingMode = rounding;
    softfloat_detectTininess = softfloat_tininess_afterRounding;
    softfloat_exceptionFlags = 0;
    switch(op) {
        case 0: f = f32_add(x, y); result = f.v; break;
        case 1: f = f32_sub(x, y); result = f.v; break;
        case 2: f = f32_mul(x, y); result = f.v; break;
        case 3: f = f32_div(x, y); result = f.v; break;
        case 4: f = f32_sqrt(x); result = f.v; break;
        case 5: {
            bool lt = f32_lt_quiet(x, y), eq = f32_eq(x, y), gt = f32_lt_quiet(y, x);
            result = lt | (eq << 1) | (gt << 2) | ((!lt && !eq && !gt) << 3);
            break;
        }
        case 6: f = i32_to_f32((int32_t)a); result = f.v; break;
        case 7: result = f32_to_i32(x, rounding, true); break;
        case 8: f = ui32_to_f32(a); result = f.v; break;
        case 9: result = f32_to_ui32(x, rounding, true); break;
        default: result = 0x7fc00000; softfloat_exceptionFlags = softfloat_flag_invalid;
    }
    // Adopt NUMERIK's deterministic saturated integer result on invalid
    // conversion, including maximum integer for NaN. Exception flags remain
    // SoftFloat's independent assessment of validity and rounding.
    if ((op == 7 || op == 9) && (softfloat_exceptionFlags & softfloat_flag_invalid)) {
        bool nan = (a & 0x7fffffff) > 0x7f800000;
        bool maximum = nan || !(a >> 31);
        result = op == 7 ? (maximum ? 0x7fffffff : 0x80000000) : (maximum ? 0xffffffff : 0);
    }
    *flags = softfloat_exceptionFlags;
    return result;
}

// Binary64 has the same operation/rounding contract; integer conversions still
// consume/produce 32 bits. No C floating-point arithmetic participates.
uint64_t rekursiv_reference_float64(uint8_t op, uint8_t rounding,
                                    uint64_t a, uint64_t b, uint8_t *flags) {
    float64_t x = { a }, y = { b }, f;
    uint64_t result;
    softfloat_roundingMode = rounding;
    softfloat_detectTininess = softfloat_tininess_afterRounding;
    softfloat_exceptionFlags = 0;
    switch(op) {
        case 0: f = f64_add(x, y); result = f.v; break;
        case 1: f = f64_sub(x, y); result = f.v; break;
        case 2: f = f64_mul(x, y); result = f.v; break;
        case 3: f = f64_div(x, y); result = f.v; break;
        case 4: f = f64_sqrt(x); result = f.v; break;
        case 5: {
            bool lt = f64_lt_quiet(x, y), eq = f64_eq(x, y), gt = f64_lt_quiet(y, x);
            result = lt | (eq << 1) | (gt << 2) | ((!lt && !eq && !gt) << 3);
            break;
        }
        case 6: f = i32_to_f64((int32_t)a); result = f.v; break;
        case 7: result = (uint32_t)f64_to_i32(x, rounding, true); break;
        case 8: f = ui32_to_f64((uint32_t)a); result = f.v; break;
        case 9: result = (uint32_t)f64_to_ui32(x, rounding, true); break;
        default: result = UINT64_C(0x7ff8000000000000); softfloat_exceptionFlags = softfloat_flag_invalid;
    }
    if ((op == 7 || op == 9) && (softfloat_exceptionFlags & softfloat_flag_invalid)) {
        bool nan = (a & UINT64_C(0x7fffffffffffffff)) > UINT64_C(0x7ff0000000000000);
        bool maximum = nan || !(a >> 63);
        result = op == 7 ? (maximum ? 0x7fffffff : 0x80000000) : (maximum ? 0xffffffff : 0);
    }
    *flags = softfloat_exceptionFlags;
    return result;
}

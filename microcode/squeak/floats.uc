; Squeak binary64 primitives 40..54. No host floating-point primitives.
; Guest words are high32 then low32; NUMERIK pairs are low32 then high32.
; R2:R3 receiver, R4:R5 argument/result, R6 helper continuation; R8/R9 IP/SP.
; The generic FPU writes PRODUCT. Copy its halves before allocation_hash uses
; multiplication. GC preserves all numeric registers and PRODUCT on a miss.
; Arithmetic accepts IEEE specials; division by either signed zero fails.
; Integer conversion fails outside signed31. This avoids the archived VM's
; undefined C cast overflow. Exponent follows its frexp special case at -1.
primitive_float:
    ra=0, s=Branch, brch=40, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_from_integer
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=float_unary_arity
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=float_receiver
float_unary_arity:
    ra=0, s=Branch, brch=54, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_scale_arity
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=float_receiver
float_scale_arity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
float_receiver:
    read=Vr, vr=6
    d=Object, ldsym
    d=float_receiver_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=float_load
float_receiver_ready:
    ra=4, rb=2, ldrb
    ra=5, rb=3, ldrb
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=float_unary
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=float_argument_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=float_load
; Both finite operands are decoded. Arithmetic returns through float_result;
; comparisons return guest Booleans and allocate no Float result.
float_argument_ready:
    ra=0, s=Branch, brch=41, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_add
    ra=0, s=Branch, brch=42, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_subtract
    ra=0, s=Branch, brch=49, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_multiply
    ra=0, s=Branch, brch=50, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_divide
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Compare, ldq
    d=Q, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=43, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_less
    ra=0, s=Branch, brch=44, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_greater
    ra=0, s=Branch, brch=45, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_less_equal
    ra=0, s=Branch, brch=46, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_greater_equal
    ra=4, s=Branch, brch=2, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=float_unequal
    ra=0, s=Branch, brch=47, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_true
    seq=Jump, brch=primitive_false
float_unequal:
    ra=0, s=Branch, brch=48, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_true
    seq=Jump, brch=primitive_false
float_less:
    ra=4, s=Bus, d=1, alu=And, flags, seq=Jump, brch=float_boolean
float_greater:
    ra=4, s=Bus, d=4, alu=And, flags, seq=Jump, brch=float_boolean
float_less_equal:
    ra=4, s=Bus, d=3, alu=And, flags, seq=Jump, brch=float_boolean
float_greater_equal:
    ra=4, s=Branch, brch=6, alu=And, flags
float_boolean:
    seq=ConditionalJump, cc=!Zero, brch=primitive_true
    seq=Jump, brch=primitive_false
float_add:
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Add, ldq, seq=Jump, brch=float_result
float_subtract:
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Subtract, ldq, seq=Jump, brch=float_result
float_multiply:
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Multiply, ldq, seq=Jump, brch=float_result
float_divide:
    ra=5, s=Bus, d=0x7fffffff, alu=And, rb=6, ldrb
    ra=4, rb=6, alu=Or, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Divide, ldq, seq=Jump, brch=float_result
float_from_integer:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=2, rb=4, alu=Float, precision=Binary64, fp=FromSigned, seq=Jump, brch=float_result
float_unary:
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=float_unary_other
    ra=2, rb=4, alu=Float, precision=Binary64, fp=ToSigned, round=TowardZero, ldq
    alu=FloatStatus, rb=4, ldrb
    ra=4, s=Branch, brch=16, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=integer_result
float_unary_other:
    ra=3, s=Branch, brch=12, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=2047, alu=And, rb=4, ldrb
    ra=0, s=Branch, brch=52, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_fraction
    ra=0, s=Branch, brch=54, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_scale_argument
    ; frexp has no useful exponent for specials: return deterministic zero.
    ra=4, s=Branch, brch=2047, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_exponent_zero
    d=float_exponent_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=float_normalize
float_exponent_ready:
    ra=4, s=Branch, brch=1, alu=Add, flags
    seq=ConditionalJump, cc=Zero, brch=float_exponent_zero
    ra=3, s=Bus, d=0x7fffffff, alu=And, rb=5, ldrb
    ra=2, rb=5, alu=Or, flags
    seq=ConditionalJump, cc=Zero, brch=float_exponent_zero
    ra=4, ldq, seq=Jump, brch=integer_result
float_exponent_zero:
    d=0, r=Bus, ldq, seq=Jump, brch=integer_result

; Remove fractional mantissa bits to build the integral double, then subtract.
; All finite magnitudes >=2^52 have signed zero fractional part. Values below
; one return unchanged. NaNs are quieted by the arithmetic unit.
float_fraction:
    ra=4, s=Branch, brch=2047, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_fraction_special
    ra=4, s=Branch, brch=1023, alu=Sub, cin=One, rb=6, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=float_allocate
    ra=6, s=Branch, brch=52, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=float_signed_zero
    ra=6, s=Branch, brch=52, alu=SubReverse, cin=One, rb=6, ldrb
    ra=2, rb=4, ldrb
    ra=3, rb=5, ldrb
    d=0xffffffff, r=Bus, rb=7, ldrb
    ra=6, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_fraction_low_mask
    d=0, r=Bus, rb=4, ldrb
    ra=6, s=Branch, brch=32, alu=Sub, cin=One, rb=6, ldrb
float_fraction_high_mask:
    ra=6, flags
    seq=ConditionalJump, cc=Zero, brch=float_fraction_high_ready
    ra=7, shift=Left, rb=7, ldrb
    ra=6, s=Branch, brch=1, alu=Sub, cin=One, rb=6, ldrb
    seq=Jump, brch=float_fraction_high_mask
float_fraction_high_ready:
    ra=7, rb=5, alu=And, ldrb, seq=Jump, brch=float_fraction_subtract
float_fraction_low_mask:
    ra=7, shift=Left, rb=7, ldrb
    ra=6, s=Branch, brch=1, alu=Sub, cin=One, rb=6, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=float_fraction_low_mask
    ra=7, rb=4, alu=And, ldrb
float_fraction_subtract:
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Subtract
    ; Exact integer subtraction returns +0; modf requires the receiver sign.
    alu=ProductLow, rb=4, ldrb
    alu=ProductHigh, rb=5, ldrb
    ra=4, rb=5, alu=Or, flags
    seq=ConditionalJump, cc=Zero, brch=float_signed_zero
    seq=Jump, brch=float_result
float_fraction_special:
    ra=3, s=Bus, d=0xfffff, alu=And, rb=5, ldrb
    ra=2, rb=5, alu=Or, flags
    seq=ConditionalJump, cc=Zero, brch=float_signed_zero
    d=0, r=Bus, rb=4, ldrb
    d=0, r=Bus, rb=5, ldrb, seq=Jump, brch=float_add
float_signed_zero:
    d=0, r=Bus, rb=2, ldrb
    ra=3, s=Bus, d=0x80000000, alu=And, rb=3, ldrb
    seq=Jump, brch=float_allocate

float_scale_argument:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=10, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, s=Branch, brch=2047, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_scale_special
    d=float_scale_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=float_normalize
float_scale_ready:
    ra=3, s=Bus, d=0x7fffffff, alu=And, rb=5, ldrb
    ra=2, rb=5, alu=Or, flags
    seq=ConditionalJump, cc=Zero, brch=float_allocate
    ra=4, rb=10, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
    ra=3, s=Bus, d=0x800fffff, alu=And, rb=3, ldrb
    ra=4, s=Branch, brch=1023, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_scale_infinity
    ra=4, s=Bus, d=0xfffffc02, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_scale_subnormal
    ra=4, s=Branch, brch=1023, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=20, alu=Rotate, rb=4, ldrb
    ra=4, rb=3, alu=Or, ldrb, seq=Jump, brch=float_allocate
float_scale_subnormal:
    ra=4, s=Bus, d=0xfffffbcd, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_signed_zero
    ; E >= -1075. Scale a normal significand with exponent -1022 by
    ; 2^(E+1022); exactly one operation rounds the subnormal result.
    ra=3, s=Bus, d=0x00100000, alu=Or, rb=3, ldrb
    ra=4, s=Branch, brch=2045, alu=Add, rb=5, ldrb
    ra=5, s=Branch, brch=20, alu=Rotate, rb=5, ldrb
    d=0, r=Bus, rb=4, ldrb, seq=Jump, brch=float_multiply
float_scale_infinity:
    ra=3, s=Bus, d=0x80000000, alu=And, rb=3, ldrb
    ra=3, s=Bus, d=0x7ff00000, alu=Or, rb=3, ldrb
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=float_allocate
float_scale_special:
    d=0, r=Bus, rb=4, ldrb
    d=0x3ff00000, r=Bus, rb=5, ldrb, seq=Jump, brch=float_multiply

; Normalize a finite subnormal using an exact multiplication by 2^54.
; Return adjusted unbiased exponent R4 and normalized bits R2:R3 via R6.
float_normalize:
    ra=4, flags
    seq=ConditionalJump, cc=!Zero, brch=float_normal_exponent
    d=0, r=Bus, rb=4, ldrb
    d=0x43500000, r=Bus, rb=5, ldrb
    ra=2, rb=4, alu=Float, precision=Binary64, fp=Multiply
    alu=ProductLow, rb=2, ldrb
    alu=ProductHigh, rb=3, ldrb
    ra=3, s=Branch, brch=12, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=2047, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=54, alu=Sub, cin=One, rb=4, ldrb
float_normal_exponent:
    ra=4, s=Branch, brch=1023, alu=Sub, cin=One, rb=4, ldrb
    d=Register, ra=6, seq=Bus

; Validate a rooted Float in SYMBOL; return low/high words R4:R5 via R6.
float_load:
    d=Symbol, page=Fetch
    read=Type
    d=SPECIAL_9, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=33, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    idx=Increment
    mem=Read
    d=Object, r=Bus, rb=4, ldrb
    d=Register, ra=6, seq=Bus
float_result:
    alu=ProductLow, rb=2, ldrb
    alu=ProductHigh, rb=3, ldrb
float_allocate:
    d=SPECIAL_9, page=Allocate, size=4, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=33, mem=Write
    d=2, idx=Load
    d=0x6000, mem=Write
    d=float_hash_ready, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
float_hash_ready:
    d=3, idx=Load
    d=Register, ra=3, mem=Write
    idx=Increment
    d=Register, ra=2, mem=Write, seq=Jump, brch=send_result

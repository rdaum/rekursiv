; SmallInteger primitive dispatch and extended integer algorithms.
; Primitive numbers: 1 +, 2 -, 3 <, 4 >, 5 <=, 6 >=, 7 =, 8 ~=, 9 *,
; 10 exact /, 11 floor remainder, 12 floor //, 13 truncating quotient,
; 14 bitAnd:, 15 bitOr:, 16 bitXor:, 17 bitShift:, 18 make Point (@).
; All take one argument. Except @, operands must be compact signed integers;
; numeric results must fit -16384..16383 or the guest method handles fallback.
;
; 1..9 and 14..15 select the arithmetic kernels in interpreter.uc through
; R13's special-bytecode codes. R13=0 selects integer_extended after the same
; tag checks. That entry receives decoded receiver R4 and argument R5.
; Q carries numeric results to integer_result, which range-checks, tags, and
; returns through send_result when R15 is a primitive failure continuation.
;
; Division uses magnitudes for a fixed 15 steps, then restores signs. Exact /
; fails on a remainder. Floor quotient decrements a negative truncated quotient
; when division was inexact; modulo has the divisor's sign. Division by zero
; fails before mutation. Right shifts sign-extend and saturate the count at 31;
; left shifts of nonzero operands reject counts above 14 and check final range.
; Scratch R2..R7/R10/R13 is numeric only; VR0/VR6/VR7 keep caller and fallback
; roots. R8/R9 and R1 survive. Point construction additionally roots its second
; coordinate in VR3 before allocation and accepts noninteger coordinates too.
;
; SmallInteger primitives 1..18. Arithmetic stays in NUMERIK microcode. R0 is
; the primitive number, R1 the send arity, R15 the failure continuation. The
; shared arithmetic path validates full compact tags and signed 15-bit results.
primitive_integer:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=18, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_point
    ra=0, s=Branch, brch=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_other
    ra=0, s=Branch, brch=175, alu=Add, rb=13, ldrb
    seq=Jump, brch=arithmetic
integer_other:
    ra=0, s=Branch, brch=14, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=integer_and
    ra=0, s=Branch, brch=15, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=integer_or
    d=0, r=Bus, rb=13, ldrb, seq=Jump, brch=arithmetic
integer_and:
    d=190, r=Bus, rb=13, ldrb, seq=Jump, brch=arithmetic
integer_or:
    d=191, r=Bus, rb=13, ldrb, seq=Jump, brch=arithmetic
integer_extended:
    ra=0, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=integer_xor
    ra=0, s=Branch, brch=17, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=integer_shift

; Fifteen restoring-division steps, independent of quotient magnitude. R2
; retains the quotient sign; R3 retains the original dividend. R6 supplies
; dividend bits, R7 is the positive divisor, R4 quotient, R5 remainder.
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=4, rb=5, alu=Xor, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=4, rb=3, ldrb
    ra=4, rb=6, ldrb, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_divisor_abs
    ra=6, s=Branch, brch=0, alu=SubReverse, cin=One, rb=6, ldrb
integer_divisor_abs:
    ra=5, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_divide_start
    ra=7, s=Branch, brch=0, alu=SubReverse, cin=One, rb=7, ldrb
integer_divide_start:
    d=0, r=Bus, rb=4, ldrb
    d=0, r=Bus, rb=5, ldrb
    d=15, r=Bus, rb=10, ldrb
; Shift one dividend bit into the remainder; subtract divisor when it fits
; and set the new quotient bit. The 15-bit guest magnitude bounds the loop.
integer_divide_bit:
    ra=4, shift=Left, rb=4, ldrb
    ra=5, shift=Left, rb=5, ldrb
    ra=6, s=Branch, brch=16384, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=integer_divide_trial
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
integer_divide_trial:
    ra=6, shift=Left, rb=6, ldrb
    ra=5, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=integer_divide_next
    ra=5, rb=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=5, ldrb
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
integer_divide_next:
    ra=10, s=Branch, brch=1, alu=Sub, cin=One, rb=10, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=integer_divide_bit
    ra=0, s=Branch, brch=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=integer_modulo
    ra=0, s=Branch, brch=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=integer_quotient_sign
    ra=5, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
integer_quotient_sign:
    ra=2, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_quotient_result
    ra=4, s=Branch, brch=0, alu=SubReverse, cin=One, rb=4, ldrb
    ra=0, s=Branch, brch=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=integer_quotient_result
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=integer_quotient_result
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
integer_quotient_result:
    ra=4, ldq, seq=Jump, brch=integer_result
; For opposite signs and nonzero remainder use |divisor|-remainder, then
; apply divisor sign (R2 XOR original dividend R3 recovers that sign).
integer_modulo:
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=integer_remainder_result
    ra=2, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_remainder_sign
    ra=7, rb=5, alu=Sub, cin=One, ldrb
integer_remainder_sign:
    ra=2, rb=3, alu=Xor, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_remainder_result
    ra=5, s=Branch, brch=0, alu=SubReverse, cin=One, rb=5, ldrb
integer_remainder_result:
    ra=5, ldq, seq=Jump, brch=integer_result
integer_xor:
    ra=4, rb=5, alu=Xor, ldq, seq=Jump, brch=integer_result
integer_shift:
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=integer_quotient_result
    ra=5, flags
    seq=ConditionalJump, cc=Sign, brch=integer_shift_right
    ra=5, s=Branch, brch=14, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
integer_shift_left_loop:
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=integer_quotient_result
    ra=4, shift=Left, rb=4, ldrb
    ra=5, s=Branch, brch=1, alu=Sub, cin=One, rb=5, ldrb
    seq=Jump, brch=integer_shift_left_loop
integer_shift_right:
    ra=5, s=Branch, brch=0, alu=SubReverse, cin=One, rb=5, ldrb
    ra=5, s=Branch, brch=31, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=integer_shift_right_loop
    d=31, r=Bus, rb=5, ldrb
integer_shift_right_loop:
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=integer_quotient_result
    ra=4, shift=ArithmeticRight, rb=4, ldrb
    ra=5, s=Branch, brch=1, alu=Sub, cin=One, rb=5, ldrb
    seq=Jump, brch=integer_shift_right_loop

; Point construction accepts object values, including non-SmallInteger numbers.
; Preserve both coordinates across allocation before writing their tagged fields.
primitive_point:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    d=0xa00000000d, page=Allocate, size=3, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=16, mem=Write
    idx=Increment
    read=Vr, vr=6
    d=Object, mem=Write
    idx=Increment
    read=Vr, vr=3
    d=Object, mem=Write
    seq=Jump, brch=send_result

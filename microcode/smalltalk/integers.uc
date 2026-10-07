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

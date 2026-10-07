; Smalltalk Float wrappers for generic NUMERIK binary32 operations. Object
; layout and primitive failure stay here. The FPU receives only raw bits.
; VR6 roots the receiver, VR7 the fallback method, VR5 the allocated result.
; R2/R3 hold operand bits, R4/R5 loader scratch, R6 its continuation. R0 is
; the primitive number. R8/R9 retain caller IP/SP; send_result restores caches.
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
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=float_unary
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=float_argument_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=float_load
float_argument_ready:
    ra=4, rb=3, ldrb
    ra=0, s=Branch, brch=41, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_add
    ra=0, s=Branch, brch=42, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_subtract
    ra=0, s=Branch, brch=49, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_multiply
    ra=0, s=Branch, brch=50, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_divide
    ra=2, rb=3, alu=Float, fp=Compare, ldq
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
    ra=2, rb=3, alu=Float, fp=Add, ldq, seq=Jump, brch=float_result
float_subtract:
    ra=2, rb=3, alu=Float, fp=Subtract, ldq, seq=Jump, brch=float_result
float_multiply:
    ra=2, rb=3, alu=Float, fp=Multiply, ldq, seq=Jump, brch=float_result
float_divide:
    ra=2, rb=3, alu=Float, fp=Divide, ldq, seq=Jump, brch=float_result
float_from_integer:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=2, alu=Float, fp=FromSigned, ldq, seq=Jump, brch=float_result
float_unary:
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=float_unary_other
    ra=2, alu=Float, fp=ToSigned, round=TowardZero, ldq
    alu=FloatStatus, rb=4, ldrb
    ra=4, s=Branch, brch=16, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=integer_result
float_unary_other:
    ra=0, s=Branch, brch=52, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=float_fraction
    ; Normalize finite bits to sign R10, unbiased exponent R4 and 24-bit
    ; significand R5. Subnormal inputs need a leading-bit scan.
    ra=2, s=Bus, d=0x80000000, alu=And, rb=10, ldrb
    ra=2, s=Bus, d=0x7fffff, alu=And, rb=5, ldrb
    ra=2, s=Branch, brch=9, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=255, alu=And, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=float_subnormal_parts
    ra=4, s=Branch, brch=127, alu=Sub, cin=One, rb=4, ldrb
    ra=5, s=Bus, d=0x800000, alu=Or, rb=5, ldrb
    seq=Jump, brch=float_parts_ready
float_subnormal_parts:
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=float_zero_parts
    d=0xffffff82, r=Bus, rb=4, ldrb
float_normalize_parts:
    ra=5, s=Bus, d=0x800000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=float_parts_ready
    ra=5, shift=Left, rb=5, ldrb
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
    seq=Jump, brch=float_normalize_parts
float_zero_parts:
    ra=0, s=Branch, brch=53, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=float_scale_argument
    d=0xffffffff, r=Bus, ldq, seq=Jump, brch=integer_result
float_parts_ready:
    ra=0, s=Branch, brch=53, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=float_scale_argument
    ra=4, ldq, seq=Jump, brch=integer_result
float_fraction:
    ; At magnitude >= 2^23 a finite binary32 value has no fractional bits.
    ra=2, s=Bus, d=0x7fffffff, alu=And, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=float_fraction_zero
    ra=4, s=Bus, d=0x4b000000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=float_fraction_zero
    ra=2, alu=Float, fp=ToSigned, round=TowardZero, ldq
    r=Q, alu=Float, fp=FromSigned, ldq
    ra=2, s=Q, alu=Float, fp=Subtract, ldq, seq=Jump, brch=float_result
float_fraction_zero:
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=float_allocate
float_scale_argument:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=float_allocate
    ; SmallInteger scale cannot overflow this signed exponent calculation.
    ra=4, rb=3, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=127, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=4, s=Bus, d=0xffffff82, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_scale_subnormal
    ra=4, s=Branch, brch=127, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=23, alu=Rotate, rb=4, ldrb
    ra=5, s=Bus, d=0x7fffff, alu=And, rb=5, ldrb
    ra=4, rb=5, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=2, rb=10, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb, seq=Jump, brch=float_allocate
float_scale_subnormal:
    ra=4, s=Bus, d=0xffffff6a, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=float_scale_zero
    ; Multiply a smallest-normal-exponent significand by 2^(E+126).
    ; Only this final FPU operation rounds, including the tie at 2^-150.
    ra=5, rb=10, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=4, s=Branch, brch=253, alu=Add, rb=3, ldrb
    ra=3, s=Branch, brch=23, alu=Rotate, rb=3, ldrb
    seq=Jump, brch=float_multiply
float_scale_zero:
    ra=10, rb=2, ldrb, seq=Jump, brch=float_allocate

; SYMBOL is a rooted Float reference. Return its IEEE bits in R4 via R6.
; Byte/word indexing uses the converted descriptor, not any FPU convention.
float_load:
    d=Symbol, page=Fetch
    read=Type
    d=0xa00000000a, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=17, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=16, alu=Rotate, rb=4, ldrb
    idx=Increment
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ra=4, rb=5, alu=Or, ldq
    d=Q, r=Bus, rb=4, ldrb
    ; The guest primitive domain is finite Float values. Hardware still supports
    ; all IEEE special values for other runtimes and direct numeric programs.
    ra=4, s=Bus, d=0x7f800000, alu=And, rb=5, ldrb
    ra=5, s=Bus, d=0x7f800000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    d=Register, ra=6, seq=Bus
float_result:
    d=Q, r=Bus, rb=2, ldrb
    alu=FloatStatus, rb=4, ldrb
    ra=4, s=Branch, brch=28, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
float_allocate:
    d=0xa00000000a, page=Allocate, size=3, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=17, mem=Write
    ra=2, s=Branch, brch=16, alu=Rotate, rb=4, ldrb
    ra=4, s=Bus, d=65535, alu=And, ldq
    d=2, idx=Load
    d=Q, mem=Write
    ra=2, s=Bus, d=65535, alu=And, ldq
    idx=Increment
    d=Q, mem=Write, seq=Jump, brch=send_result

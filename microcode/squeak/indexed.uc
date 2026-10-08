 ; Squeak indexed access. R0 primitive, R1 arity, R2 kind, R3 logical length,
; R10 read/write/size, R11 translated component, VR3 write value, VR6 receiver.
; Guest pointer/word length is descriptor bytes / 4. Class format payload
; bits 1..6 hold fixed fields + 1. Byte/method lengths are byte counts.
; CompiledMethod objectAt: accesses header/literals; at: accesses bytecodes.
; Header mutation is rejected because it would change tagged/opaque boundaries.
; All bounds/type checks precede writes; failure leaves send operands intact.
primitive_indexed:
    d=0, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=62, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_size_arity
    ra=0, s=Branch, brch=61, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=indexed_metadata
indexed_write_arity:
    d=1, r=Bus, rb=10, ldrb
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=indexed_metadata
indexed_size_arity:
    d=2, r=Bus, rb=10, ldrb
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=indexed_integer_size
indexed_metadata:
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb
    d=Object, r=Bus, shift=Right, rb=3, ldrb
    ra=3, shift=Right, rb=3, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=indexed_length
    ra=3, shift=Right, rb=3, ldrb
    ra=3, shift=Right, rb=3, ldrb
indexed_length:
    ra=0, s=Branch, brch=68, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_method_fields
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_method_fields
    ra=0, s=Branch, brch=73, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_fixed_zero
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_fixed_zero
    read=Type
    d=Object, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, r=Bus, shift=Right, rb=7, ldrb
    ra=7, s=Branch, brch=63, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb
    seq=Jump, brch=indexed_length_ready
indexed_method_fields:
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=3, ldrb
    ra=3, s=Branch, brch=23, alu=Rotate, rb=3, ldrb
    ra=3, s=Bus, d=255, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=1, alu=Add, rb=3, ldrb
    d=0, r=Bus, rb=2, ldrb
indexed_fixed_zero:
    d=0, r=Bus, rb=7, ldrb
indexed_length_ready:
    ra=3, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=10, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_size
    ra=7, rb=11, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=10, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=indexed_index_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
indexed_index_ready:
    ra=4, s=Bus, d=0xfc000000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=4, rb=11, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
    ra=3, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_translate
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
indexed_translate:
    ra=4, s=Branch, brch=2, alu=Add, rb=11, ldrb
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_access
    read=Vr, vr=6
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=7, ldrb
    ra=7, s=Branch, brch=23, alu=Rotate, rb=7, ldrb
    ra=7, s=Bus, d=255, alu=And, rb=7, ldrb
    ra=7, shift=Left, rb=6, ldrb
    ra=6, shift=Left, rb=6, ldrb
    ra=6, s=Branch, brch=5, alu=Add, rb=6, ldrb
    ra=4, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=7, shift=Left, rb=6, ldrb
    ra=7, rb=6, alu=Add, ldrb
    ra=6, s=Branch, brch=1, alu=Add, rb=6, ldrb
    ra=4, rb=6, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=11, ldrb
indexed_access:
    ra=10, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_write
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    mem=Read
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_pointer_result
    d=Object, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=63, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_character_result
    seq=Jump, brch=positive_result
indexed_pointer_result:
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
indexed_character_result:
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=SPECIAL_24, page=Fetch
    ra=4, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
indexed_size:
    ra=3, rb=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=4, ldrb, seq=Jump, brch=positive_result
indexed_write:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_pointer
    read=Vr, vr=3
    d=Object, ldsym
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_numeric_value
    d=Symbol, page=Fetch
    read=Type
    d=SPECIAL_19, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, ldsym
indexed_numeric_value:
    d=indexed_numeric_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
indexed_numeric_ready:
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_numeric_store
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
indexed_numeric_store:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    d=Register, ra=4, mem=Write
    seq=Jump, brch=indexed_write_result
indexed_write_pointer:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
indexed_write_result:
    read=Vr, vr=3
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

; Decode unsigned 32-bit integers; preserve R0..3, R10..15 and all VRs.
; SYMBOL input, R4 result, R6 continuation, R7 scratch. LargePositiveInteger
; digits are little-endian raw bytes, never host integers or tagged references.
positive_value:
    d=Symbol, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=positive_large
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=Register, ra=6, seq=Bus
positive_large:
    d=Symbol, page=Fetch
    read=Type
    d=SPECIAL_13, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=7, ldrb
    ra=7, s=Branch, brch=4, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=7, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, ldsym
    ra=7, shift=Left, rb=4, ldrb
    ra=4, shift=Left, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Or, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=0, r=Bus, rb=4, ldrb
    d=3, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, s=Branch, brch=255, alu=And, ldq
    d=Q, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=Q, r=Bus, s=Register, rb=4, alu=Or, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_return
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, s=Branch, brch=255, alu=And, ldq
    d=Q, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=Object, r=Bus, s=Branch, brch=8, alu=Rotate, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Or, ldrb
    ra=7, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_return
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, s=Branch, brch=255, alu=And, ldq
    d=Q, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=Object, r=Bus, s=Branch, brch=16, alu=Rotate, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Or, ldrb
    ra=7, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_return
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, s=Branch, brch=255, alu=And, ldq
    d=Q, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=Object, r=Bus, s=Branch, brch=24, alu=Rotate, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Or, ldrb
positive_return:
    d=Register, ra=6, seq=Bus

; Unsigned word result. Values >=2^30 need four little-endian guest bytes.
positive_result:
    ra=4, s=Bus, d=0xc0000000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=positive_allocate
    ra=4, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result
positive_allocate:
    d=SPECIAL_13, page=Allocate, size=6, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=18, mem=Write
    idx=Increment
    d=0x8000, mem=Write
    d=indexed_hash_1, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
indexed_hash_1:
    idx=Increment
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    idx=Increment
    ra=4, s=Branch, brch=24, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    idx=Increment
    ra=4, s=Branch, brch=24, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    idx=Increment
    ra=4, s=Branch, brch=24, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    seq=Jump, brch=send_result

; Squeak stSizeOf: answers zero for SmallInteger.
indexed_integer_size:
    d=0xc200000000, ldvr, vr=5, seq=Jump, brch=send_result

; Class-controlled allocation. No host allocation or Squeak-aware hardware.
; Class guest field 2 contains its format: fixed+1 bits 1..6, kind bits 7..10.
; R2 physical kind, R3 original format, R4 total fields, R6 physical size,
; R10 fixed count. New bodies have descriptor/hash metadata and nil/zero data.
primitive_new:
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=new_indexed_arity
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=new_specification
new_indexed_arity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
new_specification:
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=7, shift=Right, rb=10, ldrb
    ra=10, s=Branch, brch=63, alu=And, rb=10, ldrb
    ra=10, s=Branch, brch=1, alu=Sub, cin=One, rb=10, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=7, s=Branch, brch=25, alu=Rotate, rb=3, ldrb
    ra=3, s=Branch, brch=15, alu=And, rb=3, ldrb
    d=0, r=Bus, rb=2, ldrb
    ra=3, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=new_kind_ready
    d=1, r=Bus, rb=2, ldrb
    ra=3, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=new_kind_ready
    d=2, r=Bus, rb=2, ldrb
    ra=3, s=Branch, brch=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
new_kind_ready:
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=new_indexed_count
    ra=10, rb=4, ldrb, seq=Jump, brch=new_allocate
new_indexed_count:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=new_count_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
new_count_ready:
    ; Bound before adding fixed fields or computing the descriptor.
    ra=4, s=Bus, d=0xff000000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, rb=10, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
new_allocate:
    ; Current generic object size limit: 16777215 physical words, metadata included.
    ra=4, s=Bus, d=16777213, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failed
    ra=4, s=Branch, brch=2, alu=Add, rb=6, ldrb
    ; Preserve the guest's low-space reserve before allocating. Collection may
    ; reclaim capacity, but a rejected allocation never creates a partial object.
    d=0, r=Bus, rb=7, ldrb
new_capacity:
    read=FreeWords
    d=Object, r=Bus, s=Register, rb=6, alu=Sub, cin=One, ldq, flags
    d=Q, r=Bus, rb=5, ldrb
    seq=ConditionalJump, cc=!Carry, brch=new_capacity_short
    ra=5, shift=Left, rb=5, ldrb
    ra=5, shift=Left, rb=5, ldrb
    d=11, r=Bus, rb=13, ldrb
    ra=13, d=Root, r=Bus, rb=13, ldrb
    ra=5, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=new_capacity_ready
new_capacity_short:
    ra=7, flags
    seq=ConditionalJump, cc=!Zero, brch=new_capacity_failed
    d=1, r=Bus, rb=7, ldrb
    gc=Collect
    seq=Jump, brch=new_capacity
new_capacity_failed:
    d=4, r=Bus, rb=7, ldrb
    ra=7, d=1, ldroot
    seq=Jump, brch=primitive_failed
new_capacity_ready:
    read=Vr, vr=6
    d=Object, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ra=4, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=new_descriptor
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
new_descriptor:
    ; Byte-format low bits retain the padding of a hypothetical packed image.
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=new_padding_ready
    ra=4, s=Branch, brch=0, alu=SubReverse, cin=One, rb=7, ldrb
    ra=7, s=Branch, brch=3, alu=And, rb=7, ldrb
    ra=7, rb=3, alu=Or, ldrb
new_padding_ready:
    ra=0, rb=2, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    d=2, idx=Load
    ra=3, s=Branch, brch=12, alu=Rotate, ldq
    d=Q, mem=Write
    d=new_hash_ready, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
new_hash_ready:
    d=3, r=Bus, rb=7, ldrb
new_initialize:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_result
    d=Register, ra=7, idx=Load
    ra=2, flags
    seq=ConditionalJump, cc=!Zero, brch=new_zero_field
    d=NIL, mem=Write
    seq=Jump, brch=new_next_field
new_zero_field:
    d=0, mem=Write
new_next_field:
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=new_initialize

; Set a new object's stored hash exactly as ObjectMemory>>newObjectHash.
; Selected object has its format in component 2. R7 return address, R0/R13/Q
; scratch; other registers/VRs survive. Root 2 is the 16-bit generator state.
allocation_hash:
    d=2, idx=Load, r=Bus, rb=13, ldrb
    mem=Read
    ra=13, d=Root, r=Bus, rb=0, ldrb
    ra=0, s=Bus, d=27181, alu=MultiplyUnsigned
    alu=ProductLow, rb=0, ldrb
    ra=0, s=Bus, d=13849, alu=Add, rb=0, ldrb
    ra=0, s=Bus, d=65535, alu=And, ldq
    ra=13, d=Q, ldroot
    d=Q, r=Bus, s=Branch, brch=4095, alu=And, rb=0, ldrb
    d=Object, r=Bus, s=Register, rb=0, alu=Or, ldq
    d=Q, mem=Write
    d=Register, ra=7, seq=Bus

primitive_hash:
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=4095, alu=And, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result

; Primitive 79: newMethod: byteCount header: header. Header/literals stay tagged;
; bytes stay opaque. R11 literal count, VR3 header, R4 byte count. Every size
; check precedes allocation. The class must describe a CompiledMethod format.
primitive_new_method:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=5, idx=Load
    mem=Read
    d=Object, r=Bus, rb=7, ldrb
    ra=7, s=Branch, brch=25, alu=Rotate, rb=7, ldrb
    ra=7, s=Branch, brch=15, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3, ldsym, r=Bus, rb=7, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=7, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=7, s=Branch, brch=23, alu=Rotate, rb=11, ldrb
    ra=11, s=Bus, d=255, alu=And, rb=11, ldrb
    idx=Decrement
    mem=Read
    d=Object, ldsym
    d=new_method_count, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
new_method_count:
    ra=4, s=Bus, d=0xff000000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, rb=11, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=3, alu=Add, rb=6, ldrb
    ra=6, s=Bus, d=16777215, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ra=11, s=Branch, brch=1, alu=Add, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, rb=4, alu=Add, ldq
    d=Q, r=Bus, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    d=2, idx=Load
    ra=4, s=Branch, brch=0, alu=SubReverse, cin=One, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=And, rb=0, ldrb
    ra=0, s=Branch, brch=12, alu=Or, rb=0, ldrb
    ra=0, s=Branch, brch=12, alu=Rotate, ldq
    d=Q, mem=Write
    d=new_method_hash, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
new_method_hash:
    d=3, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
    d=4, r=Bus, rb=7, ldrb
    ra=11, s=Branch, brch=3, alu=Add, rb=11, ldrb
new_method_fields:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_result
    d=Register, ra=7, idx=Load
    ra=11, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=new_method_byte
    d=NIL, mem=Write
    seq=Jump, brch=new_method_next
new_method_byte:
    d=0, mem=Write
new_method_next:
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=new_method_fields

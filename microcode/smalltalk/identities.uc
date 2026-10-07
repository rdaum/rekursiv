; Language identity conversion and instance enumeration. Generic directory
; commands return a canonical reference and its class without reading bodies.
; Full references are held in VRs/SYMBOL, never solely in NUMERIK registers.
;
; Preserve the pinned image's identity map: stored IDs <32768 use signed
; 15-bit values; 32768..65535 encode SmallIntegers. New stored IDs therefore
; encode as ID+32768 in a LargePositiveInteger. This map remains one-to-one
; over all 37 hardware identity bits and the guest's immediate integers.
primitive_identity_number:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym
    d=SymbolHigh, r=Bus, s=Branch, brch=192, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Symbol, r=Bus, rb=2, ldrb
    d=SymbolHigh, r=Bus, s=Branch, brch=31, alu=And, rb=3, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_extended
    ra=2, s=Bus, d=32768, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=identity_extended
    ra=2, s=Branch, brch=16384, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=identity_small_number
    ra=2, s=Bus, d=32768, alu=Sub, cin=One, rb=2, ldrb
identity_small_number:
    ra=2, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result
identity_extended:
    ra=2, s=Bus, d=32768, alu=Add, rb=2, ldrb, flags
    seq=ConditionalJump, cc=!Carry, brch=identity_count
    ra=3, s=Branch, brch=1, alu=Add, rb=3, ldrb
identity_count:
    d=5, r=Bus, rb=4, ldrb
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_allocate
    d=0, r=Bus, rb=4, ldrb
    ra=2, rb=5, ldrb
identity_count_byte:
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    ra=5, s=Branch, brch=24, alu=Rotate, rb=5, ldrb
    ra=5, s=Bus, d=0xffffff, alu=And, rb=5, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_count_byte
identity_allocate:
    ra=4, s=Branch, brch=1, alu=Add, rb=6, ldrb
    d=0xa00000000e, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ra=4, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, s=Branch, brch=2, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    d=2, r=Bus, rb=7, ldrb
identity_write_byte:
    ra=2, s=Branch, brch=255, alu=And, ldq
    d=Register, ra=7, idx=Load
    d=Q, mem=Write
    ra=7, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=send_result
    ; Shift the numeric 40-bit pair right by one byte.
    ra=2, s=Branch, brch=24, alu=Rotate, rb=2, ldrb
    ra=2, s=Bus, d=0xffffff, alu=And, rb=2, ldrb
    ra=3, s=Branch, brch=24, alu=Rotate, rb=3, ldrb
    ra=2, rb=3, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=3, ldrb
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=identity_write_byte

primitive_identity_object:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=identity_large_input
    ra=2, s=Bus, d=0xffffc000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failed
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failed
    ra=2, s=Branch, brch=32767, alu=And, rb=2, ldrb
    d=0, r=Bus, rb=3, ldrb, seq=Jump, brch=identity_find
identity_large_input:
    d=Symbol, page=Fetch
    read=Type
    d=0xa00000000e, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Object, r=Bus, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=4, s=Branch, brch=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=4, s=Branch, brch=1, alu=Add, rb=7, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=3, ldrb
identity_read_byte:
    d=Register, ra=7, idx=Load
    mem=Read
    d=Object, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ; Shift a 40-bit accumulator left by eight, then include the next byte.
    ra=2, s=Branch, brch=8, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=255, alu=And, rb=5, ldrb
    ra=3, s=Branch, brch=8, alu=Rotate, rb=3, ldrb
    ra=3, rb=5, alu=Or, ldq
    d=Q, r=Bus, rb=3, ldrb
    ra=2, s=Bus, d=0xffffff00, alu=And, rb=2, ldrb
    ra=2, rb=4, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_read_byte
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_decode_extended
    ra=2, s=Bus, d=32768, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=identity_find
    ra=2, s=Bus, d=65536, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=identity_decode_extended
    ra=2, s=Bus, d=49152, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=identity_negative_immediate
    ra=2, s=Bus, d=32768, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=identity_small_number
identity_negative_immediate:
    ra=2, s=Bus, d=65536, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=identity_small_number
identity_decode_extended:
    ra=2, s=Bus, d=32768, alu=Sub, cin=One, rb=2, ldrb, flags
    seq=ConditionalJump, cc=Carry, brch=identity_find
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb
identity_find:
    ra=3, s=Branch, brch=31, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=2, ldq
    d=Register, ra=3, r=Q, estk=Wide
    d=Estk, page=FindObject, vr=4
    d=Object, ldsym
    d=0xc000000000, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    d=Symbol, ldvr, vr=5, seq=Jump, brch=send_result

primitive_instances:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=77, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=instance_after_receiver
    read=Vr, vr=6
    d=Object, ldvr, vr=3
    d=0, page=NextObject, vr=4, seq=Jump, brch=instance_candidate
instance_after_receiver:
    read=Vr, vr=6
    d=Object, ldsym
    d=SymbolHigh, r=Bus, s=Branch, brch=192, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Symbol, page=FindObject, vr=3
    d=Object, ldsym
    d=0xc000000000, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    d=Symbol, page=NextObject, vr=4
instance_candidate:
    d=Object, ldsym
    d=0xc000000000, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    d=Symbol, ldvr, vr=5
    read=Vr, vr=3
    d=Object, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=send_result
    read=Vr, vr=5
    d=Object, page=NextObject, vr=4, seq=Jump, brch=instance_candidate

; Low-space registration is a seven-field guest Array rooted in slot30:
; semaphore, identity threshold in three 14-bit chunks (low first), then word
; threshold in three chunks. Every chunk is an ordinary positive SmallInteger.
; Copying numeric values prevents later mutation of an argument's byte object
; from changing the registration. No private untagged values masquerade as
; guest pointers; the generic collector scans this ordinary Array normally.
;
; A registration signals once when either available count is strictly below
; its threshold. Delivery clears slot30 before signal; registration rearms it.
; Nil cancels. Identity thresholds extend to 37 bits, word thresholds to 32.
primitive_low_space:
    ra=1, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=5, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    idx=Increment
    mem=Read
    d=Object, ldvr, vr=4
    idx=Increment
    mem=Read
    d=Object, ldvr, vr=5
    read=Vr, vr=3
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=low_space_cancel
    d=Object, page=Fetch
    read=Type
    d=0xa000000013, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=4
    d=Object, ldsym
    d=low_space_ids_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=unsigned40_value
low_space_ids_ready:
    ra=3, s=Branch, brch=31, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=2, rb=10, ldrb
    ra=3, rb=11, ldrb
    read=Vr, vr=5
    d=Object, ldsym
    d=low_space_words_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=unsigned40_value
low_space_words_ready:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, rb=12, ldrb
    d=0xa000000008, page=Allocate, size=8, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=56, mem=Write
    idx=Increment
    read=Vr, vr=3
    d=Object, mem=Write
    idx=Increment
    ra=10, s=Branch, brch=16383, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=10, s=Branch, brch=18, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=16383, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=10, s=Branch, brch=4, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=15, alu=And, rb=2, ldrb
    ra=11, s=Branch, brch=4, alu=Rotate, rb=3, ldrb
    ra=2, rb=3, alu=Or, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=12, s=Branch, brch=16383, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=12, s=Branch, brch=18, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=16383, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=12, s=Branch, brch=4, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=15, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    d=30, r=Bus, rb=7, ldrb
    read=Vr, vr=5
    ra=7, d=Object, ldroot
    seq=Jump, brch=clock_result
low_space_cancel:
    d=30, r=Bus, rb=7, ldrb
    ra=7, d=NIL, ldroot
    seq=Jump, brch=clock_result

; Decode a nonnegative SmallInteger or 1..5-byte LargePositiveInteger in SYMBOL.
; R2/R3 receive low32/high8. R4/R5/R7 are scratch; R6 is the continuation.
; This helper is a primitive operand check, so every error preserves the send.
unsigned40_value:
    d=Symbol, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=unsigned40_large
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0, r=Bus, rb=3, ldrb
    d=Register, ra=6, seq=Bus
unsigned40_large:
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
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=4, s=Branch, brch=1, alu=Add, rb=7, ldrb
    read=Size
    d=Object, r=Bus, s=Register, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=3, ldrb
unsigned40_byte:
    d=Register, ra=7, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
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
    seq=ConditionalJump, cc=!Zero, brch=unsigned40_byte
    d=Register, ra=6, seq=Bus

; Boundary checks preserve R8..R14 and VR0..VR2. Threshold storage is an
; ordinary object, so fetching it may page or collect. Sample each hardware
; count only AFTER its threshold reads, against the resulting allocator state.
check_low_space:
    d=30, r=Bus, rb=7, ldrb
    d=NIL, ldsym
    ra=7, d=Root, seq=ConditionalJump, cc=Symbol, brch=check_device_events
    ra=7, d=Root, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    d=3, idx=Load
    d=low_space_check_ids, r=Bus, rb=7, ldrb, seq=Jump, brch=low_space_chunks
low_space_check_ids:
    read=FreeIdentities
    d=Object, ldsym, r=Bus, rb=4, ldrb
    d=SymbolHigh, r=Bus, rb=5, ldrb
    ra=5, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=low_space_signal
    seq=ConditionalJump, cc=!Zero, brch=low_space_check_words
    ra=4, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=low_space_signal
low_space_check_words:
    d=6, idx=Load
    d=low_space_words_checked, r=Bus, rb=7, ldrb, seq=Jump, brch=low_space_chunks
low_space_words_checked:
    read=FreeWords
    d=Object, r=Bus, rb=4, ldrb
    ra=4, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=low_space_signal
    d=0, ldvr, vr=5
    seq=Jump, brch=check_device_events
low_space_signal:
    ; Fetch the recipient before releasing the registration; it then remains
    ; in VR6 across any collection or preemption during the ordinary signal.
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=6
    d=30, r=Bus, rb=7, ldrb
    ra=7, d=NIL, ldroot
    d=85, r=Bus, rb=0, ldrb
    d=1, r=Bus, rb=1, ldrb, seq=Jump, brch=scheduler_load

; Rebuild an unsigned value from three positive 14-bit SmallInteger chunks
; at IDX..IDX+2. R2/R3 receive low32/high bits, R4 is scratch, R7 is return.
low_space_chunks:
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=14, alu=Rotate, rb=4, ldrb
    ra=2, rb=4, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=28, alu=Rotate, rb=3, ldrb
    ra=3, s=Bus, d=0xf0000000, alu=And, rb=4, ldrb
    ra=2, rb=4, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=3, s=Branch, brch=31, alu=And, rb=3, ldrb
    d=Register, ra=7, seq=Bus

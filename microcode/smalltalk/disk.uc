; AltoFile storage policy belongs to the language runtime. Primitive 135
; selects the four-byte serial number and unsigned 16-bit virtual leader page
; for a later snapshot; it does not write an image or invoke a host filename
; resolver. The snapshot writer remains separate machine code (stage 5).
;
; Root29's private device Array retains a registration in component40. It is
; an ordinary six-field Array: four serial bytes, leader low14, leader high2.
; Copy values so later mutation of either argument cannot change the target.
; Build the replacement completely before publishing it. Every validation
; failure preserves the old registration, receiver, and original arguments.
;
; VR3 roots the serial argument; VR4 the new registration; VR6 the receiver;
; VR7 the fallback method. R10..R13 hold serial bytes, R14 the leader address.
; R1 (arity), R8/R9 (caller IP/SP), and R15 (failure continuation) stay intact.
primitive_snapshot_target:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=6, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    idx=Increment
    mem=Read
    d=Object, ldsym
    d=snapshot_leader_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=unsigned40_value
snapshot_leader_ready:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, s=Bus, d=65535, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=2, rb=14, ldrb
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Representation
    d=Object, ldsym
    d=18, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed ; four bytes, kind2
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=10, ldrb
    d=Register, ra=10, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=10, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=11, ldrb
    d=Register, ra=11, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=11, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=12, ldrb
    d=Register, ra=12, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=12, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=13, ldrb
    d=Register, ra=13, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=13, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0xa000000008, page=Allocate, size=7, scan=1
    d=Object, ldvr, vr=4
    d=1, idx=Load
    d=48, mem=Write
    idx=Increment
    ra=10, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=11, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=12, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=13, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=14, s=Branch, brch=16383, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=14, s=Branch, brch=18, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=3, alu=And, estk=Compact, compact=2
    d=Estk, mem=Write
    d=snapshot_publish, r=Bus, rb=7, ldrb, seq=Jump, brch=input_buffer_load
snapshot_publish:
    d=40, idx=Load
    read=Vr, vr=4
    d=Object, mem=Write
    seq=Jump, brch=clock_result

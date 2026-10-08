; Missing lookup creates a guest argument Array and two-field Message.
; VR4 selector, VR6 receiver, R1 arity; arguments are copied before mutation.
; Then send doesNotUnderstand:. A missing DNU method stops with status 6.
lookup_failed:
    ra=15, s=Branch, brch=perform_found, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_transform
    read=Vr, vr=4
    d=SPECIAL_20, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=recursive_not_understood
    ra=1, s=Branch, brch=2, alu=Add, rb=6, ldrb
    d=SPECIAL_7, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=7
    d=1, idx=Load
    ra=1, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    d=2, idx=Load
    d=0x2000, mem=Write
    d=messages_hash_1, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
messages_hash_1:
    d=0, r=Bus, rb=7, ldrb
message_copy_arguments:
    ra=1, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=message_allocate
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=7
    d=Object, page=Fetch
    ra=7, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=message_copy_arguments
message_allocate:
    d=SPECIAL_15, page=Allocate, size=4, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=32, mem=Write
    idx=Increment
    d=0x1000, mem=Write
    d=messages_hash_2, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
messages_hash_2:
    idx=Increment
    read=Vr, vr=4
    d=Object, mem=Write
    idx=Increment
    read=Vr, vr=7
    d=Object, mem=Write
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, rb=7, ldrb
message_clear_arguments:
    ra=7, flags
    seq=ConditionalJump, cc=Zero, brch=message_push
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb
    seq=Jump, brch=message_clear_arguments
message_push:
    ra=14, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
    d=SPECIAL_20, ldvr, vr=4
    d=1, r=Bus, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare

; Shared execution, parameterized by profile layout constants.
push_receiver_field:
    ra=13, s=Bus, d=15, alu=And, rb=1, ldrb
    d=0, r=Bus, rb=3, ldrb, seq=Jump, brch=receiver_target
push_temporary:
    ra=13, s=Bus, d=15, alu=And, rb=1, ldrb
    d=0, r=Bus, rb=3, ldrb, seq=Jump, brch=temporary_target
push_literal:
    ra=13, s=Bus, d=31, alu=And, rb=1, ldrb
    d=2, r=Bus, rb=2, ldrb
    seq=Jump, brch=push_variable
push_association:
    ra=13, s=Bus, d=31, alu=And, rb=1, ldrb
    d=3, r=Bus, rb=2, ldrb
push_variable:
    d=0, r=Bus, rb=3, ldrb
    seq=Jump, brch=variable
store_receiver:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    d=2, r=Bus, rb=3, ldrb, seq=Jump, brch=receiver_target
store_temporary:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    d=2, r=Bus, rb=3, ldrb, seq=Jump, brch=temporary_target
; Opcodes 128/129/130 become mode 0 push / 1 store / 2 store-and-pop.
; The extension byte supplies kind in bits 6..7 and zero-based index in 0..5.
extended:
    ra=13, s=Branch, brch=128, alu=Sub, cin=One, rb=3, ldrb
    d=extended_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
extended_decoded:
    ra=0, s=Bus, d=63, alu=And, rb=1, ldrb
    ra=0, s=Branch, brch=26, alu=Rotate, rb=2, ldrb
    ra=2, s=Bus, d=0x03ffffff, alu=And, rb=2, ldrb
; R2 kind: 0 receiver, 1 home temporary, 2 literal, 3 literal Association.
; Resolve and root a writable target in VR3; literals themselves are read-only.
variable:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=receiver_target
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=temporary_target
    ra=10, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch, ra=1, s=Branch, brch=FIELD_BASE+1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=association_target
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    d=Object, ldsym, seq=Jump, brch=push
association_target:
    d=Object, ldvr, vr=3
    d=FIELD_BASE+1, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    seq=Jump, brch=access_variable
receiver_target:
    read=Vr, vr=2
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch, ra=1, s=Branch, brch=FIELD_BASE, alu=Add, ldq
    d=Q, idx=Load
    seq=Jump, brch=access_variable
temporary_target:
    d=temporary_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
temporary_home_ready:
    ; Read the home method's temporary count. R12 is the evaluation-stack floor,
    ; which is zero inside a block and therefore cannot validate this access.
    d=FIELD_BASE, idx=Load, read=Vr, vr=1
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, s=Branch, brch=TEMP_ROTATE, alu=Rotate, rb=0, ldrb
    ra=0, s=Branch, brch=TEMP_MASK, alu=And, rb=0, ldrb
    ra=0, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=3
    d=Object, page=Fetch, ra=1, s=Branch, brch=CTX_STACK+1, alu=Add, ldq
    d=Q, idx=Load
access_variable:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=write_variable
    mem=Read
    d=Object, ldsym, seq=Jump, brch=push
; peek selects the caller, so preserve the target index in R1 first. VR3
; keeps the target alive and VR4 keeps the value alive while refetching it.
write_variable:
    read=Index
    d=Object, r=Bus, rb=1, ldrb
    d=write_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
write_value:
    d=Symbol, ldvr, vr=4
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=1, idx=Load
    read=Vr, vr=4
    d=Object, mem=Write
    ra=3, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=pop
    seq=Jump, brch=boundary

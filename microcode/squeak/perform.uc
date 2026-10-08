; Squeak perform: and perform:withArguments: reuse the send engine.
; Validate target arity before rewriting caller operands. Private ESTK slots
; retain the primitive method and argument Array across lookup/collection.
; R11 target arity, R12 primitive, CSTK saves the caller temporary floor.
primitive_perform:
    ra=0, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_array_check
    ra=1, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb
    d=NIL, ldvr, vr=3
    seq=Jump, brch=perform_save
perform_array_check:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_7, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=11, ldrb
    ra=9, s=Branch, brch=2, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=11, alu=Add, ldq
    d=Q, r=Bus, s=Register, rb=14, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
perform_save:
    d=Register, ra=12, cstk=Bus
    ra=0, rb=12, ldrb
    d=1, esp=Bus
    read=Vr, vr=7
    d=Object, estk=Bus
    d=2, esp=Bus
    read=Vr, vr=3
    d=Object, estk=Bus
    d=2, sp=Bus
    d=0, esp=Bus
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    d=perform_found, r=Bus, rb=15, ldrb
    d=0, r=Bus, rb=2, ldrb
    seq=Jump, brch=lookup_receiver_class
perform_found:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ra=5, s=Branch, brch=8, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=31, alu=And, rb=2, ldrb
perform_arity:
    ra=2, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=perform_failed
perform_transform:
    ra=9, rb=10, ldrb
    d=0, r=Bus, rb=7, ldrb
perform_copy:
    ra=11, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_copied
    ra=12, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_array_element
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=10, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    seq=Jump, brch=perform_store_element
perform_array_element:
    d=2, esp=Bus
    estk=Read
    d=Estk, page=Fetch
    d=0, esp=Bus
    ra=7, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
perform_store_element:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=perform_copy
perform_copied:
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=11, alu=Add, ldq
    d=Q, r=Bus, rb=9, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
perform_clear:
    ra=10, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_send
    seq=ConditionalJump, cc=Sign, brch=perform_send
    ra=10, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=10, s=Branch, brch=1, alu=Sub, cin=One, rb=10, ldrb
    seq=Jump, brch=perform_clear
perform_send:
    ra=11, rb=1, ldrb
    d=Cstk, r=Bus, rb=12, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare
perform_failed:
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=7
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=Cstk, r=Bus, rb=12, ldrb
    seq=Jump, brch=primitive_method_failed

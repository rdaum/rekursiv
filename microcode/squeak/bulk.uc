; Primitive 105 follows the archived forward-copy semantics, including aliases.
; Bounds, fixed fields and original formats are checked before the first write.
; Header/literal byte aliases on CompiledMethod remain a primitive failure.
primitive_replace:
    ra=1, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=12, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=3
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=11, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=10, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=12, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=11, rb=10, alu=Sub, cin=One, ldq, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    r=Q, s=Branch, brch=1, alu=Add, rb=11, ldrb
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    d=replace_destination, r=Bus, rb=6, ldrb, seq=Jump, brch=bulk_layout
replace_destination:
    ra=2, rb=13, ldrb
    ra=5, rb=14, ldrb
    ra=4, rb=10, alu=Add, ldrb
    ra=10, rb=11, alu=Add, ldq
    r=Q, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
    ra=3, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=10, s=Branch, brch=2, alu=Add, rb=10, ldrb
    read=Vr, vr=3
    d=Object, ldvr, vr=5
    d=replace_source, r=Bus, rb=6, ldrb, seq=Jump, brch=bulk_layout
replace_source:
    ra=2, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=5, rb=14, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, rb=12, alu=Add, ldrb
    ra=12, rb=11, alu=Add, ldq
    r=Q, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
    ra=3, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=12, s=Branch, brch=2, alu=Add, rb=12, ldrb
replace_element:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=12, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=10, idx=Load
    d=Symbol, mem=Write
    ra=11, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=device_receiver_result
    ra=10, s=Branch, brch=1, alu=Add, rb=10, ldrb
    ra=12, s=Branch, brch=1, alu=Add, rb=12, ldrb
    seq=Jump, brch=replace_element

; VR5 object -> R2 kind, R3 total elements, R4 fixed fields, R5 format.
; Return R6. Byte padding bits are excluded when comparing formats.
bulk_layout:
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=3, ldrb
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=20, alu=Rotate, rb=5, ldrb
    ra=5, s=Branch, brch=15, alu=And, rb=5, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bulk_fixed
    ra=5, s=Branch, brch=12, alu=And, rb=5, ldrb
bulk_fixed:
    read=Type
    d=Object, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, r=Bus, shift=Right, rb=4, ldrb
    ra=4, s=Branch, brch=63, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=Register, ra=6, seq=Bus

primitive_fill:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=10, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=fill_length
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=10, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
fill_length:
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=11, ldrb
    d=3, idx=Load
fill_element:
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=device_receiver_result
    d=Register, ra=10, mem=Write
    idx=Increment
    ra=11, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb
    seq=Jump, brch=fill_element

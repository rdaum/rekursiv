; Identity and class primitives use full tagged values, never payload identity.
; R1 arity, VR6 receiver, VR5 result; common send_result consumes operands.
primitive_identity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=6
    d=Object, seq=ConditionalJump, cc=Symbol, brch=primitive_true
primitive_false:
    d=FALSE, ldvr, vr=5, seq=Jump, brch=send_result
primitive_true:
    d=TRUE, ldvr, vr=5, seq=Jump, brch=send_result
primitive_class:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

; Identity and class primitives, plus the shared Boolean result exits.
; Entry primitive_identity: primitive 110 or special bytecode 198, one argument.
; Compare the complete 40-bit receiver and argument; never compare only payloads.
; Entry primitive_class: primitive 111 or special bytecode 199, no arguments.
; Fetch selects even a compact receiver; read=Type uses its compact-class mapping.
;
; Inputs follow sends.uc: VR0 caller, VR6 receiver, R1 arity, R9 caller SP,
; R15 failure continuation. Scratch: Q, SYMBOL, selection, IDX, Object, flags.
; primitive_true/primitive_false put guest singleton references in VR5 and enter
; send_result. These exits are also used by comparisons and stream end tests.
; Arity failures jump to primitive_failed without modifying guest stack fields.
;
; Primitive results use VR5 and the common send_result path. Rejected operands
; jump through R15 without changing caller fields. Method fallback reloads its
; header from the rooted method in VR7; special sends resume dictionary lookup.
primitive_identity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
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

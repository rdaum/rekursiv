; Shared argument transfer, send results and sender resumption.
copy_argument:
    ; Copy receiver first, then arguments in source order. A tagged SYMBOL
    ; bridges the two fetches. Clear each source only after the destination write.
    read=Vr, vr=0
    d=Object, page=Fetch, ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=CTX_STACK, alu=Add, rb=6, ldrb
    d=Register, ra=6, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=5
    d=Object, page=Fetch, ra=7, s=Branch, brch=CTX_STACK, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    read=Vr, vr=0
    d=Object, page=Fetch
    d=Register, ra=6, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    ra=1, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=copy_argument
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    d=CTX_IP+1, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

; Quick methods and successful allocation replace receiver+arguments by a result
; without an activation. In particular, no dummy MethodContext is allocated.
send_result:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, s=Branch, brch=1, alu=Add, rb=7, ldrb
clear_send_operands:
    ra=9, s=Branch, brch=CTX_STACK, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=clear_send_operands
    d=16, r=Bus, rb=15, ldrb
    ; Restore the caller's header cache, which lookup used for the new method.
    d=CTX_IP+1, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=load_context

; Object is the proposed sender/caller. A nil saved IP marks an inactive
; return target and routes to cannot_return before modifying the outgoing frame.
return_sender:
    ; Result and destination must both survive a destination refill.
    d=Object, ldvr, vr=7
    d=CTX_IP, idx=Load, read=Vr, vr=7
    d=Object, page=Fetch
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=cannot_return
    d=FIELD_BASE, idx=Load, read=Vr, vr=0
    d=Object, page=Fetch
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    read=Vr, vr=7
    d=Object, ldvr, vr=0
    d=16, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context
resume_result:
    read=Vr, vr=5
    d=Object, ldsym, seq=Jump, brch=push
recursive_not_understood:
    d=6, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context
argument_mismatch:
    d=7, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

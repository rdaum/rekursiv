; Keep the result rooted in VR5 before reading or refilling the sender.
; A nil sender terminates the diagnostic root; sends.uc handles other returns.
;
; Shared execution, parameterized by profile layout constants.
return_receiver:
    read=Vr, vr=2
    d=Object, ldsym, seq=Jump, brch=return_value
return_true:
    d=TRUE, ldsym, seq=Jump, brch=return_value
return_false:
    d=FALSE, ldsym, seq=Jump, brch=return_value
return_nil:
    d=NIL, ldsym, seq=Jump, brch=return_value
return_top:
return_block:
    d=return_pop, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
return_pop:
    ; Keep the value in SYMBOL while removing it from the outgoing stack.
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
return_value:
    d=Symbol, ldvr, vr=5
    ra=13, s=Branch, brch=125, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=return_local
    d=return_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
return_home_ready:
    read=Vr, vr=3
    d=Object, page=Fetch
    seq=Jump, brch=return_target
return_local:
    read=Vr, vr=0
    d=Object, page=Fetch
; Selected object is the home method for nonlocal returns, active context
; for bytecode 125. FIELD_BASE supplies sender/caller; validate before unlinking.
return_target:
    d=FIELD_BASE, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_sender
    ; Only the boot activation has a diagnostic nil-sender return. Escaped
    ; blocks and manipulated sender chains send cannotReturn: on the machine.
    read=Vr, vr=0
    d=ACTIVE_CONTEXT, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=cannot_return
    d=1, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

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
    ; The new frame was built here, so its caches come from the activation
    ; instead of load_context: R10 literal count, R4 temporaries, R14 stack
    ; capacity, VR7 method, VR6 receiver, VR5 context. Only the method's byte
    ; length is read, from its descriptor.
    read=Vr, vr=7
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, shift=Right, rb=11, ldrb
    ra=11, shift=Right, rb=11, ldrb
    read=Vr, vr=7
    d=Object, ldvr, vr=1
    read=Vr, vr=6
    d=Object, ldvr, vr=2
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    ra=10, shift=Left, rb=8, ldrb
    ra=8, s=Branch, brch=3, alu=Add, rb=8, ldrb
    ra=4, rb=9, ldrb
    ra=4, rb=12, ldrb
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=boundary

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
    d=CTX_IP+1, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ; The caller is still in VR0..VR2; only the caches lookup and the
    ; primitive used need rebuilding.
    seq=Jump, brch=reload_frame

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
    ; Context recycling (see sends.uc). R2: 0 block to caller, 1 method from
    ; itself, 2 block from its home.
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=return_resume
    seq=ConditionalJump, cc=!Zero, brch=return_shared
    ; A method frame returns. With a zero count it may be shared: leave it.
    seq=ConditionalJump, cc=ControlZero, brch=return_resume
    cstk=Decrement
; Clear its temporaries and stack, slots 1..SP, so it is ready for reuse.
return_clear:
    ra=9, flags
    seq=ConditionalJump, cc=Zero, brch=return_park
    ra=9, s=Branch, brch=CTX_STACK, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=return_clear
return_park:
    ; Park it with the sender's home MethodContext, VR3: the sender itself,
    ; or a BlockContext sender's home.
    read=Vr, vr=7
    d=Object, ldvr, vr=3
    read=Vr, vr=7
    d=Object, page=Fetch
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=return_keep
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_resume
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_resume
return_keep:
    d=6, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
; A nonzero count means the sender, if it is a MethodContext, is one of the
; counted frames: built by this microcode and never handed to the guest. Its
; fields are then known good, and it is reloaded without load_context's
; checks. Its SP, method and receiver are components 4, 5 and 7.
return_counted:
    seq=ConditionalJump, cc=ControlZero, brch=return_resume
    read=Vr, vr=7
    d=Object, page=Fetch
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_resume
    read=Vr, vr=7
    d=Object, ldvr, vr=0
    d=CTX_IP+1, idx=Load
    mem=Read, idx=Increment
    d=Object, r=Bus, rb=9, ldrb, mem=Read
    d=Object, ldvr, vr=1
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=2, seq=Jump, brch=reload_frame
return_shared:
    ; A block returned from its home method, past frames in between.
    d=0, cstk=Bus
return_resume:
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

; Rebuild the caches of a trusted frame whose context, method and receiver are
; in VR0, VR1 and VR2 and whose SP is in R9: the caller after a quick method
; or primitive, or a counted sender after a return. Sets R8 IP, R14 stack
; capacity, R10 literal count, R11 byte length and R12 stack floor, then
; pushes VR5 through resume_result. load_context does the same with the
; checks that a context from the guest needs.
reload_frame:
    read=Vr, vr=0
    d=Object, page=Fetch
    d=CTX_IP, idx=Load, read=Size
    d=Object, r=Bus, s=Branch, brch=7, alu=Sub, cin=One, rb=14, ldrb
    mem=Read
    d=Object, r=Bus, rb=8, ldrb
    read=Vr, vr=1
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, shift=Right, rb=11, ldrb
    ra=11, shift=Right, rb=11, ldrb, d=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=0, s=Bus, d=63, alu=And, rb=10, ldrb
    ra=0, s=Branch, brch=25, alu=Rotate, rb=12, ldrb
    ra=12, s=Bus, d=31, alu=And, rb=12, ldrb
    ; A block's evaluation stack starts at zero; its temporaries are its home's.
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Type
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=resume_result
    d=0, r=Bus, rb=12, ldrb, seq=Jump, brch=resume_result

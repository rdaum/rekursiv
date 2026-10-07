; Primitive 112 word capacity query uses FreeWords; primitive
; 115 identity capacity uses FreeIdentities. Both require zero arguments and
; return an unsigned guest integer. R2/R3 hold low32/high8; large results share
; identity_count in identities.uc, so even a 37-bit count retains every bit.
; FreeWords counts the allocator's available tail, not all reclaimable garbage.
; Capacity primitives do not request collection just to measure free space.
;
; Primitives 113 quit and 114 debugger also take zero arguments. They materialize
; the already-advanced caller IP/SP, root the receiver in VR5, and stop with R15
; 10 (halt) or 11 (resumable Service). The receiver already occupies its zero-arg
; send result slot. system_debugger_resume resets R15 and enters load_context;
; it must continue after the completed send, not dispatch the primitive again.
; These status values are language runtime diagnostics, separate from OBJEKT
; command error codes. See ../../docs/smalltalk-execution.md for stop inspection.
;
; System primitives retain the hardware/language boundary: capacity comes from
; generic OBJEKT reads; guest integer construction executes here. Counts are a
; snapshot before allocating any LargePositiveInteger used to return the value.
primitive_capacity:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=112, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=capacity_identities
    read=FreeWords
    seq=Jump, brch=capacity_value
capacity_identities:
    read=FreeIdentities
capacity_value:
    d=Object, ldsym, r=Bus, rb=2, ldrb
    d=SymbolHigh, r=Bus, rb=3, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=identity_count
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=identity_count
    ra=2, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result

; Quit stops the machine with status 10. Debugger entry reports status 11 and
; takes LOGIK's resumable service break. No host language handler is involved.
; Both are zero-argument sends, so the existing receiver remains their result
; on the guest stack. Save the advanced IP/SP before either stop. Debug resume
; reloads the caller caches before the next bytecode; it does not repeat the send.
primitive_system_stop:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    read=Vr, vr=0
    d=Object, page=Fetch
    d=3, idx=Load
    ra=8, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=0, s=Branch, brch=113, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=system_debugger
    d=10, r=Bus, rb=15, ldrb, halt
system_debugger:
    d=11, r=Bus, rb=15, ldrb, seq=Service, brch=11
system_debugger_resume:
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

; Shared Xerox/Squeak execution; layout constants come from the profile.
boundary:
    ; Release completed-op scratch roots (VR3-VR7) after their values reach
    ; guest fields. The clears ride on words that run anyway: VR7, VR6 and
    ; VR5 here, VR4 and VR3 on decoded's NAM and MAP fetches, which use no D
    ; bus. fetch_byte, in between, never touches VR3-VR6. Leaving early for
    ; deferred work clears them all first.
    ; IP and SP stay in R8 and R9 between bytecodes. They are written into
    ; the context where something can observe it: a send (send_prepare), a
    ; process switch (check_process_switch), and a stop (save_context).
    ; Deferred work raises the pending flag: a pending process switch, idle,
    ; or input words awaiting signals. With it clear, the low-space line low
    ; and no interrupt, the next bytecode starts at once.
    d=0, ldvr, vr=7, r=Bus, rb=15, ldrb, seq=ConditionalJump, cc=Pending, brch=boundary_deferred
    d=0, ldvr, vr=6, seq=ConditionalJump, cc=LowSpace, brch=boundary_deferred
    d=0, ldvr, vr=5, seq=ConditionalJump, cc=!Interrupt, brch=cycle
boundary_deferred:
    d=0, ldvr, vr=3
    d=0, ldvr, vr=4
    d=0, ldvr, vr=5
    d=0, ldvr, vr=6, seq=Jump, brch=check_process_switch
save_context:
    ; Select the caller and prepare IDX/IP in parallel. Fetch preserves IDX,
    ; including through refill/collection. Writes consume old IDX/ESTKR.
    d=CTX_IP, idx=Load, read=Vr, vr=0, ra=8, estk=Compact, compact=2
    d=Object, page=Fetch
    d=Estk, mem=Write, idx=Increment, ra=9, estk=Compact, compact=2
    d=Estk, mem=Write, ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=stopped
    seq=Jump, brch=check_process_switch
; Each dialect's cycle fetches the next bytecode and enters decode_byte with
; it in R0, or decoded with it in R0 and the APC. Tests observe cycle without
; changing machine state or supplying work.
decode_byte:
    ; Register write and APC both consume the fetched byte in old R0.
    d=Register, ra=0, rb=13, ldrb, apc=Bus
decoded:
    fetch=Nam, d=0, ldvr, vr=4, ra=0, rb=13, ldrb
    fetch=Map, d=0, ldvr, vr=3
    seq=Dispatch

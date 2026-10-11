; Shared Xerox/Squeak execution; layout constants come from the profile.
boundary:
    ; Release completed-op scratch roots after their values reach guest fields.
    d=0, ldvr, vr=3
    d=0, ldvr, vr=4
    d=0, ldvr, vr=5
    d=0, ldvr, vr=6
    d=0, ldvr, vr=7, r=Bus, rb=15, ldrb
save_context:
    ; Select the caller and prepare IDX/IP in parallel. Fetch preserves IDX,
    ; including through refill/collection. Writes consume old IDX/ESTKR.
    d=CTX_IP, idx=Load, read=Vr, vr=0, ra=8, estk=Compact, compact=2
    d=Object, page=Fetch
    d=Estk, mem=Write, idx=Increment, ra=9, estk=Compact, compact=2
    d=Estk, mem=Write, ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=stopped
    seq=Jump, brch=check_process_switch
; Tests observe this boundary without changing machine state or supplying work.
cycle:
    d=decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
decoded:
    ; Register write and APC both consume the fetched byte in old R0.
    d=Register, ra=0, rb=13, ldrb, apc=Bus
    fetch=Nam
    fetch=Map
    seq=Dispatch

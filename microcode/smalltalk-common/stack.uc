; Shared Xerox/Squeak execution; layout constants come from the profile.
push_receiver:
    read=Vr, vr=2
    d=Object, ldsym, seq=Jump, brch=push
push_true:
    d=TRUE, ldsym, seq=Jump, brch=push
push_false:
    d=FALSE, ldsym, seq=Jump, brch=push
push_nil:
    d=NIL, ldsym, seq=Jump, brch=push
push_integer:
    ra=13, s=Branch, brch=117, alu=Sub, cin=One, estk=Compact, compact=2
    d=Estk, ldsym, seq=Jump, brch=push
push_context:
    read=Vr, vr=0
    d=Object, ldsym, seq=Jump, brch=push
duplicate:
    d=push, r=Bus, rb=6, ldrb, seq=Jump, brch=peek

; Peek preserves SP and returns its tagged value in SYMBOL. Push consumes
; SYMBOL. Pop clears the dead slot before reducing SP, keeping tracing precise.
peek:
    ; Caches have nonnegative, validated frame bounds. Carry tests floor >= SP.
    ra=12, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch, ra=9, s=Branch, brch=CTX_STACK, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=Register, ra=6, seq=Bus
push:
    ra=9, rb=14, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bad_state
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb, read=Vr, vr=0
    d=Object, page=Fetch, ra=9, s=Branch, brch=CTX_STACK, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write, seq=Jump, brch=boundary
pop:
    d=pop_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
pop_value:
    d=NIL, mem=Write, ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=boundary

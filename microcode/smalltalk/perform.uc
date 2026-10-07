; Dynamic sends: primitive 83 perform: with selector plus zero or more direct
; arguments; 84 perform:withArguments: with selector and one Array. VR6 remains
; the eventual receiver. R11 holds target arity; R12 temporarily holds primitive
; 83/84; R10 later holds the old SP; R7 is the argument-copy cursor.
;
; Save the original primitive method and optional Array, then reuse sends.uc's
; lookup with R15=perform_found as a sentinel. Read the found method's header,
; including extended arity, before any caller operand changes. Arity mismatch
; restores VR7 and the stack floor and activates the original primitive method.
; A lookup miss instead enters perform_transform via messages.uc and lets an
; ordinary send construct doesNotUnderstand: with the transformed arguments.
;
; Transformation removes the selector slot: copy direct args one slot downward
; or expand Array contents into the caller, update R9 and R1, clear surplus slots,
; restore R12, release private ESTK roots, and reenter send_prepare with R13=0.
; The Array form checks available caller capacity before lookup. After operand
; rewriting starts, failure cannot return to the original perform call.
;
; Dynamic sends validate the target arity before changing caller operands.
; During lookup, expression-stack slot1 roots the original primitive method and
; slot2 roots the argument Array. ESP returns to zero so compact scratch writes
; cannot overwrite either root. SP=2 exposes them to GC. Control-stack slot0
; saves the caller's numeric stack floor. No guest execution occurs in this state.
primitive_perform:
    ra=0, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_array_check
    ra=1, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb
    d=NIL, ldvr, vr=3
    seq=Jump, brch=perform_save
perform_array_check:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=0xa000000008, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb
    ra=9, s=Branch, brch=2, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=11, alu=Add, ldq
    d=Q, r=Bus, s=Register, rb=14, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
perform_save:
    d=Register, ra=12, cstk=Bus
    ra=0, rb=12, ldrb
    d=1, esp=Bus
    read=Vr, vr=7
    d=Object, estk=Bus
    d=2, esp=Bus
    read=Vr, vr=3
    d=Object, estk=Bus
    d=2, sp=Bus
    d=0, esp=Bus
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    d=perform_found, r=Bus, rb=15, ldrb
    d=0, r=Bus, rb=2, ldrb
    seq=Jump, brch=lookup_receiver_class
; Lookup found VR7 without invoking it. Decode arity even for quick and
; extended methods; on mismatch the original primitive stack is still intact.
perform_found:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ra=5, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, s=Branch, brch=7, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=perform_arity
    ra=2, s=Branch, brch=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_extended
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=perform_arity
perform_extended:
    ra=5, s=Branch, brch=63, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=perform_failed
    ra=4, s=Branch, brch=1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, s=Branch, brch=31, alu=And, rb=2, ldrb
perform_arity:
    ra=2, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=perform_failed
perform_transform:
    ; For a missing target, transform first, then let an ordinary send create
    ; doesNotUnderstand:'s Message from the actual dynamic-send arguments.
    ra=9, rb=10, ldrb
    d=0, r=Bus, rb=7, ldrb
perform_copy:
    ra=11, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_copied
    ra=12, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_array_element
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    seq=Jump, brch=perform_store_element
perform_array_element:
    d=2, esp=Bus
    estk=Read
    d=Estk, page=Fetch
    d=0, esp=Bus
    ra=7, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
perform_store_element:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=perform_copy
; New SP = old SP - original primitive arity + target arity. Clear only
; slots above the new SP, then drop the private hardware-stack roots.
perform_copied:
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=11, alu=Add, ldq
    d=Q, r=Bus, rb=9, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
perform_clear:
    ra=10, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_send
    seq=ConditionalJump, cc=Sign, brch=perform_send
    ra=10, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=10, s=Branch, brch=1, alu=Sub, cin=One, rb=10, ldrb
    seq=Jump, brch=perform_clear
perform_send:
    ra=11, rb=1, ldrb
    d=Cstk, r=Bus, rb=12, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare
perform_failed:
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=7
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=Cstk, r=Bus, rb=12, ldrb
    seq=Jump, brch=primitive_method_failed

; Sends, lookup, activation and ordinary MethodContext returns.
; This source is assembled together with interpreter.uc. All guest reads and
; writes use OBJEKT; the host neither traverses dictionaries nor builds frames.
;
; While a send is in progress: VR0/1/2 retain the caller, its method and receiver.
; VR3 holds the lookup class, VR4 the selector, VR5 the dictionary/new context,
; VR6 the send receiver, VR7 the method array/new method. SYMBOL protects one
; transferred value across a fetch that can page or collect. No sole reference
; lives in a NUMERIK register. Each new context is rooted before another fetch.
;
; r1=argument count, r2=super flag then method flag, r3=dictionary capacity,
; r4=probe index then new temporary count, r5=probe count then method header,
; r6=new context size, r7=transfer cursor. r8/r9 retain caller IP/SP until saved.
; Cache fields r10/r12/r14 can be reused after lookup; load_context restores them.
;
; Failed method lookup stops with status 6; argument mismatch with status 7.
; doesNotUnderstand:, blocks and non-local returns remain stage 4 operations.

send_literal:
    ra=13, s=Bus, d=15, alu=And, rb=4, ldrb
    ra=13, s=Branch, brch=208, alu=Sub, cin=One, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=send_selector
send_single:
    ra=13, s=Branch, brch=133, alu=Sub, cin=One, flags
    d=0, r=Bus, rb=2, ldrb
    seq=ConditionalJump, cc=!Zero, brch=single_fetch
    d=1, r=Bus, rb=2, ldrb
single_fetch:
    d=single_descriptor, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
single_descriptor:
    ra=0, s=Bus, d=31, alu=And, rb=4, ldrb
    ra=0, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    seq=Jump, brch=send_selector
send_double:
    ra=13, s=Branch, brch=134, alu=Sub, cin=One, flags
    d=0, r=Bus, rb=2, ldrb
    seq=ConditionalJump, cc=!Zero, brch=double_fetch
    d=1, r=Bus, rb=2, ldrb
double_fetch:
    d=double_arguments, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
double_arguments:
    ra=0, rb=1, ldrb
    d=double_selector, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
double_selector:
    ra=0, rb=4, ldrb
send_selector:
    ra=10, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=4, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    seq=Jump, brch=send_prepare

; The fixed guest array contains alternating selector and argument-count fields.
; Arithmetic failures arrive here with an unchanged receiver/argument stack.
send_special:
    ra=13, s=Branch, brch=176, alu=Sub, cin=One, shift=Left, rb=4, ldrb
    d=0xa000000018, page=Fetch
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=1, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=0, r=Bus, rb=2, ldrb
send_prepare:
    ; A send needs a receiver in addition to its arguments, above temporaries.
    ra=1, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=9, rb=12, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch
    d=3, idx=Load
    ra=8, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=6
    ra=2, flags
    seq=ConditionalJump, cc=!Zero, brch=super_class
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=integer_class
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=Object, ldvr, vr=3, seq=Jump, brch=lookup_class
integer_class:
    d=0xa000000006, ldvr, vr=3, seq=Jump, brch=lookup_class
super_class:
    ; Super starts above the method's defining class, not above self's class.
    ; The last literal is that class's Association, whose value is guest field 1.
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=10, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    seq=Jump, brch=superclass

; Linear probing starts at identity_hash & (capacity-1), and wraps exactly
; once. The dictionary has two fixed fields followed by a power-of-two table.
; Its parallel method Array starts at guest field zero (physical component 2).
lookup_class:
    read=Vr, vr=3
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=lookup_failed
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, seq=ConditionalJump, cc=Symbol, brch=superclass
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=3, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=0, ldrb
    ra=3, rb=0, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    ra=3, rb=5, ldrb
    read=Vr, vr=4
    d=Object, r=Bus, s=Branch, brch=32767, alu=And, ldq
    d=Q, r=Bus, s=Register, rb=0, alu=And, ldq
    d=Q, r=Bus, rb=4, ldrb
probe_selector:
    ra=4, s=Branch, brch=4, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=superclass
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=method_found
    ra=5, s=Branch, brch=1, alu=Sub, cin=One, rb=5, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=superclass
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    ra=4, rb=0, alu=And, ldq
    d=Q, r=Bus, rb=4, ldrb, seq=Jump, brch=probe_selector
superclass:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=3, seq=Jump, brch=lookup_class
method_found:
    d=3, idx=Load
    mem=Read
    d=Object, page=Fetch
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ; Converted header payload is source_header >> 1. Flag is bits 12..14.
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
    ra=2, s=Bus, d=7, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=normal_header
    ra=2, s=Branch, brch=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=extended_header
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    ra=2, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=quick_self
    ; Quick instance loads use the header's temporary-count bits as index.
    ra=5, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, s=Bus, d=31, alu=And, rb=4, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
quick_self:
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
normal_header:
    ra=2, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    seq=Jump, brch=activate
extended_header:
    ra=5, s=Bus, d=63, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ; Next-to-last literal: physical component literal_count + 1.
    ra=4, s=Branch, brch=1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=0, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, s=Bus, d=31, alu=And, rb=2, ldrb
    ra=2, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    ra=0, s=Bus, d=255, alu=And, rb=0, ldrb
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new
    ; Unimplemented or failed primitive methods execute their Smalltalk body.
    seq=Jump, brch=activate

activate:
    ra=5, s=Bus, d=63, alu=And, rb=10, ldrb
    ra=5, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, s=Bus, d=31, alu=And, rb=4, ldrb
    ra=4, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=argument_mismatch
    ra=5, s=Bus, d=64, alu=And, flags
    d=19, r=Bus, rb=6, ldrb
    seq=ConditionalJump, cc=Zero, brch=context_size
    d=39, r=Bus, rb=6, ldrb
context_size:
    ra=6, s=Branch, brch=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=0xa00000000b, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ; Initializing all pointer fields replaces the allocator's machine NIL.
    d=2, r=Bus, rb=7, ldrb
initialize_context:
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=initialize_context
    d=1, idx=Load
    ra=6, s=Branch, brch=1, alu=Sub, cin=One, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    d=2, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    idx=Increment
    ra=10, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=Add, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    read=Vr, vr=7
    d=Object, mem=Write
    d=0, r=Bus, rb=7, ldrb
copy_argument:
    ; Copy receiver first, then arguments in source order. A tagged SYMBOL
    ; bridges the two fetches. Clear each source only after the destination write.
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=7, alu=Add, rb=6, ldrb
    d=Register, ra=6, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=7, s=Branch, brch=7, alu=Add, ldq
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
    d=4, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

; Pointer-format primitive 70 (new) supplies an ordinary allocated guest object.
; Other formats, indexable classes, and arguments fail into the method body.
primitive_new:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=activate
    read=Vr, vr=6
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=0, s=Bus, d=0x4000, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=activate
    ra=0, s=Bus, d=0x1000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=activate
    ra=0, s=Bus, d=2047, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=1, alu=Add, rb=6, ldrb
    read=Vr, vr=6
    d=Object, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    ra=4, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    d=2, r=Bus, rb=7, ldrb
initialize_object:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_result
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=initialize_object

; Quick methods and successful allocation replace receiver+arguments by a result
; without an activation. In particular, no dummy MethodContext is allocated.
send_result:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, s=Branch, brch=1, alu=Add, rb=7, ldrb
clear_send_operands:
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=clear_send_operands
    d=16, r=Bus, rb=15, ldrb
    ; Restore the caller's header cache, which lookup used for the new method.
    d=4, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=load_context

return_sender:
    ; Object response still contains the sender; result is rooted in VR5.
    d=Object, ldvr, vr=7
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    read=Vr, vr=7
    d=Object, ldvr, vr=0
    d=16, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context
resume_result:
    read=Vr, vr=5
    d=Object, ldsym, seq=Jump, brch=push
lookup_failed:
    d=6, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context
argument_mismatch:
    d=7, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

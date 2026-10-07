; Runtime entry and bytecode dispatch. This file and the other .uc files in
; this directory form ONE assembly; none is an independently bootable image.
; crates/rekursiv-smalltalk/src/interpreter.rs concatenates them, supplies
; ACTIVE_CONTEXT, and reserves control addresses 8064 onward for the collector.
; See ../README.md for the source map, physical layouts, and calling conventions.
;
; Control flow: start -> load_context -> boundary -> save_context -> scheduler
; checks -> cycle -> fetch_byte -> decoded -> handler -> boundary. Sends and
; returns may replace VR0 before reentering load_context. R15=16 is an internal
; resume-result marker: reload caches, then push the result rooted in VR5.
; R15 otherwise carries a stop status here, but a failure continuation in sends.
;
; Physical MethodContext components: 1 descriptor, 2 sender, 3 IP, 4 SP,
; 5 method, 6 unused, 7 receiver, 8 onward temporaries/evaluation stack.
; BlockContext uses 2 caller, 3 IP, 4 SP, 5 arity, 6 initial IP, 7 home.
; R9 counts slots after component 7; top is component R9+7. Guest field zero
; is normally component 2. Method literal zero is component 3, and bytecode
; component = guest IP - literal count. Do not apply one offset to every kind.
;
; The map at the end covers all 256 bytes. Reserved bytes halt through
; unsupported; missing primitive implementations instead execute guest methods.
;
; Smalltalk-80 bytecode interpreter, Blue Book chapters 27-29.
; Language policy lives here; OBJEKT only sees generic tagged components.
;
; Halted boot supplies ACTIVE_CONTEXT and a converted object graph. VR0 owns
; the context, VR1 the method, VR2 the receiver, VR3 an association/variable
; target, and VR4 a store value. All retain full tags and are collector roots.
; SYMBOL and ESTKR are rooted scratch values. NUMERIK registers contain only
; integers, offsets and microaddresses; never the sole copy of a reference.
;
; Register convention:
; 0 scratch, 1 field index/jump delta, 2 variable kind, 3 store mode,
; 4/5 arithmetic operands, 6 peek continuation, 7 byte-fetch continuation,
; 8 guest IP (one-based bytes), 9 guest SP (includes temporaries),
; 10 literal count, 11 method byte length, 12 evaluation floor (method temps or zero for a block),
; 13 opcode, 14 context slot capacity, 15 terminal status.
;
; Status: 1 root return, 2 unsupported bytecode, 5 malformed state/index,
; 6 missing doesNotUnderstand:, 7 argument mismatch. No runnable process waits
; in scheduler_idle until an external event makes a process runnable.
; Arithmetic failure preserves operands and enters ordinary message lookup.
; sends.uc owns that path, context allocation, and return to a sender.
;
; Every instruction boundary writes IP/SP into the context. Popped slots are
; cleared to guest nil. The whole context is scanned by the generic collector.
; A nil sender terminates a diagnostic root invocation. Ordinary returns clear
; the finished context's sender/IP and resume its sender. blocks.uc supplies
; home/caller resolution and cannotReturn:; messages.uc supplies failed sends.
.equ NIL = 0xa000000001
.equ FALSE = 0xa000000002
.equ TRUE = 0xa000000003
.entry start
.root 26, ACTIVE_CONTEXT
.root 27, NIL ; input semaphore
.root 28, NIL ; timer semaphore
.root 29, NIL ; private device state (input, cursor/display, snapshot target, storage)
.root 30, NIL ; low-space registration Array (semaphore and threshold chunks)
.root 31, 0   ; raw idle flag, never a guest reference
start:
    d=ACTIVE_CONTEXT, ldvr, vr=0
; Rebuild all activation caches from the rooted context. Only R15=16 preserves
; VR5 for a pending result; a normal reload reaches boundary and releases it.
load_context:
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=7, alu=Sub, cin=One, rb=14, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=3, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=8, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=9, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=context_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
; VR3 is the home MethodContext. Its method and receiver supply block caches
; as well as method caches. Descriptor length is guest bytes, not RAM words.
context_home_ready:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, ldvr, vr=1
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=2
    read=Vr, vr=1
    d=Object, page=Fetch
    d=1, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    d=Object, r=Bus, shift=Right, rb=11, ldrb
    ra=11, shift=Right, rb=11, ldrb
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=0, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=0, s=Bus, d=63, alu=And, rb=10, ldrb
    ra=0, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, shift=Right, rb=12, ldrb
    ra=12, s=Bus, d=31, alu=And, rb=12, ldrb
    ; A block has its own evaluation stack, starting at zero. Its temporaries
    ; still belong to the home method and are accessed through context_home.
    read=Vr, vr=0
    d=Object, ldsym
    read=Vr, vr=3
    d=Object, seq=ConditionalJump, cc=Symbol, brch=context_stack_floor
    d=0, r=Bus, rb=12, ldrb
context_stack_floor:
    ra=9, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=14, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=15, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=resume_result
    seq=Jump, brch=boundary

; IP/SP materialization is also used on terminal paths. The writes use compact
; SmallIntegers. Descriptor word one remains raw and is never a guest field.
boundary:
    ; Release completed-op scratch roots after their values reach guest fields.
    d=0, ldvr, vr=3
    d=0, ldvr, vr=4
    d=0, ldvr, vr=5
    d=0, ldvr, vr=6
    d=0, ldvr, vr=7
    d=0, r=Bus, rb=15, ldrb
save_context:
    read=Vr, vr=0
    d=Object, page=Fetch
    d=3, idx=Load
    ra=8, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=stopped
    seq=Jump, brch=check_process_switch
; Tests observe this boundary without changing machine state or supplying work.
cycle:
    d=decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
decoded:
    ra=0, rb=13, ldrb
    d=Register, ra=13, apc=Bus
    fetch=Nam
    fetch=Map
    seq=Dispatch

; Return byte in r0. Conversion places each byte in a raw component. The
; physical component is guest IP - literal count; guest offsets stay unchanged.
fetch_byte:
    ra=10, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=Add, rb=0, ldrb
    ra=8, rb=0, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=11, rb=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=8, rb=10, alu=Sub, cin=One, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=Register, ra=7, seq=Bus

; These four push families share generic field access. Kind 3 dereferences
; the literal Association's value (guest field 1).
push_receiver_field:
    ra=13, s=Bus, d=15, alu=And, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    seq=Jump, brch=push_variable
push_temporary:
    ra=13, s=Bus, d=15, alu=And, rb=1, ldrb
    d=1, r=Bus, rb=2, ldrb
    seq=Jump, brch=push_variable
push_literal:
    ra=13, s=Bus, d=31, alu=And, rb=1, ldrb
    d=2, r=Bus, rb=2, ldrb
    seq=Jump, brch=push_variable
push_association:
    ra=13, s=Bus, d=31, alu=And, rb=1, ldrb
    d=3, r=Bus, rb=2, ldrb
push_variable:
    d=0, r=Bus, rb=3, ldrb
    seq=Jump, brch=variable
store_receiver:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=2, r=Bus, rb=3, ldrb
    seq=Jump, brch=variable
store_temporary:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    d=1, r=Bus, rb=2, ldrb
    d=2, r=Bus, rb=3, ldrb
    seq=Jump, brch=variable
; Opcodes 128/129/130 become mode 0 push / 1 store / 2 store-and-pop.
; The extension byte supplies kind in bits 6..7 and zero-based index in 0..5.
extended:
    ra=13, s=Branch, brch=128, alu=Sub, cin=One, rb=3, ldrb
    d=extended_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
extended_decoded:
    ra=0, s=Bus, d=63, alu=And, rb=1, ldrb
    ra=0, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
; R2 kind: 0 receiver, 1 home temporary, 2 literal, 3 literal Association.
; Resolve and root a writable target in VR3; literals themselves are read-only.
variable:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=receiver_target
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=temporary_target
    ra=10, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=1, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=association_target
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    d=Object, ldsym, seq=Jump, brch=push
association_target:
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    seq=Jump, brch=access_variable
receiver_target:
    read=Vr, vr=2
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=1, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    seq=Jump, brch=access_variable
temporary_target:
    d=temporary_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
temporary_home_ready:
    ; Read the home method's temporary count. R12 is the evaluation-stack floor,
    ; which is zero inside a block and therefore cannot validate this access.
    read=Vr, vr=1
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, shift=Right, rb=0, ldrb
    ra=0, s=Branch, brch=31, alu=And, rb=0, ldrb
    ra=0, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=1, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
access_variable:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=write_variable
    mem=Read
    d=Object, ldsym, seq=Jump, brch=push
; peek selects the caller, so preserve the target index in R1 first. VR3
; keeps the target alive and VR4 keeps the value alive while refetching it.
write_variable:
    read=Index
    d=Object, r=Bus, rb=1, ldrb
    d=write_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
write_value:
    d=Symbol, ldvr, vr=4
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=1, idx=Load
    read=Vr, vr=4
    d=Object, mem=Write
    ra=3, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=pop
    seq=Jump, brch=boundary

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
    ra=9, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=Register, ra=6, seq=Bus
push:
    ra=14, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    seq=Jump, brch=boundary
pop:
    d=pop_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
pop_value:
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=boundary

; Keep the result rooted in VR5 before reading or refilling the sender.
; A nil sender terminates the diagnostic root; sends.uc handles other returns.
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
; for bytecode 125. Component 2 supplies sender/caller; validate before unlinking.
return_target:
    d=2, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_sender
    ; Only the boot activation has a diagnostic nil-sender return. Escaped
    ; blocks and manipulated sender chains send cannotReturn: on the machine.
    read=Vr, vr=0
    d=ACTIVE_CONTEXT, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=cannot_return
    d=1, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

; Short offsets are (opcode & 7)+1. Long unconditional offsets are
; ((opcode & 7)-4)*256+extension, allowing backward branches. Conditional long
; offsets are (opcode & 3)*256+extension; all are relative to advanced IP.
short_jump:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=1, alu=Add, rb=1, ldrb
    seq=Jump, brch=apply_jump
short_conditional:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=1, alu=Add, rb=1, ldrb
    seq=Jump, brch=conditional
long_jump:
    d=long_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
long_decoded:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=4, alu=Sub, cin=One, rb=1, ldrb
    seq=Jump, brch=scale_offset
long_conditional:
    d=long_cond_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
long_cond_decoded:
    ra=13, s=Bus, d=3, alu=And, rb=1, ldrb
scale_offset:
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, shift=Left, rb=1, ldrb
    ra=1, rb=0, alu=Add, ldq
    d=Q, r=Bus, rb=1, ldrb
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=apply_jump
; peek leaves selected caller/top slot intact. Only full TRUE/FALSE identities
; are accepted. A non-Boolean sends mustBeBoolean (identity 26) without popping.
conditional:
    d=condition_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
condition_value:
    d=TRUE, seq=ConditionalJump, cc=Symbol, brch=condition_true
    d=FALSE, seq=ConditionalJump, cc=!Symbol, brch=boolean_failure
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=take_conditional
    ra=13, s=Branch, brch=172, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=take_conditional
    seq=Jump, brch=skip_conditional
condition_true:
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=skip_conditional
    ra=13, s=Branch, brch=172, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=take_conditional
skip_conditional:
    d=0, r=Bus, rb=1, ldrb
take_conditional:
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
apply_jump:
    ra=8, rb=1, alu=Add, ldq
    d=Q, r=Bus, rb=8, ldrb
    seq=Jump, brch=boundary

; Arithmetic fast paths never discard arguments until type/range checks pass.
; Reconstructing and comparing the complete compact word checks the tag, not
; just its payload. A reference with a small identity cannot pass this check.
arithmetic:
    ra=9, rb=12, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=arithmetic_argument, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
arithmetic_argument:
    d=Symbol, r=Bus, rb=5, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failure
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failure
    ra=13, flags
    seq=ConditionalJump, cc=Zero, brch=integer_extended
    ra=13, s=Branch, brch=176, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=add
    ra=13, s=Branch, brch=177, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=subtract
    ra=13, s=Branch, brch=184, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=compare
    seq=ConditionalJump, cc=Zero, brch=multiply
    ra=13, s=Branch, brch=190, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bit_and
    seq=Jump, brch=bit_or
add:
    ra=4, rb=5, alu=Add, ldq
    seq=Jump, brch=integer_result
subtract:
    ra=4, rb=5, alu=Sub, cin=One, ldq
    seq=Jump, brch=integer_result
multiply:
    ra=4, rb=5, alu=MultiplySigned
    alu=ProductLow, ldq
    seq=Jump, brch=integer_result
bit_and:
    ra=4, rb=5, alu=And, ldq
    seq=Jump, brch=integer_result
bit_or:
    ra=4, rb=5, alu=Or, ldq
integer_result:
    d=Q, r=Bus, rb=4, ldrb
    ra=4, s=Bus, d=0xffffc000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failure
    ra=4, s=Bus, d=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failure
    ra=4, estk=Compact, compact=2
    d=Estk, ldsym, seq=Jump, brch=arithmetic_result
compare:
    ra=13, s=Branch, brch=178, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=less
    ra=13, s=Branch, brch=179, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=greater
    ra=13, s=Branch, brch=180, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=less_equal
    ra=13, s=Branch, brch=181, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=greater_equal
    ra=13, s=Branch, brch=182, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=equal
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=true_result
    seq=Jump, brch=false_result
less:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
greater:
    ra=4, rb=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
less_equal:
    ra=4, rb=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
greater_equal:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
equal:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=true_result
false_result:
    d=FALSE, ldsym, seq=Jump, brch=arithmetic_result
true_result:
    d=TRUE, ldsym
; With R15=0 this is a bytecode fast path: arithmetic left IDX on receiver,
; so replace it and clear the argument. Otherwise use the general primitive
; result path to refetch caller state and consume exactly R1 arguments.
arithmetic_result:
    ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_arithmetic_result
    d=Symbol, mem=Write
    idx=Increment
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=boundary
unsupported:
    d=2, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context
primitive_arithmetic_result:
    d=Symbol, ldvr, vr=5, seq=Jump, brch=send_result
primitive_failure:
    ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=send_special
boolean_failure:
    d=0xa00000001a, ldvr, vr=4
    d=0, r=Bus, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare
bad_state:
    d=5, r=Bus, rb=15, ldrb
    ; Malformed boot state may not be safe to write back to a guest context.
    halt
stopped:
    halt

; The NAM contains a fixed identity map of byte values, not guest instructions.
; OBJEKT supplies each byte. These tables only select microcode entry points.
.nam 0, 0, 0
.map 0, push_receiver_field
.nam 1, 1, 0
.map 1, push_receiver_field
.nam 2, 2, 0
.map 2, push_receiver_field
.nam 3, 3, 0
.map 3, push_receiver_field
.nam 4, 4, 0
.map 4, push_receiver_field
.nam 5, 5, 0
.map 5, push_receiver_field
.nam 6, 6, 0
.map 6, push_receiver_field
.nam 7, 7, 0
.map 7, push_receiver_field
.nam 8, 8, 0
.map 8, push_receiver_field
.nam 9, 9, 0
.map 9, push_receiver_field
.nam 10, 10, 0
.map 10, push_receiver_field
.nam 11, 11, 0
.map 11, push_receiver_field
.nam 12, 12, 0
.map 12, push_receiver_field
.nam 13, 13, 0
.map 13, push_receiver_field
.nam 14, 14, 0
.map 14, push_receiver_field
.nam 15, 15, 0
.map 15, push_receiver_field
.nam 16, 16, 0
.map 16, push_temporary
.nam 17, 17, 0
.map 17, push_temporary
.nam 18, 18, 0
.map 18, push_temporary
.nam 19, 19, 0
.map 19, push_temporary
.nam 20, 20, 0
.map 20, push_temporary
.nam 21, 21, 0
.map 21, push_temporary
.nam 22, 22, 0
.map 22, push_temporary
.nam 23, 23, 0
.map 23, push_temporary
.nam 24, 24, 0
.map 24, push_temporary
.nam 25, 25, 0
.map 25, push_temporary
.nam 26, 26, 0
.map 26, push_temporary
.nam 27, 27, 0
.map 27, push_temporary
.nam 28, 28, 0
.map 28, push_temporary
.nam 29, 29, 0
.map 29, push_temporary
.nam 30, 30, 0
.map 30, push_temporary
.nam 31, 31, 0
.map 31, push_temporary
.nam 32, 32, 0
.map 32, push_literal
.nam 33, 33, 0
.map 33, push_literal
.nam 34, 34, 0
.map 34, push_literal
.nam 35, 35, 0
.map 35, push_literal
.nam 36, 36, 0
.map 36, push_literal
.nam 37, 37, 0
.map 37, push_literal
.nam 38, 38, 0
.map 38, push_literal
.nam 39, 39, 0
.map 39, push_literal
.nam 40, 40, 0
.map 40, push_literal
.nam 41, 41, 0
.map 41, push_literal
.nam 42, 42, 0
.map 42, push_literal
.nam 43, 43, 0
.map 43, push_literal
.nam 44, 44, 0
.map 44, push_literal
.nam 45, 45, 0
.map 45, push_literal
.nam 46, 46, 0
.map 46, push_literal
.nam 47, 47, 0
.map 47, push_literal
.nam 48, 48, 0
.map 48, push_literal
.nam 49, 49, 0
.map 49, push_literal
.nam 50, 50, 0
.map 50, push_literal
.nam 51, 51, 0
.map 51, push_literal
.nam 52, 52, 0
.map 52, push_literal
.nam 53, 53, 0
.map 53, push_literal
.nam 54, 54, 0
.map 54, push_literal
.nam 55, 55, 0
.map 55, push_literal
.nam 56, 56, 0
.map 56, push_literal
.nam 57, 57, 0
.map 57, push_literal
.nam 58, 58, 0
.map 58, push_literal
.nam 59, 59, 0
.map 59, push_literal
.nam 60, 60, 0
.map 60, push_literal
.nam 61, 61, 0
.map 61, push_literal
.nam 62, 62, 0
.map 62, push_literal
.nam 63, 63, 0
.map 63, push_literal
.nam 64, 64, 0
.map 64, push_association
.nam 65, 65, 0
.map 65, push_association
.nam 66, 66, 0
.map 66, push_association
.nam 67, 67, 0
.map 67, push_association
.nam 68, 68, 0
.map 68, push_association
.nam 69, 69, 0
.map 69, push_association
.nam 70, 70, 0
.map 70, push_association
.nam 71, 71, 0
.map 71, push_association
.nam 72, 72, 0
.map 72, push_association
.nam 73, 73, 0
.map 73, push_association
.nam 74, 74, 0
.map 74, push_association
.nam 75, 75, 0
.map 75, push_association
.nam 76, 76, 0
.map 76, push_association
.nam 77, 77, 0
.map 77, push_association
.nam 78, 78, 0
.map 78, push_association
.nam 79, 79, 0
.map 79, push_association
.nam 80, 80, 0
.map 80, push_association
.nam 81, 81, 0
.map 81, push_association
.nam 82, 82, 0
.map 82, push_association
.nam 83, 83, 0
.map 83, push_association
.nam 84, 84, 0
.map 84, push_association
.nam 85, 85, 0
.map 85, push_association
.nam 86, 86, 0
.map 86, push_association
.nam 87, 87, 0
.map 87, push_association
.nam 88, 88, 0
.map 88, push_association
.nam 89, 89, 0
.map 89, push_association
.nam 90, 90, 0
.map 90, push_association
.nam 91, 91, 0
.map 91, push_association
.nam 92, 92, 0
.map 92, push_association
.nam 93, 93, 0
.map 93, push_association
.nam 94, 94, 0
.map 94, push_association
.nam 95, 95, 0
.map 95, push_association
.nam 96, 96, 0
.map 96, store_receiver
.nam 97, 97, 0
.map 97, store_receiver
.nam 98, 98, 0
.map 98, store_receiver
.nam 99, 99, 0
.map 99, store_receiver
.nam 100, 100, 0
.map 100, store_receiver
.nam 101, 101, 0
.map 101, store_receiver
.nam 102, 102, 0
.map 102, store_receiver
.nam 103, 103, 0
.map 103, store_receiver
.nam 104, 104, 0
.map 104, store_temporary
.nam 105, 105, 0
.map 105, store_temporary
.nam 106, 106, 0
.map 106, store_temporary
.nam 107, 107, 0
.map 107, store_temporary
.nam 108, 108, 0
.map 108, store_temporary
.nam 109, 109, 0
.map 109, store_temporary
.nam 110, 110, 0
.map 110, store_temporary
.nam 111, 111, 0
.map 111, store_temporary
.nam 112, 112, 0
.map 112, push_receiver
.nam 113, 113, 0
.map 113, push_true
.nam 114, 114, 0
.map 114, push_false
.nam 115, 115, 0
.map 115, push_nil
.nam 116, 116, 0
.map 116, push_integer
.nam 117, 117, 0
.map 117, push_integer
.nam 118, 118, 0
.map 118, push_integer
.nam 119, 119, 0
.map 119, push_integer
.nam 120, 120, 0
.map 120, return_receiver
.nam 121, 121, 0
.map 121, return_true
.nam 122, 122, 0
.map 122, return_false
.nam 123, 123, 0
.map 123, return_nil
.nam 124, 124, 0
.map 124, return_top
.nam 125, 125, 0
.map 125, return_block
.nam 126, 126, 0
.map 126, unsupported
.nam 127, 127, 0
.map 127, unsupported
.nam 128, 128, 0
.map 128, extended
.nam 129, 129, 0
.map 129, extended
.nam 130, 130, 0
.map 130, extended
.nam 131, 131, 0
.map 131, send_single
.nam 132, 132, 0
.map 132, send_double
.nam 133, 133, 0
.map 133, send_single
.nam 134, 134, 0
.map 134, send_double
.nam 135, 135, 0
.map 135, pop
.nam 136, 136, 0
.map 136, duplicate
.nam 137, 137, 0
.map 137, push_context
.nam 138, 138, 0
.map 138, unsupported
.nam 139, 139, 0
.map 139, unsupported
.nam 140, 140, 0
.map 140, unsupported
.nam 141, 141, 0
.map 141, unsupported
.nam 142, 142, 0
.map 142, unsupported
.nam 143, 143, 0
.map 143, unsupported
.nam 144, 144, 0
.map 144, short_jump
.nam 145, 145, 0
.map 145, short_jump
.nam 146, 146, 0
.map 146, short_jump
.nam 147, 147, 0
.map 147, short_jump
.nam 148, 148, 0
.map 148, short_jump
.nam 149, 149, 0
.map 149, short_jump
.nam 150, 150, 0
.map 150, short_jump
.nam 151, 151, 0
.map 151, short_jump
.nam 152, 152, 0
.map 152, short_conditional
.nam 153, 153, 0
.map 153, short_conditional
.nam 154, 154, 0
.map 154, short_conditional
.nam 155, 155, 0
.map 155, short_conditional
.nam 156, 156, 0
.map 156, short_conditional
.nam 157, 157, 0
.map 157, short_conditional
.nam 158, 158, 0
.map 158, short_conditional
.nam 159, 159, 0
.map 159, short_conditional
.nam 160, 160, 0
.map 160, long_jump
.nam 161, 161, 0
.map 161, long_jump
.nam 162, 162, 0
.map 162, long_jump
.nam 163, 163, 0
.map 163, long_jump
.nam 164, 164, 0
.map 164, long_jump
.nam 165, 165, 0
.map 165, long_jump
.nam 166, 166, 0
.map 166, long_jump
.nam 167, 167, 0
.map 167, long_jump
.nam 168, 168, 0
.map 168, long_conditional
.nam 169, 169, 0
.map 169, long_conditional
.nam 170, 170, 0
.map 170, long_conditional
.nam 171, 171, 0
.map 171, long_conditional
.nam 172, 172, 0
.map 172, long_conditional
.nam 173, 173, 0
.map 173, long_conditional
.nam 174, 174, 0
.map 174, long_conditional
.nam 175, 175, 0
.map 175, long_conditional
.nam 176, 176, 0
.map 176, arithmetic
.nam 177, 177, 0
.map 177, arithmetic
.nam 178, 178, 0
.map 178, arithmetic
.nam 179, 179, 0
.map 179, arithmetic
.nam 180, 180, 0
.map 180, arithmetic
.nam 181, 181, 0
.map 181, arithmetic
.nam 182, 182, 0
.map 182, arithmetic
.nam 183, 183, 0
.map 183, arithmetic
.nam 184, 184, 0
.map 184, arithmetic
.nam 185, 185, 0
.map 185, send_special
.nam 186, 186, 0
.map 186, send_special
.nam 187, 187, 0
.map 187, send_special
.nam 188, 188, 0
.map 188, send_special
.nam 189, 189, 0
.map 189, send_special
.nam 190, 190, 0
.map 190, arithmetic
.nam 191, 191, 0
.map 191, arithmetic
.nam 192, 192, 0
.map 192, send_special
.nam 193, 193, 0
.map 193, send_special
.nam 194, 194, 0
.map 194, send_special
.nam 195, 195, 0
.map 195, send_special
.nam 196, 196, 0
.map 196, send_special
.nam 197, 197, 0
.map 197, send_special
.nam 198, 198, 0
.map 198, send_special
.nam 199, 199, 0
.map 199, send_special
.nam 200, 200, 0
.map 200, send_special
.nam 201, 201, 0
.map 201, send_special
.nam 202, 202, 0
.map 202, send_special
.nam 203, 203, 0
.map 203, send_special
.nam 204, 204, 0
.map 204, send_special
.nam 205, 205, 0
.map 205, send_special
.nam 206, 206, 0
.map 206, send_special
.nam 207, 207, 0
.map 207, send_special
.nam 208, 208, 0
.map 208, send_literal
.nam 209, 209, 0
.map 209, send_literal
.nam 210, 210, 0
.map 210, send_literal
.nam 211, 211, 0
.map 211, send_literal
.nam 212, 212, 0
.map 212, send_literal
.nam 213, 213, 0
.map 213, send_literal
.nam 214, 214, 0
.map 214, send_literal
.nam 215, 215, 0
.map 215, send_literal
.nam 216, 216, 0
.map 216, send_literal
.nam 217, 217, 0
.map 217, send_literal
.nam 218, 218, 0
.map 218, send_literal
.nam 219, 219, 0
.map 219, send_literal
.nam 220, 220, 0
.map 220, send_literal
.nam 221, 221, 0
.map 221, send_literal
.nam 222, 222, 0
.map 222, send_literal
.nam 223, 223, 0
.map 223, send_literal
.nam 224, 224, 0
.map 224, send_literal
.nam 225, 225, 0
.map 225, send_literal
.nam 226, 226, 0
.map 226, send_literal
.nam 227, 227, 0
.map 227, send_literal
.nam 228, 228, 0
.map 228, send_literal
.nam 229, 229, 0
.map 229, send_literal
.nam 230, 230, 0
.map 230, send_literal
.nam 231, 231, 0
.map 231, send_literal
.nam 232, 232, 0
.map 232, send_literal
.nam 233, 233, 0
.map 233, send_literal
.nam 234, 234, 0
.map 234, send_literal
.nam 235, 235, 0
.map 235, send_literal
.nam 236, 236, 0
.map 236, send_literal
.nam 237, 237, 0
.map 237, send_literal
.nam 238, 238, 0
.map 238, send_literal
.nam 239, 239, 0
.map 239, send_literal
.nam 240, 240, 0
.map 240, send_literal
.nam 241, 241, 0
.map 241, send_literal
.nam 242, 242, 0
.map 242, send_literal
.nam 243, 243, 0
.map 243, send_literal
.nam 244, 244, 0
.map 244, send_literal
.nam 245, 245, 0
.map 245, send_literal
.nam 246, 246, 0
.map 246, send_literal
.nam 247, 247, 0
.map 247, send_literal
.nam 248, 248, 0
.map 248, send_literal
.nam 249, 249, 0
.map 249, send_literal
.nam 250, 250, 0
.map 250, send_literal
.nam 251, 251, 0
.map 251, send_literal
.nam 252, 252, 0
.map 252, send_literal
.nam 253, 253, 0
.map 253, send_literal
.nam 254, 254, 0
.map 254, send_literal
.nam 255, 255, 0
.map 255, send_literal

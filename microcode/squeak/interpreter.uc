 ; Squeak 1.1 bytecode execution. Registers and roots follow the Xerox engine,
; but this profile owns its layout and semantics. All fields use OBJEKT.
; Physical components: descriptor 1, hash/format 2, sender 3, IP 4, SP 5,
; method/arity 6, closure/initialIP 7, receiver/home 8, temporaries from 9.
; R8=guest IP, R9=SP, R10=literal count, R11=method byte length,
; R12=temporary floor, R13=opcode, R14=frame capacity, R15=continuation/status.
; VR0=context, VR1=method, VR2=receiver; VR3..7 are rooted scratch.
; boundary materializes IP/SP and releases scratch. Popped slots become guest nil.
; Stops: 1 root return, 2 reserved bytecode, 5 malformed state, 6 missing DNU,
; 7 argument mismatch, 8 unimplemented required primitive, 9 no runnable process.
; No host code interprets bytecodes or supplies language primitive results.
.entry start
.root 3, NIL
.root 10, NIL
.root 12, 0xffffffff
.root 26, ACTIVE_CONTEXT
.root 27, NIL
.root 28, NIL
.root 29, NIL
.root 30, NIL
.root 31, 0
start:
    d=ACTIVE_CONTEXT, ldvr, vr=0
load_context:
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=8, alu=Sub, cin=One, rb=14, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=8, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=9, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=context_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
context_home_ready:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=6, idx=Load
    mem=Read
    d=Object, ldvr, vr=1
    d=8, idx=Load
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
    d=3, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=0, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=0, s=Branch, brch=23, alu=Rotate, rb=10, ldrb
    ra=10, s=Bus, d=255, alu=And, rb=10, ldrb
    ra=0, s=Branch, brch=14, alu=Rotate, rb=12, ldrb
    ra=12, s=Bus, d=63, alu=And, rb=12, ldrb
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

boundary:
    d=0, ldvr, vr=3
    d=0, ldvr, vr=4
    d=0, ldvr, vr=5
    d=0, ldvr, vr=6
    d=0, ldvr, vr=7, r=Bus, rb=15, ldrb
save_context:
    read=Vr, vr=0
    d=Object, page=Fetch
    d=4, idx=Load, ra=8, estk=Compact, compact=2
    d=Estk, mem=Write, idx=Increment, ra=9, estk=Compact, compact=2
    d=Estk, mem=Write, ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=stopped
    seq=Jump, brch=check_process_switch
cycle:
    d=decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
decoded:
    d=Register, ra=0, rb=13, ldrb, apc=Bus
    fetch=Nam
    fetch=Map
    seq=Dispatch

fetch_byte:
    ; Guest IP counts four bytes per header/literal; physical fields count one.
    ; First byte is 4*(literals+1)+1; its component is literals+4.
    ra=10, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=5, alu=Add, rb=0, ldrb
    ra=8, rb=0, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=11, rb=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=10, shift=Left, rb=0, ldrb
    ra=10, rb=0, alu=Add, ldrb
    ra=0, s=Branch, brch=1, alu=Add, rb=0, ldrb
    read=Vr, vr=1
    d=Object, page=Fetch, ra=8, rb=0, alu=Sub, cin=One, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    d=Register, ra=7, r=Branch, brch=0, s=Register, rb=8, alu=Add, cin=One, ldrb, seq=Bus

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
    ra=1, s=Branch, brch=4, alu=Add, ldq
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
    d=4, idx=Load
    seq=Jump, brch=access_variable
receiver_target:
    read=Vr, vr=2
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=1, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    seq=Jump, brch=access_variable
temporary_target:
    d=temporary_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
temporary_home_ready:
    read=Vr, vr=1
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=0, s=Branch, brch=14, alu=Rotate, rb=0, ldrb
    ra=0, s=Branch, brch=63, alu=And, rb=0, ldrb
    ra=0, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=1, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
access_variable:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=write_variable
    mem=Read
    d=Object, ldsym, seq=Jump, brch=push
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

peek:
    ra=9, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch, ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=Register, ra=6, seq=Bus
push:
    ra=14, rb=9, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb, read=Vr, vr=0
    d=Object, page=Fetch, ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write, seq=Jump, brch=boundary
pop:
    d=pop_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
pop_value:
    d=NIL, mem=Write, ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=boundary

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
return_target:
    d=3, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=return_sender
    read=Vr, vr=0
    d=ACTIVE_CONTEXT, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=cannot_return
    d=1, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

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
    alu=ProductHigh, rb=0, ldrb
    alu=ProductLow, rb=4, ldrb
    ra=4, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=5, shift=ArithmeticRight, rb=5, ldrb
    ra=0, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failure
    ra=4, ldq
    seq=Jump, brch=integer_result
bit_and:
    ra=4, rb=5, alu=And, ldq
    seq=Jump, brch=integer_result
bit_or:
    ra=4, rb=5, alu=Or, ldq
integer_result:
    d=Q, r=Bus, rb=4, ldrb
    ra=4, s=Bus, d=0xc0000000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=primitive_failure
    ra=4, s=Bus, d=1073741823, alu=SubReverse, cin=One, flags
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
    d=SPECIAL_25, ldvr, vr=4
    d=0, r=Bus, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare
bad_state:
    d=5, r=Bus, rb=15, ldrb
    halt
stopped:
    halt

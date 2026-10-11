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
.equ FIELD_BASE = 3
.equ TEMP_ROTATE = 14
.equ TEMP_MASK = 63
.equ CTX_STACK = 8
.equ CTX_IP = 4
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
    d=4, idx=Load, read=Size
    d=Object, r=Bus, s=Branch, brch=8, alu=Sub, cin=One, rb=14, ldrb, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    mem=Read
    d=Object, ldsym, r=Bus, rb=8, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=9, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=context_home_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=context_home
context_home_ready:
    d=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    d=2, mem=Read, idx=Step
    d=Object, ldvr, vr=1
    mem=Read
    d=Object, ldvr, vr=2
    d=1, idx=Load, read=Vr, vr=1
    d=Object, page=Fetch
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

fetch_byte_end:

multiply:
    ra=4, rb=5, alu=MultiplySigned
    alu=ProductHigh, rb=0, ldrb
    alu=ProductLow, rb=4, ldrb
    ; ProductHigh must equal the sign extension of ProductLow.
    ra=4, flags
    d=0, r=Bus, rb=5, ldrb, seq=ConditionalJump, cc=!Sign, brch=multiply_high
    d=0xffffffff, r=Bus, rb=5, ldrb
multiply_high:
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

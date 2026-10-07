; Shared context resolution, block primitives, and invalid-return sends.
; primitive_block_copy implements 80 (one arity argument) and special bytecode
; 200. primitive_value implements 81 and special 201/202 (arity comes from the
; BlockContext). primitive_value_array implements 82 (one argument Array).
;
; A block owns caller/IP/SP/argument slots but shares its home MethodContext's
; method, receiver and temporaries. blockCopy allocates the home's physical size
; and starts at caller IP+2 to skip the compiler's long jump around the body.
; value reuses the existing block, clears old slots, copies arguments, restores
; initial IP, links caller, and enters load_context with VR0 set to the block.
; The caller operands are consumed only after copying succeeds. valueWithArguments:
; uses R2=1 and VR3=Array; ordinary value uses R2=0 and reads caller stack slots.
;
; context_home takes VR0 and returns VR3 via R6; it changes selection, Object,
; SYMBOL and IDX but preserves arithmetic registers. It validates context classes;
; malformed state stops through bad_state rather than ordinary primitive fallback.
; cannot_return takes intended result VR5, appends active context plus result to
; the guest stack, and sends fixed selector identity 22 via send_prepare. Room
; for both entries is required. Method/nonlocal return target selection begins
; in interpreter.uc, and sends.uc resumes a valid sender.
;
; Blue Book contexts and block primitives. Guest blocks retain their home
; MethodContext; they own a separate caller/IP/SP/evaluation stack. The home
; supplies method, receiver, and temporary variables. All references remain in
; VRs or tagged object fields, including values crossing an allocating fetch.
;
; context_home: input VR0, output VR3, continuation R6. Preserves numeric state.
context_home:
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Type
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=context_block_home
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    read=Vr, vr=0
    d=Object, ldvr, vr=3
    d=Register, ra=6, seq=Bus
context_block_home:
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size, class=0xa00000000b
    d=Register, ra=6, seq=Bus

; Primitive 80 / special bytecode 200. Validate before touching guest operands.
; R4 holds block arity, R6 body size, R7 initialization cursor. VR3 roots home,
; VR5 roots the new block. R15 is the failure continuation selected by sends.uc.
primitive_block_copy:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=block_copy_home
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=3, seq=Jump, brch=block_copy_argument
block_copy_home:
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
block_copy_argument:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size, class=0xa00000000b
    d=Object, r=Bus, rb=6, ldrb
    ra=6, s=Branch, brch=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=0xa00000000c, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    d=2, r=Bus, rb=7, ldrb
block_initialize:
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=block_initialize
    d=1, idx=Load
    ra=6, s=Branch, brch=1, alu=Sub, cin=One, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    ; IP already points past blockCopy:. Skip the compiler's two-byte jump.
    d=3, idx=Load
    ra=8, s=Branch, brch=2, alu=Add, estk=Compact, compact=2
    d=Estk, mem=Write
    d=6, idx=Load
    d=Estk, mem=Write
    d=4, idx=Load
    d=0xc200000000, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    d=7, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
    seq=Jump, brch=send_result

; Primitive 81 / special bytecodes 201 and 202. The existing BlockContext is
; activated, not copied. Argument count and capacity checks precede mutation.
primitive_value:
    d=0, r=Bus, rb=2, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
block_value_capacity:
    read=Size
    d=Object, r=Bus, rb=6, ldrb
    ra=6, s=Branch, brch=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=6, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    ; Clear slots from an earlier activation so they cannot retain dead objects.
    d=8, r=Bus, rb=7, ldrb
block_clear_slots:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=block_copy_arguments_start
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=block_clear_slots
block_copy_arguments_start:
    d=0, r=Bus, rb=7, ldrb
block_copy_arguments:
    ra=4, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=block_activate
    ra=2, flags
    seq=ConditionalJump, cc=!Zero, brch=block_array_argument
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    seq=Jump, brch=block_store_argument
block_array_argument:
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=7, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
block_store_argument:
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=7, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=block_copy_arguments
; Reset block IP from initial IP, set SP to copied argument count, and link
; the current caller. Only then clear caller operands and replace active VR0.
block_activate:
    read=Vr, vr=5
    d=Object, page=Fetch
    d=6, idx=Load
    mem=Read
    d=Object, ldsym
    d=3, idx=Load
    d=Symbol, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    d=2, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    ; Consume the caller operands only after every argument reached the block.
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, s=Branch, brch=1, alu=Add, rb=7, ldrb
block_clear_caller:
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=block_clear_caller
    d=4, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

; Invalid return is an ordinary message to the active context. The result is
; already rooted in VR5. Both new stack entries are guest-visible before lookup.
cannot_return:
    ra=14, rb=9, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    idx=Increment
    read=Vr, vr=5
    d=Object, mem=Write
    ra=9, s=Branch, brch=2, alu=Add, rb=9, ldrb
    d=0xa000000016, ldvr, vr=4
    d=1, r=Bus, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    ; This is an explicit send, regardless of the bytecode that failed return.
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare

; Primitive 82 keeps the argument Array intact; unlike ordinary value sends,
; only the array reference occupies an argument slot in the caller.
primitive_value_array:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=0xa00000000c, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
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
    d=Object, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, rb=0, ldrb
    ra=0, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=1, r=Bus, rb=2, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    seq=Jump, brch=block_value_capacity

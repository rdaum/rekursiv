 ; Squeak message lookup and activation. VR4 selector, VR6 receiver, VR7 method.
; R1 arity; R2 super flag; R15 primitive failure continuation. All references
; remain tagged roots across faults and allocation. Dictionary probing uses the
; saved 12-bit hash in component 2, never the OBJEKT identity.
; Bytecodes 132 and 134 follow the original Squeak Interpreter definitions.
; send_result consumes receiver/arguments only after success, then restores caches.
; Unknown primitives stop before mutation, exposing R0 and VR7 for diagnostics.

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
send_second:
    d=second_descriptor, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
second_descriptor:
    ra=0, s=Branch, brch=63, alu=And, rb=4, ldrb
    ra=0, s=Branch, brch=26, alu=Rotate, rb=1, ldrb
    ra=1, s=Branch, brch=3, alu=And, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=send_selector
; Bytecode 132: high three bits of first operand select eight operations.
send_double:
    d=double_descriptor, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
double_descriptor:
    ra=0, s=Branch, brch=27, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=7, alu=And, rb=2, ldrb
    ra=0, s=Branch, brch=31, alu=And, rb=1, ldrb
    d=double_operand, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
double_operand:
    ra=0, rb=4, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_selector
    ra=0, rb=1, ldrb
    d=0, r=Bus, rb=3, ldrb
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=double_literal
    ra=2, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=double_association
    ra=2, s=Branch, brch=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=double_store_association
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=double_receiver
    ra=2, s=Branch, brch=4, alu=Sub, cin=One, rb=3, ldrb
double_receiver:
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=variable
double_literal:
    d=2, r=Bus, rb=2, ldrb, seq=Jump, brch=variable
double_store_association:
    d=1, r=Bus, rb=3, ldrb
double_association:
    d=3, r=Bus, rb=2, ldrb, seq=Jump, brch=variable
send_selector:
    ra=10, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=4, s=Branch, brch=4, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    seq=Jump, brch=send_prepare

send_special:
    ra=13, s=Branch, brch=176, alu=Sub, cin=One, shift=Left, rb=4, ldrb
    d=SPECIAL_23, page=Fetch
    ra=4, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=1, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=0, r=Bus, rb=2, ldrb
send_prepare:
    ra=1, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=9, rb=12, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch
    d=4, idx=Load, ra=8, estk=Compact, compact=2
    d=Estk, mem=Write, idx=Increment, ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=6
    d=special_lookup, r=Bus, rb=15, ldrb
    ra=13, s=Branch, brch=198, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_identity
    ra=13, s=Branch, brch=199, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_class
    ra=13, s=Branch, brch=200, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_block_copy
    ra=13, s=Branch, brch=201, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value
    ra=13, s=Branch, brch=202, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value
    seq=Jump, brch=lookup_receiver_class
special_lookup:
    d=0, r=Bus, rb=2, ldrb
lookup_receiver_class:
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
    d=SPECIAL_5, ldvr, vr=3, seq=Jump, brch=lookup_class
super_class:
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch
    ra=10, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    seq=Jump, brch=superclass

lookup_class:
    read=Vr, vr=3
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=lookup_failed
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, seq=ConditionalJump, cc=Symbol, brch=superclass
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=4, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=0, ldrb
    ra=3, rb=0, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    ra=3, rb=5, ldrb
    read=Vr, vr=4
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=4095, alu=And, ldq
    d=Q, r=Bus, s=Register, rb=0, alu=And, ldq
    d=Q, r=Bus, rb=4, ldrb
    read=Vr, vr=5
    d=Object, page=Fetch
probe_selector:
    ra=4, s=Branch, brch=5, alu=Add, ldq
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
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=3, seq=Jump, brch=lookup_class
method_found:
    d=4, idx=Load
    mem=Read
    d=Object, page=Fetch
    ra=4, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    ra=15, s=Branch, brch=perform_found, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_found
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ; Header payload: primitive 0..8, literals 9..16, large frame 17,
    ; temporaries 18..23, arguments 24..28. Quick returns use primitive >=256.
    ra=5, s=Branch, brch=8, alu=Rotate, rb=2, ldrb
    ra=2, s=Branch, brch=31, alu=And, rb=2, ldrb
    ra=2, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    ra=5, s=Branch, brch=511, alu=And, rb=0, ldrb
    d=primitive_method_failed, r=Bus, rb=15, ldrb
primitive_dispatch:
    ra=0, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=256, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=quick_return
    seq=Jump, brch=dispatch_primitive
quick_return:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    ra=0, s=Branch, brch=256, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=quick_self
    ra=0, s=Branch, brch=257, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_true
    ra=0, s=Branch, brch=258, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_false
    ra=0, s=Branch, brch=259, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=quick_nil
    ra=0, s=Branch, brch=264, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=quick_field
    ra=0, s=Branch, brch=261, alu=Sub, cin=One, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result
quick_nil:
    d=NIL, ldvr, vr=5, seq=Jump, brch=send_result
quick_self:
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
quick_field:
    ra=0, s=Branch, brch=261, alu=Sub, cin=One, rb=4, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
primitive_failed:
    d=Register, ra=15, seq=Bus
primitive_method_failed:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    seq=Jump, brch=activate

activate:
    ra=5, s=Branch, brch=23, alu=Rotate, rb=10, ldrb
    ra=10, s=Bus, d=255, alu=And, rb=10, ldrb
    ra=5, s=Branch, brch=14, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=63, alu=And, rb=4, ldrb
    ra=4, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=argument_mismatch
    ra=5, s=Bus, d=0x20000, alu=And, flags
    d=20, r=Bus, rb=6, ldrb
    seq=ConditionalJump, cc=Zero, brch=context_size
    d=40, r=Bus, rb=6, ldrb
context_size:
    ra=6, s=Branch, brch=8, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=SPECIAL_10, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    d=3, r=Bus, rb=7, ldrb
initialize_context:
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=initialize_context
    d=1, idx=Load
    ra=6, s=Branch, brch=2, alu=Sub, cin=One, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    d=2, idx=Load
    d=0x3000, mem=Write
    d=sends_hash_1, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
sends_hash_1:
    d=3, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    idx=Increment
    ra=10, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=5, alu=Add, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    read=Vr, vr=7
    d=Object, mem=Write
    d=0, r=Bus, rb=7, ldrb
copy_argument:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=7, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=8, alu=Add, rb=6, ldrb
    d=Register, ra=6, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=7, s=Branch, brch=8, alu=Add, ldq
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
    d=5, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

send_result:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, s=Branch, brch=1, alu=Add, rb=7, ldrb
clear_send_operands:
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=clear_send_operands
    d=16, r=Bus, rb=15, ldrb
    d=5, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=load_context

return_sender:
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=cannot_return
    read=Vr, vr=0
    d=Object, page=Fetch
    d=3, idx=Load
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    read=Vr, vr=7
    d=Object, ldvr, vr=0
    d=16, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context
resume_result:
    read=Vr, vr=5
    d=Object, ldsym, seq=Jump, brch=push
recursive_not_understood:
    d=6, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context
argument_mismatch:
    d=7, r=Bus, rb=15, ldrb, seq=Jump, brch=save_context

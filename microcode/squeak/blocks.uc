; Squeak BlockContexts share home temporaries but own caller/IP/SP/arguments.
; context_home: VR0 -> VR3, return via R6, preserving numeric registers.
; blockCopy: (special bytecode 200 only) allocates a same-sized frame.
; value/valueWithArguments: validate before mutation, copy arguments then switch.
; All contexts have two metadata words and six fixed guest fields.
context_home:
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_11, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=context_block_home
    d=SPECIAL_10, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    read=Vr, vr=0
    d=Object, ldvr, vr=3
    d=Register, ra=6, seq=Bus
context_block_home:
    d=8, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size, class=SPECIAL_10
    d=Register, ra=6, seq=Bus

primitive_block_copy:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_11, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=block_copy_home
    d=SPECIAL_10, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=3, seq=Jump, brch=block_copy_argument
block_copy_home:
    d=8, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
block_copy_argument:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size, class=SPECIAL_10
    d=Object, r=Bus, rb=6, ldrb
    ra=6, s=Branch, brch=8, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=SPECIAL_11, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    d=3, r=Bus, rb=7, ldrb
block_initialize:
    d=Register, ra=7, idx=Load
    d=NIL, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=block_initialize
    d=1, idx=Load
    ra=6, s=Branch, brch=2, alu=Sub, cin=One, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
    d=2, idx=Load
    d=0x3000, mem=Write
    d=blocks_hash_1, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
blocks_hash_1:
    d=4, idx=Load
    ra=8, s=Branch, brch=2, alu=Add, estk=Compact, compact=2
    d=Estk, mem=Write
    d=7, idx=Load
    d=Estk, mem=Write
    d=5, idx=Load
    d=0xc200000000, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    d=8, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
    seq=Jump, brch=send_result

primitive_value:
    d=0, r=Bus, rb=2, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_11, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=6, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
block_value_capacity:
    read=Size
    d=Object, r=Bus, rb=6, ldrb
    ra=6, s=Branch, brch=8, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=7, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    d=9, r=Bus, rb=7, ldrb
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
    d=Q, r=Bus, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    seq=Jump, brch=block_store_argument
block_array_argument:
    read=Vr, vr=3
    d=Object, page=Fetch
    ra=7, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
block_store_argument:
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=7, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    d=Symbol, mem=Write
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=block_copy_arguments
block_activate:
    read=Vr, vr=5
    d=Object, page=Fetch
    d=7, idx=Load
    mem=Read
    d=Object, ldsym
    d=4, idx=Load
    d=Symbol, mem=Write
    idx=Increment
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    d=3, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=1, s=Branch, brch=1, alu=Add, rb=7, ldrb
block_clear_caller:
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=block_clear_caller
    d=5, idx=Load
    ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    read=Vr, vr=5
    d=Object, ldvr, vr=0
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

cannot_return:
    ra=14, rb=9, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=9, alu=Add, ldq
    d=Q, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    idx=Increment
    read=Vr, vr=5
    d=Object, mem=Write
    ra=9, s=Branch, brch=2, alu=Add, rb=9, ldrb
    d=SPECIAL_21, ldvr, vr=4
    d=1, r=Bus, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=13, ldrb
    seq=Jump, brch=send_prepare

primitive_value_array:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_11, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=6, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_7, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=0, ldrb
    ra=0, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=1, r=Bus, rb=2, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    seq=Jump, brch=block_value_capacity

; Message-send engine shared by bytecode handlers and runtime-generated sends.
; Entry send_prepare needs VR4 selector, R1 arity, R2 super flag, R13 opcode,
; and valid caller caches R8..R14. It finds VR6 at component R9-R1+7 of VR0.
; Set R13=0 for synthetic sends so a stale opcode cannot select a fast primitive.
;
; Flow: decode selector -> save caller -> optional special primitive -> class
; lookup -> header decoding -> quick result / primitive / context activation.
; Lookup is bounded per dictionary, then follows superclass links. Dictionaries
; must have power-of-two selector capacity; this code assumes a valid class
; hierarchy. No method cache is maintained, so flushCache is a successful no-op.
;
; Primitive ABI: R0 number, R1 arity, VR6 receiver, VR7 fallback method,
; R8/R9 advanced caller IP/SP, R15 failure microaddress. Success normally places
; a rooted value in VR5 and enters send_result; failure enters primitive_failed,
; which jumps via R15. Special sends fail back to lookup; method primitives fail
; to primitive_method_failed, which reloads the header before activation.
;
; send_result consumes receiver plus R1 arguments, reloads the caller caches,
; and pushes VR5. Block activation, perform, and system stops have their own
; completion paths. No validation failure may discard the original operands.
; See ../README.md for the primitive-to-module map and common helper contracts.
;
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
; Failed lookup constructs a guest Message through messages.uc. Only a missing
; doesNotUnderstand: handler stops with status 6. Argument mismatch uses status 7.

; Opcodes 208..255: low nibble selects literal; (opcode-208)>>4 is arity.
send_literal:
    ra=13, s=Bus, d=15, alu=And, rb=4, ldrb
    ra=13, s=Branch, brch=208, alu=Sub, cin=One, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    ra=1, shift=Right, rb=1, ldrb
    d=0, r=Bus, rb=2, ldrb, seq=Jump, brch=send_selector
; 131/133 use one extension: low5 literal index, high3 arity; 133 is super.
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
; 132/134 use two extension bytes: arity then literal index; 134 is super.
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
    ; Materialize both caller fields before lookup or primitive execution.
    ; The first write consumes old IDX/ESTKR while preparing the next field.
    d=3, idx=Load, ra=8, estk=Compact, compact=2
    d=Estk, mem=Write, idx=Increment, ra=9, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=9, rb=1, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=6
    ; Five special bytecodes attempt their primitive before method lookup.
    ; Failure returns here with the original receiver and arguments intact.
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
; Class component 2 is superclass, 3 dictionary. Dictionary component 2 is
; its tally (unused here), 3 method Array, 4 onward selectors. Capacity is
; physical size-3. R0=mask, R3=capacity, R4=probe, R5=remaining probes.
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
; Use the same zero-based probe in the parallel method Array (component +2).
; VR7 replaces that temporary array role with the selected method. perform
; intercepts here before any quick method or primitive can change operands.
method_found:
    d=3, idx=Load
    mem=Read
    d=Object, page=Fetch
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    ra=15, s=Branch, brch=perform_found, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_found
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ; Converted header payload is source_header >> 1. Flag is bits 12..14.
    ; Rotate right by twelve; the following mask keeps header bits 12..14.
    ra=5, s=Branch, brch=20, alu=Rotate, rb=2, ldrb
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
    ra=5, s=Branch, brch=25, alu=Rotate, rb=4, ldrb
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
; Flags 0..4 encode argument count directly. Flags 5/6 are zero-argument
; quick self/field returns; flag 7 uses the next-to-last literal extension.
normal_header:
    ra=2, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    seq=Jump, brch=activate
; Extension payload low8 is primitive number; bits 8..12 are arity.
; At least two literals are needed: extension and defining-class Association.
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
    d=primitive_method_failed, r=Bus, rb=15, ldrb
primitive_dispatch:
    ; Observation boundary: R0 holds the primitive, VR7 its method, and R1
    ; its argument count. Device models never receive this language metadata.
    ra=0, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=19, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_integer
    ra=0, s=Branch, brch=40, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_nonfloat
    ra=0, s=Branch, brch=55, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_float
primitive_nonfloat:
    ra=0, s=Branch, brch=60, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=61, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=62, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=63, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=65, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=67, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=68, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=73, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=83, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_perform
    ra=0, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_perform
    ra=0, s=Branch, brch=85, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=86, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=87, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=89, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=93, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_input_semaphore
    ra=0, s=Branch, brch=90, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_mouse_point
    ra=0, s=Branch, brch=91, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_cursor_position
    ra=0, s=Branch, brch=92, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_cursor_link
    ra=0, s=Branch, brch=94, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_sample_interval
    ra=0, s=Branch, brch=95, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_input_word
    ra=0, s=Branch, brch=96, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitblt
    ra=0, s=Branch, brch=98, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_clock
    ra=0, s=Branch, brch=99, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_clock
    ra=0, s=Branch, brch=100, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_timer
    ra=0, s=Branch, brch=101, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitmap
    ra=0, s=Branch, brch=102, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitmap
    ra=0, s=Branch, brch=110, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_identity
    ra=0, s=Branch, brch=111, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_class
    ra=0, s=Branch, brch=112, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_capacity
    ra=0, s=Branch, brch=115, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_capacity
    ra=0, s=Branch, brch=135, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_snapshot_target
    ra=0, s=Branch, brch=116, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_low_space
    ra=0, s=Branch, brch=113, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_system_stop
    ra=0, s=Branch, brch=114, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_system_stop
    ra=0, s=Branch, brch=82, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value_array
    ra=0, s=Branch, brch=80, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_block_copy
    ra=0, s=Branch, brch=81, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value
    ra=0, s=Branch, brch=72, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_become
    ra=0, s=Branch, brch=71, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new
    ra=0, s=Branch, brch=75, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_identity_number
    ra=0, s=Branch, brch=76, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_identity_object
    ra=0, s=Branch, brch=77, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_instances
    ra=0, s=Branch, brch=78, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_instances
    ra=0, s=Branch, brch=79, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new_method
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new
    ; Unimplemented or failed primitive methods execute their Smalltalk body.
    seq=Jump, brch=activate
primitive_failed:
    d=Register, ra=15, seq=Bus
primitive_method_failed:
    ; Primitive scratch may replace the numeric header cache. The tagged method
    ; remains in VR7 until the primitive has irrevocably succeeded.
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    seq=Jump, brch=activate

; R5 is the method header, VR7 the method, VR6 the receiver. Decode literal
; count bits 0..5, frame-size bit 6, temporary count bits 7..11. Context sizes
; 19/39 include descriptor and six fixed fields plus 12/32 stack slots.
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
; Initialize through the final physical component before copying arguments.
; Initial IP=2*literal_count+3 and initial SP=temporary_count; temporaries
; include arguments, which must fit both the declared count and chosen frame.
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

; Object is the proposed sender/caller. A nil saved IP marks an inactive
; return target and routes to cannot_return before modifying the outgoing frame.
return_sender:
    ; Result and destination must both survive a destination refill.
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=cannot_return
    read=Vr, vr=0
    d=Object, page=Fetch
    d=2, idx=Load
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

; Message-send engine shared by bytecode handlers and runtime-generated sends.
; Entry send_prepare needs VR4 selector, R1 arity, R2 super flag, R13 opcode,
; and valid caller caches R8..R14. It finds VR6 at component R9-R1+7 of VR0.
; Set R13=0 for synthetic sends so a stale opcode cannot select a fast primitive.
;
; Flow: decode selector -> save caller -> optional special primitive -> class
; lookup -> header decoding -> quick result / primitive / context activation.
; Lookup is bounded per dictionary, then follows superclass links. Dictionaries
; must have power-of-two selector capacity; this code assumes a valid class
; hierarchy. A method cache, described at cache_lookup, precedes the search.
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
; Context recycling. A MethodContext that returns to its sender is normally
; garbage at once. Instead of leaving it to the collector, the return clears
; its temporaries and stack and parks it in the unused field (component 6,
; the Blue Book's receiverMap) of its sender's home MethodContext: the
; sender itself, or the home of a BlockContext sender. The next send from
; that method or one of its blocks that needs that frame size takes it back
; out. Parking a frame replaces any frame already parked there.
;
; A frame may be reused only if nothing else can refer to it. The control
; stack register counts the newest frames of the running chain that are known
; to be unshared: each method activation increments it, a recycling return
; decrements it, and a return that finds it zero leaves the frame alone.
; Anything that can hand a context to the guest resets it to zero, which
; marks every existing frame as shared for good: pushing thisContext,
; blockCopy, a process switch, a block's return from its home method,
; cannotReturn:, and the primitives that produce an object from its identity
; or enumerate instances. Frames activated afterwards count again.
;
; Failed lookup constructs a guest Message through messages.uc. Only a missing
; doesNotUnderstand: handler stops with status 6. Argument mismatch uses status 7.

; Opcodes 208..255: low nibble selects literal; (opcode-208)>>4 is arity.
send_literal:
    ra=13, s=Bus, d=15, alu=And, rb=4, ldrb
    ra=13, s=Branch, brch=208, alu=Sub, cin=One, shift=Right, rb=1, ldrb
    ra=1, s=Branch, brch=29, alu=Rotate, rb=1, ldrb
    ra=1, s=Bus, d=0x1fffffff, alu=And, rb=1, ldrb
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
    ra=0, s=Branch, brch=27, alu=Rotate, rb=1, ldrb
    ra=1, s=Bus, d=0x07ffffff, alu=And, rb=1, ldrb
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
    d=Object, page=Fetch, ra=4, s=Branch, brch=3, alu=Add, ldq
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
    d=Object, ldvr, vr=3, seq=Jump, brch=cache_lookup
integer_class:
    d=0xa000000006, ldvr, vr=3, seq=Jump, brch=cache_lookup
super_class:
    ; Super starts above the method's defining class, not above self's class.
    ; The last literal is that class's Association, whose value is guest field 1.
    ; Super sends bypass the method cache: R6=0 asks for no fill.
    d=0, r=Bus, rb=6, ldrb
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    read=Vr, vr=1
    d=Object, page=Fetch, ra=10, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    seq=Jump, brch=superclass

; Method cache. Component 44 of root 29's private Array holds an Array of 512
; entries, three components each: receiver class, selector, method. A send's
; entry is component 2 + 3 * ((class ^ selector) & 511), from the low
; identity bits. When class and selector match, the send uses the method
; without searching dictionaries. A miss leaves the entry's component in R6
; and the class in ESTKR, and method_found stores the result there. Empty
; entries hold machine NIL, which matches no class. flushCache (89) and
; become: (72) drop the cache; the next miss allocates a fresh one. The
; cache is private and is not reachable from guest objects.
cache_lookup:
    d=0, r=Bus, rb=6, ldrb
    d=NIL, ldsym, r=Branch, brch=29, rb=0, ldrb
    ra=0, d=Root, ldsym, ldvr, vr=5, seq=ConditionalJump, cc=Symbol, brch=lookup_class
    d=Symbol, page=Fetch
    d=44, idx=Load
    mem=Read
    d=Object, ldsym, ldvr, vr=5
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=cache_create
cache_probe:
    read=Vr, vr=3
    d=Object, r=Bus, rb=4, ldrb
    read=Vr, vr=4
    d=Object, r=Bus, alu=Xor, rb=4, ldrb
    ra=4, s=Bus, d=511, alu=And, rb=4, ldrb
    ra=4, shift=Left, rb=5, ldrb
    ra=4, rb=5, alu=Add, ldrb
    ra=5, s=Branch, brch=2, alu=Add, rb=6, ldrb
    read=Vr, vr=5
    d=Object, page=Fetch
    d=Register, ra=6, idx=Load
    mem=Read, idx=Increment
    d=Object, ldsym
    read=Vr, vr=3
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=cache_miss
    mem=Read, idx=Increment
    d=Object, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=cache_miss
    mem=Read
    d=Object, ldvr, vr=7, seq=Jump, brch=method_ready
cache_miss:
    ; The class waits in ESTKR; lookup changes VR3 as it climbs.
    read=Vr, vr=3
    d=Object, estk=Bus, seq=Jump, brch=lookup_class
cache_create:
    ; 512 entries after the descriptor, initially machine NIL. A small heap
    ; goes without: create the cache only with at least 16384 words free.
    read=FreeWords
    d=Object, r=Bus, s=Branch, brch=16384, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=cache_skip
    d=0xa000000008, page=Allocate, size=1537, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=12288, mem=Write
    d=29, r=Bus, rb=0, ldrb
    ra=0, d=Root, ldsym
    d=Symbol, page=Fetch
    d=44, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write, seq=Jump, brch=cache_probe
cache_skip:
    d=0, r=Bus, rb=6, ldrb, seq=Jump, brch=lookup_class

; Drop the method cache; the next miss allocates a fresh one. Returns via R7;
; R2 is scratch.
cache_flush:
    d=NIL, ldsym, r=Branch, brch=29, rb=2, ldrb
    ra=2, d=Root, ldsym, seq=ConditionalJump, cc=Symbol, brch=cache_flushed
    d=Symbol, page=Fetch
    d=44, idx=Load
    d=NIL, mem=Write
cache_flushed:
    d=Register, ra=7, seq=Bus

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
    ra=0, rb=4, alu=And, ldq, ldrb, seq=Jump, brch=probe_selector
superclass:
    d=2, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    mem=Read
    d=Object, ldvr, vr=3, seq=Jump, brch=lookup_class
; Use the same zero-based probe in the parallel method Array (component +2).
; VR7 replaces that temporary array role with the selected method. perform
; intercepts here before any quick method or primitive can change operands.
method_found:
    d=3, idx=Load
    mem=Read
    d=Object, page=Fetch, ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    ; Fill the method cache entry chosen by cache_lookup, if any.
    ra=6, flags
    seq=ConditionalJump, cc=Zero, brch=method_ready
    d=NIL, ldsym, r=Branch, brch=29, rb=0, ldrb
    ra=0, d=Root, ldsym, seq=ConditionalJump, cc=Symbol, brch=method_ready
    d=Symbol, page=Fetch
    d=44, idx=Load
    mem=Read
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=method_ready
    d=Symbol, page=Fetch
    d=Register, ra=6, idx=Load
    d=Estk, mem=Write, idx=Increment
    read=Vr, vr=4
    d=Object, mem=Write, idx=Increment
    read=Vr, vr=7
    d=Object, mem=Write
method_ready:
    ra=15, s=Branch, brch=perform_found, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=perform_found
    d=2, idx=Load, read=Vr, vr=7
    d=Object, page=Fetch
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
    d=Object, page=Fetch, ra=4, s=Branch, brch=2, alu=Add, ldq
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
    ra=0, s=Branch, brch=24, alu=Rotate, rb=2, ldrb
    ra=2, s=Bus, d=31, alu=And, rb=2, ldrb
    ra=2, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=argument_mismatch
    ra=0, s=Bus, d=255, alu=And, rb=0, ldrb
    d=primitive_method_failed, r=Bus, rb=15, ldrb
primitive_dispatch:
    seq=Jump, brch=dispatch_primitive
primitive_failed:
    d=Register, ra=15, seq=Bus
primitive_method_failed:
    ; Primitive scratch may replace the numeric header cache. The tagged method
    ; remains in VR7 until the primitive has irrevocably succeeded.
    d=2, idx=Load, read=Vr, vr=7
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    seq=Jump, brch=activate

; R5 is the method header, VR7 the method, VR6 the receiver. Decode literal
; count bits 0..5, frame-size bit 6, temporary count bits 7..11. Context sizes
; 19/39 include descriptor and six fixed fields plus 12/32 stack slots.
activate:
    ra=5, s=Bus, d=63, alu=And, rb=10, ldrb
    ra=5, s=Branch, brch=25, alu=Rotate, rb=4, ldrb
    ra=4, s=Bus, d=31, alu=And, rb=4, ldrb
    ra=4, rb=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=argument_mismatch
    ra=5, s=Bus, d=64, alu=And, flags
    d=19, r=Bus, rb=6, ldrb
    seq=ConditionalJump, cc=Zero, brch=context_size
    d=39, r=Bus, rb=6, ldrb
context_size:
    ; R14 becomes the new frame's stack capacity; activation keeps it.
    ra=6, s=Branch, brch=7, alu=Sub, cin=One, ldq, rb=14, ldrb
    d=Q, r=Bus, s=Register, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ; Count this activation as recyclable; see "Context recycling" below.
    cstk=Increment
    ; The caller's home MethodContext keeps a finished callee in its unused
    ; field (component 6). Reuse it when it has the size this method needs.
    ; VR3 is that home: the caller itself, or a BlockContext caller's home.
    read=Vr, vr=0
    d=Object, ldvr, vr=3
    read=Vr, vr=0
    d=Object, page=Fetch
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=context_parked
    d=7, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=context_allocate
context_parked:
    d=6, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, ldvr, vr=5, seq=ConditionalJump, cc=Symbol, brch=context_allocate
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Register, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=context_allocate
    read=Type
    d=0xa00000000b, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=context_allocate
    ; Take it out of its keeper before it becomes active.
    read=Vr, vr=3
    d=Object, page=Fetch
    d=6, idx=Load
    d=NIL, mem=Write
    read=Vr, vr=5
    d=Object, page=Fetch
    seq=Jump, brch=context_fields
context_allocate:
    d=0xa00000000b, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ; Initializing all pointer fields replaces the allocator's machine NIL.
    d=2, r=Bus, rb=7, ldrb, idx=Load
; Initialize through the final physical component before copying arguments.
; Initial IP=2*literal_count+3 and initial SP=temporary_count; temporaries
; include arguments, which must fit both the declared count and chosen frame.
initialize_context:
    d=NIL, mem=Write, idx=Increment, ra=6, rb=7, alu=Sub, cin=One, flags
    ra=7, s=Bus, d=1, alu=Add, rb=7, ldrb, seq=ConditionalJump, cc=!Zero, brch=initialize_context
    d=1, idx=Load
    ra=6, s=Branch, brch=1, alu=Sub, cin=One, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    d=Register, ra=0, mem=Write
; A recycled frame arrives here selected, with its descriptor, its own cached
; callee, and nil temporaries and stack already in place.
context_fields:
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
    d=0, r=Bus, rb=7, ldrb, seq=Jump, brch=copy_argument

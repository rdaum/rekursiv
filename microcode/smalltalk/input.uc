; Entry map: 90 mousePoint (no arguments), 91 cursor position (Point),
; 92 cursor link (Boolean), 94 sample interval (0..16383), 95 next input word
; (no arguments). Primitive 93 registration and boundary dispatch are in events.uc.
; Successful setters return receiver through clock_result; empty word reads and
; invalid primitive operands take R15 fallback. Malformed external packets halt.
;
; The same module owns a 16-word ring and translates device packets into guest
; 16-bit words: type 1 motion -> 0x1000|x, 0x2000|y; type 2 down -> 0x3000|code;
; type 3 up -> 0x4000|code. Prepend elapsed milliseconds (0..4095), or absolute
; 0x5000, high16, low16. Coordinates clamp to 0..4095; key codes must fit 12 bits.
; The saved timestamp determines when to rebase, keeping guest accumulated deltas
; within SmallInteger range. Up to five words require reservation before dequeue.
;
; input_buffer_load lazily creates/returns selected root29 state in VR3 via R7;
; R0, SYMBOL, IDX/Object and selection are scratch. It preserves other numeric
; registers and VR4..VR7, allowing bitmap and snapshot registration to share it.
; input_append stores R2's unsigned word at tail R5 and advances modulo 16;
; return via R6, clobber R0/R5/R7/ESTKR/IDX/Object, preserve packet fields and Q.
; It writes data only; input_packet_publish updates counts before consuming the
; physical packet and acknowledging its notification. A full ring defers input
; while allowing timer/storage events and guest execution to proceed.
;
; Pointer primitives use generic signed 32-bit device registers. This runtime
; represents Point coordinates as signed 15-bit SmallIntegers; out-of-range
; coordinates enter guest fallback. Validation precedes allocation or writes.
;
; R2/R3 retain X/Y across device waits and object allocation. R4 is range-check
; scratch, R6 a continuation, and R7 a byte-addressed device register. Numeric
; scratch contains no object references. VR6 retains the send receiver; VR3
; roots its argument; VR5 roots a newly allocated Point until send_result.
; R8..R14 retain the caller's execution caches throughout these primitives.
primitive_mouse_point:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0x300, r=Bus, rb=7, ldrb
    ; Reading X captures a pair; reading Y returns that captured coordinate,
    ; even if the physical mouse moves while either reply is delayed.
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    d=0x304, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=3, ldrb
    d=mouse_allocate, r=Bus, rb=6, ldrb, seq=Jump, brch=pointer_range
mouse_allocate:
    ; A Point has a pointer descriptor and two fields. Allocate only after
    ; both coordinates pass: fallback must not leak a partially built Point.
    d=0xa00000000d, page=Allocate, size=3, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=16, mem=Write
    idx=Increment
    ra=2, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=3, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=send_result

; Fetch the one argument into VR3 and return it in Object. The caller context
; stays selected; cursor_point fetches the argument when needed. Preserve the
; send receiver in VR6; R6 is the continuation. No host decodes guest values.
input_argument:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Register, ra=6, seq=Bus
primitive_cursor_position:
    d=cursor_point, r=Bus, rb=6, ldrb, seq=Jump, brch=input_argument
cursor_point:
    d=Object, page=Fetch
    read=Type
    d=0xa00000000d, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=cursor_publish, r=Bus, rb=6, ldrb, seq=Jump, brch=pointer_range
pointer_range:
    ; Unsigned (coordinate + 16384) < 32768 checks the signed 15-bit range
    ; without relying on signed-overflow flags at the 32-bit extremes.
    ra=2, s=Bus, d=16384, alu=Add, rb=4, ldrb
    ra=4, s=Bus, d=0xffff8000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=3, s=Bus, d=16384, alu=Add, rb=4, ldrb
    ra=4, s=Bus, d=0xffff8000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Register, ra=6, seq=Bus
cursor_publish:
    ; Separate staging registers prevent visible half-updates. Each write
    ; retires once. Only the final publish changes the cursor or linked mouse;
    ; a device error faults the processor, as for other generic device writes.
    d=0x400, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=0x404, r=Bus, rb=7, ldrb
    ra=3, ldq
    ra=7, d=Q, io=Write
    d=0x408, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    seq=Jump, brch=clock_result
primitive_cursor_link:
    d=cursor_link_value, r=Bus, rb=6, ldrb, seq=Jump, brch=input_argument
cursor_link_value:
    ; Compare full singleton references; a compact numeric 0 or 1 is not a
    ; guest Boolean. Failure retains the argument in the caller's stack.
    d=Object, ldsym
    d=1, r=Bus, rb=2, ldrb
    d=TRUE, seq=ConditionalJump, cc=Symbol, brch=cursor_link_write
    d=FALSE, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=0, r=Bus, rb=2, ldrb
cursor_link_write:
    d=0x40c, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    seq=Jump, brch=clock_result
primitive_sample_interval:
    d=sample_interval_value, r=Bus, rb=6, ldrb, seq=Jump, brch=input_argument
sample_interval_value:
    ; First check the complete compact tag, then the nonnegative guest range.
    ; This instruction path configures sampling; input packet production and
    ; guest word buffering are separate from the configuration transaction.
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x30c, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    seq=Jump, brch=clock_result

; Buffered input belongs to the runtime. Root29 retains a private Array:
;   component 2: storage semaphore (reserved for the storage primitive)
;   component 3: read index; 4: occupied words; 5: unsignalled words
;   components 6..37: sixteen (low14, high2) word pairs
;   components 38/39: registered cursor/display Forms
;   component 40: copied snapshot serial number and virtual leader address.
;   components 41..43: previous input timestamp, low14/mid14/high4.
; Component 41 is nil until the first packet establishes absolute time.
; Each component is a canonical guest SmallInteger or reference. No unrooted
; pointer or host queue contains converted guest words. R7 supplies the return
; microaddress for lazy allocation; VR3 retains the buffer through collection.
input_buffer_load:
    d=29, r=Bus, rb=0, ldrb
    ra=0, d=Root, ldsym, ldvr, vr=3
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=input_buffer_allocate
    d=Symbol, page=Fetch
    d=Register, ra=7, seq=Bus
input_buffer_allocate:
    d=0xa000000008, page=Allocate, size=43, scan=1
    d=Object, ldvr, vr=3
    d=1, idx=Load
    d=336, mem=Write
    d=2, r=Bus, rb=0, ldrb
input_buffer_zero:
    d=Register, ra=0, idx=Load
    d=0xc200000000, mem=Write
    ra=0, s=Branch, brch=1, alu=Add, rb=0, ldrb
    ra=0, s=Branch, brch=44, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=input_buffer_zero
    d=2, idx=Load
    d=NIL, mem=Write
    d=38, idx=Load
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    idx=Increment
    d=NIL, mem=Write
    d=29, r=Bus, rb=0, ldrb
    read=Vr, vr=3
    ra=0, d=Object, ldroot
    d=Register, ra=7, seq=Bus

; Deliver each queued word once, before accepting another external packet.
; Decrement before signalling: a higher-priority waiter may preempt this path.
; The saved count and the word ring survive any process switch or collection.
input_notifications:
    ; Compare the root bus against old SYMBOL=NIL while loading its new value.
    ; Buffered notifications are checked at every boundary, even without IRQ.
    d=NIL, ldsym, r=Branch, brch=27, rb=7, ldrb
    ra=7, d=Root, ldsym, ldvr, vr=6, seq=ConditionalJump, cc=Symbol, brch=input_notifications_done
    d=NIL, ldsym, r=Branch, brch=29, rb=7, ldrb
    ra=7, d=Root, ldsym, seq=ConditionalJump, cc=Symbol, brch=input_notifications_done
    d=Symbol, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=input_notifications_done
    ra=0, s=Branch, brch=1, alu=Sub, cin=One, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=event_recipient_ready

; Input IRQ means a raw packet is waiting. Reserve room for its maximum five
; output words before reading it. A full ring leaves the packet and IRQ intact,
; but permits timer/storage delivery and ordinary guest execution to continue.
input_packet_event:
    d=0x310, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state ; visible physical FIFO overrun
    seq=ConditionalJump, cc=Zero, brch=bad_state ; inconsistent device IRQ
    d=input_packet_space, r=Bus, rb=7, ldrb, seq=Jump, brch=input_buffer_load
input_packet_space:
    d=4, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=11, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=input_packet_full
    d=0x314, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=1, ldrb
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bad_state
    ra=1, s=Branch, brch=3, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bad_state
    d=0x318, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=3, ldrb
    d=0x31c, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=input_key_range
    ; The image's coordinate events have twelve unsigned bits. Clamp physical
    ; positions to that range here, without narrowing the generic device ABI.
    ra=3, flags
    seq=ConditionalJump, cc=!Sign, brch=input_x_max
    d=0, r=Bus, rb=3, ldrb
input_x_max:
    ra=3, s=Branch, brch=4095, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=input_y_min
    d=4095, r=Bus, rb=3, ldrb
input_y_min:
    ra=4, flags
    seq=ConditionalJump, cc=!Sign, brch=input_y_max
    d=0, r=Bus, rb=4, ldrb
input_y_max:
    ra=4, s=Branch, brch=4095, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=input_packet_time
    d=4095, r=Bus, rb=4, ldrb
    seq=Jump, brch=input_packet_time
input_key_range:
    ra=3, s=Branch, brch=4095, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bad_state
input_packet_time:
    d=0x320, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, ldq
    ; Tail = (head + count) modulo sixteen. R1 is kind, R3/R4 payload,
    ; Q timestamp, R5 tail. The append helper changes only R0/R5/R7 and ESTKR.
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    idx=Increment
    mem=Read
    ra=5, s=Bus, d=Object, alu=Add, rb=5, ldrb
    ra=5, s=Branch, brch=15, alu=And, rb=5, ldrb
    ; Most packets need only the Blue Book's twelve-bit elapsed-time word.
    ; Sending absolute time on every movement makes InputState reconstruct a
    ; LargePositiveInteger on every sample, starving the lower-priority UI.
    ; Keep all timestamp pieces as canonical SmallIntegers in rooted state.
    d=input_time_absolute, r=Bus, rb=6, ldrb
    d=41, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=input_time_remember
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=14, alu=Rotate, rb=0, ldrb
    ra=0, rb=2, alu=Or, ldrb
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=28, alu=Rotate, rb=0, ldrb
    ra=0, rb=2, alu=Or, ldrb
    ; Rebase whenever the upper timestamp bits change. This also handles
    ; midnight, wrap, and clock corrections, and bounds the guest's accumulated
    ; deltaTime to 16383 so its arithmetic stays in the SmallInteger range.
    r=Q, rb=2, alu=Sub, cin=One, ldrb
    ra=2, s=Bus, d=0xfffff000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=input_time_remember
    r=Q, s=Branch, brch=16383, alu=And, rb=0, ldrb
    ra=0, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=input_time_remember
    d=input_time_delta, r=Bus, rb=6, ldrb
input_time_remember:
    ; R2 retains delta, R6 selects absolute/delta, Q retains the new time.
    d=41, idx=Load
    r=Q, s=Branch, brch=16383, alu=And, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    r=Q, s=Branch, brch=18, alu=Rotate, rb=0, ldrb
    ra=0, s=Branch, brch=16383, alu=And, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    r=Q, s=Branch, brch=4, alu=Rotate, rb=0, ldrb
    ra=0, s=Branch, brch=15, alu=And, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    d=Register, ra=6, seq=Bus
input_time_delta:
    d=input_first_value, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_time_absolute:
    ; First packet, long gap, or clock rebase: 0x5000, high16, low16.
    d=0x5000, r=Bus, rb=2, ldrb
    d=input_time_high, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_time_high:
    r=Q, s=Branch, brch=16, alu=Rotate, rb=2, ldrb
    ra=2, s=Bus, d=65535, alu=And, rb=2, ldrb
    d=input_time_low, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_time_low:
    r=Q, s=Bus, d=65535, alu=And, rb=2, ldrb
    d=input_first_value, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_first_value:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_motion_x
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_key_down
    ra=3, s=Bus, d=0x4000, alu=Or, rb=2, ldrb
    seq=Jump, brch=input_key_append
input_key_down:
    ra=3, s=Bus, d=0x3000, alu=Or, rb=2, ldrb
input_key_append:
    d=input_packet_publish, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_motion_x:
    ra=3, s=Bus, d=0x1000, alu=Or, rb=2, ldrb
    d=input_motion_y, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_motion_y:
    ra=4, s=Bus, d=0x2000, alu=Or, rb=2, ldrb
    d=input_packet_publish, r=Bus, rb=6, ldrb, seq=Jump, brch=input_append
input_packet_publish:
    ; Derive the appended length from the new tail and old head/count.
    ; Absolute packets append 4/5 words; elapsed-time packets append 2/3.
    d=3, idx=Load
    mem=Read
    ra=5, s=Bus, d=Object, alu=Sub, cin=One, rb=2, ldrb
    idx=Increment
    mem=Read
    ra=2, s=Bus, d=Object, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=15, alu=And, rb=2, ldrb
input_packet_count:
    d=4, idx=Load
    mem=Read
    ra=2, s=Bus, d=Object, alu=Add, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    mem=Read
    ra=2, s=Bus, d=Object, alu=Add, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    ; Only complete guest words become visible. Consume the raw packet after
    ; publication, then acknowledge exactly its one physical notification.
    d=0x324, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    d=0x104, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    seq=Jump, brch=event_delivered
input_append:
    ra=5, shift=Left, rb=7, ldrb
    ra=7, s=Branch, brch=6, alu=Add, rb=7, ldrb
    d=Register, ra=7, idx=Load
    ra=2, s=Branch, brch=16383, alu=And, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=2, s=Branch, brch=18, alu=Rotate, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=And, rb=0, ldrb
    ra=0, estk=Compact, compact=2
    d=Estk, mem=Write
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=5, s=Branch, brch=15, alu=And, rb=5, ldrb
    d=Register, ra=6, seq=Bus
input_packet_full:
    d=0x100, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, s=Branch, brch=6, alu=And, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=events_done
    d=28, r=Bus, rb=5, ldrb
    d=2, r=Bus, rb=6, ldrb, seq=Jump, brch=event_select

primitive_input_word:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=29, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    d=Symbol, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, r=Bus, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, rb=5, ldrb
    ra=5, shift=Left, rb=7, ldrb
    ra=7, s=Branch, brch=6, alu=Add, rb=7, ldrb
    d=Register, ra=7, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    d=0xc200000000, mem=Write
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=14, alu=Rotate, rb=3, ldrb
    ra=2, rb=3, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    d=0xc200000000, mem=Write
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
    d=4, idx=Load
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
    ; Polling can consume an unsignalled word before registration. Pending
    ; words form the tail suffix, so cap their count at the new occupancy.
    idx=Increment
    mem=Read
    ra=4, s=Bus, d=Object, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=input_word_head
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
input_word_head:
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=5, s=Branch, brch=15, alu=And, rb=5, ldrb
    d=3, idx=Load
    ra=5, estk=Compact, compact=2
    d=Estk, mem=Write
    ; Guest unsigned 16-bit words above 16383 need a LargePositiveInteger.
    ; The generic collector may interrupt its construction after dequeue.
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=identity_small_number
    d=0, r=Bus, rb=3, ldrb, seq=Jump, brch=identity_count

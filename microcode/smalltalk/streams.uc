; Entries: primitive_stream for 65 next, 66 nextPut:, 67 atEnd. Arity is
; zero except nextPut: (one). Physical stream components 2/3/4/5 hold collection,
; position, readLimit, writeLimit; position counts already consumed elements.
; The next item is collection component position+2, then position increments.
;
; next returns an Array element or the Character table entry for a String byte.
; nextPut: returns its argument after storing the pointer or Character's byte.
; Only those two concrete collection classes take this path. next/nextPut:
; validate position against the operation's limit, physical collection length,
; and SmallInteger capacity before storing. atEnd compares position>=readLimit
; without advancing. Collection writes precede the position update; machine I/O
; faults are processor faults, not transactional primitive fallback.
;
; Blue Book stream primitives 65/66/67. The guest owns collection, position,
; readLimit, and writeLimit at fields 0/1/2/3. Only Array and String use this
; fast path. A failed check leaves both collection and position unchanged.
; VR4 roots collection, VR5 result, VR6 stream, VR7 fallback method.
; R2 position, R3 limit, R4 collection kind (0 pointer, 2 byte), R5 length,
; R6 byte value. R8/R9 retain the caller IP/SP until send_result reloads it.
primitive_stream:
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_write_arity
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=stream_fields
stream_write_arity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
stream_fields:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    d=3, idx=Load
    mem=Read
    d=Object, ldsym
    d=Object, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_write_limit
    d=4, idx=Load, seq=Jump, brch=stream_limit
stream_write_limit:
    d=5, idx=Load
stream_limit:
    mem=Read
    d=Object, ldsym
    d=Object, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Type
    d=0xa000000008, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=stream_array
    d=0xa000000007, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=2, r=Bus, rb=4, ldrb, seq=Jump, brch=stream_kind
stream_array:
    d=0, r=Bus, rb=4, ldrb
stream_kind:
    ra=0, s=Branch, brch=67, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_at_end
    ; next/nextPut require a valid position and space under both limits.
    ra=2, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    ra=2, s=Branch, brch=16383, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=5, ldrb
    ra=5, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Object, r=Bus, shift=Right, rb=5, ldrb
    ra=5, shift=Right, rb=5, ldrb
    ra=4, flags
    seq=ConditionalJump, cc=!Zero, brch=stream_length
    ra=5, shift=Right, rb=5, ldrb
stream_length:
    ra=2, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_write
    ra=2, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=stream_advance
    read=Vr, vr=5
    d=Object, r=Bus, s=Branch, brch=2, alu=Add, ldq
    d=0xa000000019, page=Fetch
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=stream_advance
stream_write:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=stream_store_pointer
    ; String writes accept Character instances with an unsigned byte code.
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Type
    d=0xa000000014, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=Object, r=Bus, rb=6, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=6, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=6, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Vr, vr=4
    d=Object, page=Fetch
    ra=2, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    d=Register, ra=6, mem=Write, seq=Jump, brch=stream_advance
stream_store_pointer:
    read=Vr, vr=4
    d=Object, page=Fetch
    ra=2, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
stream_advance:
    read=Vr, vr=6
    d=Object, page=Fetch
    ra=2, s=Branch, brch=1, alu=Add, estk=Compact, compact=2
    d=3, idx=Load
    d=Estk, mem=Write, seq=Jump, brch=send_result
stream_at_end:
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_false
    seq=Jump, brch=primitive_true

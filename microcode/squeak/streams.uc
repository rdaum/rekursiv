; Stream primitives 65/66/67. Fields are collection, index, read/write limits.
; Fast paths support the archived Array, String, ByteArray and Bitmap classes.
; String elements map through CharacterTable; word values retain unsigned32.
; All limit/type checks precede collection writes and the index update.
; VR4 collection, VR5 value/result, VR6 stream. R2 index, R3 limit, R4 kind,
; R5 length, R10 string mode (or numeric kind during nextPut validation).
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
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    d=4, idx=Load
    mem=Read
    d=Object, ldsym
    d=Object, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_write_limit
    d=5, idx=Load, seq=Jump, brch=stream_limit
stream_write_limit:
    read=Size
    d=Object, r=Bus, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=6, idx=Load
stream_limit:
    mem=Read
    d=Object, ldsym
    d=Object, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_7, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=stream_array
    d=0, r=Bus, rb=10, ldrb
    d=SPECIAL_26, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=stream_bytes
    d=SPECIAL_4, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=stream_words
    d=SPECIAL_6, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=1, r=Bus, rb=10, ldrb
stream_bytes:
    d=2, r=Bus, rb=4, ldrb, seq=Jump, brch=stream_kind
stream_words:
    d=1, r=Bus, rb=4, ldrb, seq=Jump, brch=stream_kind
stream_array:
    d=0, r=Bus, rb=4, ldrb
stream_kind:
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=5, ldrb
    ra=5, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Object, r=Bus, shift=Right, rb=5, ldrb
    ra=5, shift=Right, rb=5, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_length
    ra=5, shift=Right, rb=5, ldrb
    ra=5, shift=Right, rb=5, ldrb
stream_length:
    ra=0, s=Branch, brch=67, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_at_end
    ; next/nextPut require a valid position and space under both limits.
    ra=2, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    ra=2, s=Bus, d=1073741823, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    ra=2, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_failed
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=stream_write
    ra=2, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=stream_advance
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=stream_numeric_result
    read=Vr, vr=5
    d=Object, r=Bus, s=Branch, brch=3, alu=Add, ldq
    d=SPECIAL_24, page=Fetch
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=stream_advance
stream_write:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=stream_store_pointer
    ra=10, flags
    seq=ConditionalJump, cc=Zero, brch=stream_numeric_value
    ; String writes accept Character instances with an unsigned byte code.
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_19, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=3, idx=Load
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
    ra=2, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    d=Register, ra=6, mem=Write, seq=Jump, brch=stream_advance
stream_store_pointer:
    read=Vr, vr=4
    d=Object, page=Fetch
    ra=2, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
stream_advance:
    read=Vr, vr=6
    d=Object, page=Fetch
    ra=2, s=Branch, brch=1, alu=Add, estk=Compact, compact=2
    d=4, idx=Load
    d=Estk, mem=Write, seq=Jump, brch=send_result
stream_at_end:
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_true
    ra=2, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=primitive_true
    seq=Jump, brch=primitive_false

stream_numeric_value:
    ra=4, rb=10, ldrb
    read=Vr, vr=5
    d=Object, ldsym
    d=stream_numeric_valid, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
stream_numeric_valid:
    ra=10, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=stream_numeric_store
    ra=4, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
stream_numeric_store:
    read=Vr, vr=4
    d=Object, page=Fetch
    ra=2, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    d=Register, ra=4, mem=Write, seq=Jump, brch=stream_advance
stream_numeric_result:
    ; Advance after validation, before allocating a LargePositiveInteger.
    read=Vr, vr=5
    d=Object, r=Bus, rb=4, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    ra=2, s=Branch, brch=1, alu=Add, estk=Compact, compact=2
    d=4, idx=Load
    d=Estk, mem=Write, seq=Jump, brch=positive_result

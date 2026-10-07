; Primitive map: 60 at:, 61 at:put:, 62 size, 63/64 String at:/at:put:,
; 68/69 CompiledMethod objectAt:/objectAt:put:, 73/74 instVarAt:/instVarAt:put:.
; Guest indices are one-based. Ordinary indexed access skips the fixed-field
; count from class component 4; instVar access starts at the first body field.
; Pointer reads preserve all 40 bits. Word results are unsigned 16-bit integers;
; String reads use Character table identity 25 and writes extract Character code.
;
; The descriptor is (guest_byte_length << 2) | kind: pointers=0, words=1,
; bytes=2, method=3. For pointer/word kinds divide guest bytes by two for length.
; R11 becomes the translated physical component after preserving the fixed count
; across positive_value. For method bytes require index >= 2*literal_count+3,
; then component=index-literal_count. The reserved header/literal byte prefix has
; no byte view. objectAt: uses full tagged components and header writes fail.
;
; Helpers exported here: positive_value takes SYMBOL and returns unsigned
; 0..65535 in R4 via R6 (R7 scratch); errors use primitive_failed/R15.
; positive_result takes R4, returns VR5 through send_result, allocating a
; LargePositiveInteger for 16384..65535. It is a final primitive result path,
; not a subroutine. Its two raw little-endian bytes follow descriptor 10.
;
; Indexed access translates guest indices through the language-owned descriptor.
; R0 primitive, R1 arity, R2 physical kind, R3 logical length, R4 index/value,
; R10 read(0)/write(1)/size(2), R11 physical index. Caller caches are restored by
; send_result. VR3 roots a write value; VR6 roots the receiver; VR7 retains the
; fallback method. Every failing check precedes the first guest write.
primitive_indexed:
    d=0, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=62, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_size_arity
    ra=0, s=Branch, brch=61, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_arity
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=indexed_metadata
indexed_write_arity:
    d=1, r=Bus, rb=10, ldrb
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=indexed_metadata
indexed_size_arity:
    d=2, r=Bus, rb=10, ldrb
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
indexed_metadata:
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb
    d=Object, r=Bus, shift=Right, rb=3, ldrb
    ra=3, shift=Right, rb=3, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=indexed_length
    ra=3, shift=Right, rb=3, ldrb
indexed_length:
    ra=0, s=Branch, brch=68, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_method_fields
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_method_fields
    ra=0, s=Branch, brch=73, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_fixed_zero
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_fixed_zero
    read=Type
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=2047, alu=And, rb=7, ldrb
    seq=Jump, brch=indexed_length_ready
indexed_method_fields:
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=63, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=1, alu=Add, rb=3, ldrb
    ; Pointer access to header/literals uses the same physical indexing as Array.
    d=0, r=Bus, rb=2, ldrb
indexed_fixed_zero:
    d=0, r=Bus, rb=7, ldrb
; R3 is logical total length, R7 fixed count. Size returns their difference.
; For access, save fixed count in R11 because positive_value may overwrite R7.
indexed_length_ready:
    ra=3, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=10, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_size
    ; Save fixed count in R11 while the positive-integer helper uses R7.
    ra=7, rb=11, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, rb=10, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=indexed_index_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
indexed_index_ready:
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=4, rb=11, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
    ra=3, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ; Version 2 CompiledMethod headers are immutable after newMethod:header:.
    ; objectAt:put: can replace literals, but must not reinterpret the boundary
    ; between tagged literals and raw bytecodes by replacing the header.
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_translate
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
indexed_translate:
    ra=4, s=Branch, brch=1, alu=Add, rb=11, ldrb
    ra=2, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_access
    ; CompiledMethod bytecodes retain guest byte offsets. The reserved prefix
    ; has no byte view in this port: literals can contain 37-bit identities.
    ; objectAt: reads headers/literals; objectAt:put: replaces only literals.
    read=Vr, vr=6
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=63, alu=And, rb=7, ldrb
    ra=7, shift=Left, rb=6, ldrb
    ra=6, s=Branch, brch=3, alu=Add, rb=6, ldrb
    ra=4, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=4, rb=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=11, ldrb
; The class/specification and integer checks changed selection. Always refetch
; VR6 before reading the translated component. R10 chooses read versus write.
indexed_access:
    ra=10, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_write
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    mem=Read
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_pointer_result
    d=Object, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=63, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_character_result
    seq=Jump, brch=positive_result
indexed_pointer_result:
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
indexed_character_result:
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=0xa000000019, page=Fetch
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
indexed_size:
    ra=3, rb=7, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=4, ldrb, seq=Jump, brch=positive_result
; Root the last caller argument before examining Character/digit representation.
; Do not write receiver storage until all type, index, and value checks finish.
indexed_write:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_write_pointer
    read=Vr, vr=3
    d=Object, ldsym
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=indexed_numeric_value
    d=Symbol, page=Fetch
    read=Type
    d=0xa000000014, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
indexed_numeric_value:
    d=indexed_numeric_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
indexed_numeric_ready:
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=indexed_numeric_store
    ra=4, s=Branch, brch=255, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
indexed_numeric_store:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    d=Register, ra=4, mem=Write
    seq=Jump, brch=indexed_write_result
indexed_write_pointer:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
indexed_write_result:
    read=Vr, vr=3
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

; Decode a guest positive 16-bit integer in SYMBOL into R4; return through R6.
; R7 is scratch. A LargePositiveInteger has one or two little-endian byte fields.
positive_value:
    d=Symbol, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=positive_large
    ra=4, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=4, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=Register, ra=6, seq=Bus
; Require exactly descriptor+one/two digits. Size and raw descriptor must
; agree; full 40-bit equality prevents a reference/tag from passing as a byte.
positive_large:
    d=Symbol, page=Fetch
    read=Type
    d=0xa00000000e, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    read=Size
    d=Object, r=Bus, rb=7, ldrb
    ra=7, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=7, s=Branch, brch=3, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ; Class identity alone does not prove byte format. Require the descriptor
    ; to match the physical length before accepting either digit. Compare the
    ; full tagged words too: low payload bits of a reference are not a digit.
    read=Representation
    d=Object, ldsym
    ra=7, shift=Left, rb=4, ldrb
    ra=4, shift=Left, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=7, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_return
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb
    d=Register, ra=7, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=7, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, shift=Left, rb=7, ldrb
    ra=7, rb=4, alu=Add, ldrb
positive_return:
    d=Register, ra=6, seq=Bus

; Return unsigned R4 as a guest integer. This allocates a LargePositiveInteger
; when the value exceeds the SmallInteger range, retaining all roots across GC.
positive_result:
    ra=4, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=positive_allocate
    ra=4, estk=Compact, compact=2
    d=Estk, ldvr, vr=5, seq=Jump, brch=send_result
positive_allocate:
    d=0xa00000000e, page=Allocate, size=3, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=10, mem=Write
    idx=Increment
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    idx=Increment
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    d=Register, ra=4, mem=Write
    seq=Jump, brch=send_result

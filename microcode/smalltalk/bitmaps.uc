; Primitive registration and shared presentation transport.
; 101 beCursor and 102 beDisplay take no arguments and return the Form receiver.
; Nonzero dimensions must fit positive SmallInteger range. Cursor dimensions
; must equal its device capabilities; display dimensions may be at most maxima.
; Malformed operands take R15 fallback before publication. Device I/O errors
; fault the machine; they are not a request to run the guest method twice.
;
; Flow: decode Form -> validate all used bitmap words -> check capability ->
; ensure private state -> bitmap_upload -> publish -> retain Form in root29.
; Component 38 is cursor registration, 39 display. Full uploads preserve caller
; R8/R9; R10..R14 are scratch caches later restored by send_result.
;
; Shared helpers bitmap_upload/bitmap_patch take R0 bank (0x410 or 0x510),
; R2 width, R3 height, VR4 bits, R10 source stride, R11 device stride, R15 return.
; R1 survives for BitBlt's registration cursor. Patch also requires BitBlt frame
; slots 5..8; it clobbers R8/R9 and slot22. R6=0 replacement, R6=2 patch.
; Pair high16 then low16, zero any absent half and mask final padding. Writes
; go to staged device storage; control 1 publishes atomically after all rows.
; After primitive validation has committed to upload, malformed transferred data
; stops at bad_state. BitBlt invokes these helpers only after drawing completes.
;
; Cursor/display registration (101/102). The receiver is a pointer Form whose
; first fields are bits, width, height, offset. Offset belongs to guest drawing
; policy and is not a hardware hotspot. Compatible Form subclasses work without
; embedding their identities in RTL. Bitmap storage must have word format.
;
; Validate the complete used source before allocation or device writes. Guest
; rows contain ceil(width/16) raw 16-bit words. The peripheral consumes rows of
; ceil(width/32) raw 32-bit words, most significant pixel first, with zero padding.
;
; VR6 roots the Form, VR4 its bits, VR7 the fallback method. R1 stays zero for
; fallback/result stack handling; R8/R9 retain IP/SP and R15 the failure target.
; R10..R14 are expendable method caches restored by send_result/load_context.
; R0 device bank; R2/R3 dimensions; R4 packed pixels; R5 source component;
; R6 validation limit, then upload mode; R7 device/component scratch; R10/R11 source/device stride;
; R12 rows left; R13 source words left in row; R14 last-word pixel mask.
primitive_bitmap:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=7, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=3, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=3, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=2, s=Branch, brch=15, alu=Add, rb=10, ldrb
    ra=10, s=Branch, brch=28, alu=Rotate, rb=10, ldrb
    ra=10, s=Branch, brch=2047, alu=And, rb=10, ldrb
    ra=2, s=Branch, brch=31, alu=Add, rb=11, ldrb
    ra=11, s=Branch, brch=27, alu=Rotate, rb=11, ldrb
    ra=11, s=Branch, brch=1023, alu=And, rb=11, ldrb
    ra=10, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=6, ldrb
    ra=6, s=Branch, brch=1, alu=Add, rb=6, ldrb
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=7, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Size
    d=Object, r=Bus, s=Register, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=2, r=Bus, rb=5, ldrb
bitmap_validate_word:
    d=Register, ra=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=5, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bitmap_capabilities
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    seq=Jump, brch=bitmap_validate_word
bitmap_capabilities:
    ra=0, s=Branch, brch=101, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_display_bank
    d=0x410, r=Bus, rb=0, ldrb
    ra=0, io=Read
    ra=2, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=3, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=bitmap_validated
bitmap_display_bank:
    d=0x510, r=Bus, rb=0, ldrb
    ra=0, io=Read
    ra=2, s=Bus, d=Device, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=0, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=3, s=Bus, d=Device, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
bitmap_validated:
    ; Lazy runtime state shares a root with buffered input. Its allocator uses
    ; R0/VR3, so retain the device bank in R12 and the bits in VR4 across it.
    ra=0, rb=12, ldrb
    d=bitmap_stage, r=Bus, rb=7, ldrb, seq=Jump, brch=input_buffer_load
bitmap_stage:
    ra=12, rb=0, ldrb
    d=bitmap_registered, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_upload
; Common upload also serves BitBlt refresh. Caller has validated the Form and
; device registration; packing validates every transferred word before publish.
; R0=bank, R2/R3=dimensions, VR4=bits,
; R10/R11=16/32-bit strides, R15=return address. R1 survives both paths.
; Full uploads preserve R8/R9; BitBlt saves them before a partial upload.
bitmap_patch:
    d=2, r=Bus, rb=6, ldrb, seq=Jump, brch=bitmap_begin
bitmap_upload:
    d=0, r=Bus, rb=6, ldrb
bitmap_begin:
    ra=0, s=Branch, brch=8, alu=Add, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=3, ldq
    ra=7, d=Q, io=Write
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=11, ldq
    ra=7, d=Q, io=Write
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=6, ldq
    ra=7, d=Q, io=Write ; full replacement (0) or sparse patch (2)
    ; Last output word uses its high (width modulo 32) bits. A full word uses
    ; all bits. NUMERIK rotates a one to construct the partial-word mask.
    ra=2, s=Branch, brch=31, alu=And, rb=14, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bitmap_full_mask
    d=1, r=Bus, s=Register, rb=14, alu=Rotate, ldq
    r=Q, s=Bus, d=1, alu=Sub, cin=One, rb=4, ldrb
    ra=14, s=Branch, brch=32, alu=SubReverse, cin=One, rb=14, ldrb
    ra=4, rb=14, alu=Rotate, ldq
    d=Q, r=Bus, rb=14, ldrb
    seq=Jump, brch=bitmap_transfer
bitmap_full_mask:
    d=0xffffffff, r=Bus, rb=14, ldrb
bitmap_transfer:
    read=Vr, vr=4
    d=Object, page=Fetch
    ra=6, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_patch_transfer
    ra=3, rb=12, ldrb
    d=2, r=Bus, rb=5, ldrb
bitmap_row:
    ra=10, rb=13, ldrb
bitmap_pair:
    d=Register, ra=5, idx=Load
    mem=Read
    ; Partial uploads read only changed 32-bit groups, including their edge
    ; neighbors. Reject malformed raw words before publishing any device data.
    d=Object, ldsym, r=Bus, rb=4, ldrb
    d=Register, ra=4, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=4, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    ra=4, s=Branch, brch=16, alu=Rotate, rb=4, ldrb
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=13, s=Branch, brch=1, alu=Sub, cin=One, rb=13, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bitmap_last_word
    d=Register, ra=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb
    d=Register, ra=7, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=7, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    ra=4, s=Bus, d=Object, alu=Or, rb=4, ldrb
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=13, s=Branch, brch=1, alu=Sub, cin=One, rb=13, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_write_word
bitmap_last_word:
    ra=4, rb=14, alu=And, ldq
    d=Q, r=Bus, rb=4, ldrb
bitmap_write_word:
    ra=0, s=Branch, brch=28, alu=Add, rb=7, ldrb
    ra=4, ldq
    ra=7, d=Q, io=Write
    ra=13, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_pair
    ra=6, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_patch_next
    ra=12, s=Branch, brch=1, alu=Sub, cin=One, rb=12, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_row
bitmap_publish:
    ra=0, s=Branch, brch=20, alu=Add, rb=7, ldrb
    ra=7, d=1, io=Write ; atomic visible publication
    d=Register, ra=15, seq=Bus
bitmap_registered:
    ; Retain the Form, not a movable body address. Later drawing/refresh paths
    ; can resolve this reference again after eviction, become:, or collection.
    d=29, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=Symbol, page=Fetch
    d=38, r=Bus, rb=7, ldrb
    ra=0, s=Bus, d=0x410, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bitmap_root
    d=39, r=Bus, rb=7, ldrb
bitmap_root:
    d=Register, ra=7, idx=Load
    read=Vr, vr=6
    d=Object, mem=Write, ldvr, vr=5
    seq=Jump, brch=send_result

; Patch rectangle comes from BitBlt's clipped frame slots 5..8. R8 is the
; current row, R9 the first 32-bit column, R12 rows left, slot22 source words
; per row. R6=2 selects this path in the shared packing loop. No guest object
; is allocated, and edge groups retain their unchanged neighboring pixels.
bitmap_patch_transfer:
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=27, alu=Rotate, rb=9, ldrb
    ra=9, s=Branch, brch=1023, alu=And, rb=9, ldrb
    d=7, esp=Bus
    estk=Read
    ra=4, s=Bus, d=Estk, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=31, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=27, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=1023, alu=And, rb=4, ldrb
    ra=4, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bitmap_patch_masked
    d=0xffffffff, r=Bus, rb=14, ldrb
bitmap_patch_masked:
    ra=4, shift=Left, rb=4, ldrb
    ra=4, rb=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bitmap_patch_word_limit
    ra=10, rb=4, ldrb
bitmap_patch_word_limit:
    ra=9, shift=Left, rb=7, ldrb
    ra=4, rb=7, alu=Sub, cin=One, ldq
    d=22, esp=Bus
    d=Q, estk=Bus
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=12, ldrb
bitmap_patch_row:
    ra=8, rb=11, alu=MultiplyUnsigned
    alu=ProductLow, rb=4, ldrb
    ra=4, rb=9, alu=Add, ldq
    ra=0, s=Branch, brch=24, alu=Add, rb=7, ldrb
    ra=7, d=Q, io=Write
    ra=8, rb=10, alu=MultiplyUnsigned
    alu=ProductLow, rb=5, ldrb
    ra=9, shift=Left, rb=7, ldrb
    ra=5, rb=7, alu=Add, ldq
    r=Q, s=Branch, brch=2, alu=Add, rb=5, ldrb
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=13, ldrb, seq=Jump, brch=bitmap_pair
bitmap_patch_next:
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    ra=12, s=Branch, brch=1, alu=Sub, cin=One, rb=12, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_patch_row
    seq=Jump, brch=bitmap_publish

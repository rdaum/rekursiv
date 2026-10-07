; BitBlt primitive 96. All decoding, clipping, source staging, Boolean
; operations and device refresh execute here. OBJEKT has no Form knowledge.
;
; ESTK is a private rooted frame during this primitive (SP=31). Slot 0 is
; validation scratch. 1..3 Forms; 4 rule; 5..14 the ten rectangle fields;
; 15/16 destination/source strides; 17..20 Form dimensions; 21 staged source;
; 22 words/row; 23 first destination word; 24 pass (0 validate, 1 direct, 2 snapshot); 25/26 edge masks;
; 27/28 saved IP/SP; 29 failure continuation; 30 Form validation continuation;
; 31 staged word count. VR0..2 retain caller state, VR6 receiver, VR7 method.
; VR3/4/5 hold destination/source/halftone bitmaps. No reference lives solely
; in a numeric register. Every refill can page or enter the same collector.
;
; Header and accessed-word validation precede destination writes. Disjoint
; bitmaps copy directly without allocating. A source or halftone alias uses
; a temporary opaque object, so overlap never reads an overwritten source.
; Empty rectangles need neither allocation nor publication. Input fields and
; Forms stay unchanged. Execution is single-mutator: paging and collection
; can occur between passes, but no guest process can mutate the checked words.
; Entry: VR6 BitBlt receiver, VR7 primitive method, R1 arity, R8/R9 caller
; IP/SP, R15 fallback continuation. Success returns VR6 unchanged; failure
; restores caller state before executing the original method fallback.
primitive_bitblt:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=27, esp=Bus
    ra=8, estk=Alu
    d=28, esp=Bus
    ra=9, estk=Alu
    d=29, esp=Bus
    ra=15, estk=Alu
    d=bb_failed, r=Bus, rb=15, ldrb
    d=31, sp=Bus
    d=0, esp=Bus
    read=Vr, vr=6
    d=Object, ldsym
    d=SymbolHigh, r=Bus, s=Branch, brch=192, alu=And, rb=6, ldrb
    ra=6, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=Symbol, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=15, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    read=Representation
    d=Object, r=Bus, s=Branch, brch=7, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=2, idx=Load
    mem=Read
    d=1, esp=Bus
    d=Object, estk=Bus
    d=3, idx=Load
    mem=Read
    d=2, esp=Bus
    d=Object, estk=Bus
    d=4, idx=Load
    mem=Read
    d=3, esp=Bus
    d=Object, estk=Bus
    ; Decode the eleven numeric fields with one loop. R0 names their frame
    ; slot; the corresponding receiver component is R0+1. Numeric registers
    ; retain sign-extended payloads, and the frame retains the original tags.
    d=4, r=Bus, rb=0, ldrb
bb_fields:
    ra=0, s=Branch, brch=1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=0, esp=Bus
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=2, s=Bus, d=16384, alu=Add, rb=3, ldrb
    ra=3, s=Bus, d=0xffff8000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=Register, ra=0, esp=Bus
    d=Object, estk=Bus
    ra=0, s=Branch, brch=1, alu=Add, rb=0, ldrb
    ra=0, s=Branch, brch=15, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_fields
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=15, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    d=bb_dest_ready, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_form
bb_dest_ready:
    d=Object, ldvr, vr=3
    d=15, esp=Bus
    ra=4, estk=Alu
    d=17, esp=Bus
    ra=2, estk=Alu
    d=18, esp=Bus
    ra=3, estk=Alu
    d=2, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=4
    d=NIL, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_no_source
    d=Object, ldvr, vr=3
    d=bb_source_ready, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_form
bb_source_ready:
    d=Object, ldvr, vr=4
    d=16, esp=Bus
    ra=4, estk=Alu
    d=19, esp=Bus
    ra=2, estk=Alu
    d=20, esp=Bus
    ra=3, estk=Alu
bb_no_source:
    d=3, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=5
    d=NIL, ldsym
    read=Vr, vr=5
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_forms_ready
    d=Object, ldvr, vr=3
    d=bb_halftone_ready, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_form
bb_halftone_ready:
    d=Object, ldvr, vr=5
    ra=2, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    ra=3, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
; Validation reused VR3. Resolve the destination bitmap again before clipping.
bb_forms_ready:
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    d=0, r=Bus, rb=0, ldrb
    d=bb_clip_y, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_clip
bb_clip_y:
    d=1, r=Bus, rb=0, ldrb
    d=bb_clipped, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_clip
; Destination words span floor(dx/16)..ceil((dx+width)/16). Edge masks
; select only covered pixels; the final physical word may contain padding.
bb_clipped:
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, s=Branch, brch=28, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=2047, alu=And, rb=4, ldrb
    d=23, esp=Bus
    ra=4, estk=Alu
    ra=2, rb=3, alu=Add, ldq
    r=Q, s=Branch, brch=15, alu=Add, rb=6, ldrb
    ra=6, s=Branch, brch=28, alu=Rotate, rb=6, ldrb
    ra=6, s=Branch, brch=2047, alu=And, rb=6, ldrb
    ra=6, rb=4, alu=Sub, cin=One, ldq
    r=Q, rb=6, ldrb
    d=22, esp=Bus
    ra=6, estk=Alu
    ra=2, s=Branch, brch=15, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=32, alu=SubReverse, cin=One, rb=4, ldrb
    d=65535, r=Bus, s=Register, rb=4, alu=Rotate, ldq
    r=Q, s=Bus, d=65535, alu=And, rb=4, ldrb
    d=25, esp=Bus
    ra=4, estk=Alu
    ra=2, rb=3, alu=Add, ldq
    r=Q, s=Branch, brch=15, alu=And, rb=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bb_last_full
    ra=4, s=Branch, brch=16, alu=SubReverse, cin=One, rb=4, ldrb
    d=65535, r=Bus, s=Register, rb=4, alu=Rotate, ldq
    r=Q, s=Bus, d=65535, alu=And, rb=4, ldrb
    seq=Jump, brch=bb_mask_ready
bb_last_full:
    d=65535, r=Bus, rb=4, ldrb
bb_mask_ready:
    d=26, esp=Bus
    ra=4, estk=Alu
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=6, alu=MultiplyUnsigned
    alu=ProductLow, rb=2, ldrb
    d=31, esp=Bus
    ra=2, estk=Alu
    ; Pass 0 checks only words read by this rectangle, before any writes.
    ; Pass 1 draws directly. Pass 2 snapshots sources only when bits alias.
    d=24, esp=Bus
    d=0, estk=Bus
bb_pass_start:
    d=0, r=Bus, rb=8, ldrb
    d=2, r=Bus, rb=10, ldrb
bb_stage_row:
    ; Cache numeric row bases once, instead of rereading frame slots and
    ; multiplying for every word. R0=destination component, R1=source word.
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=8, alu=Add, ldq
    r=Q, rb=2, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=0, ldrb
    d=23, esp=Bus
    estk=Read
    ra=0, s=Bus, d=Estk, alu=Add, rb=0, ldrb
    ra=0, s=Branch, brch=2, alu=Add, rb=0, ldrb
    d=10, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=8, alu=Add, ldq
    r=Q, rb=2, ldrb
    d=16, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=1, ldrb
    d=0, r=Bus, rb=9, ldrb
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=8, alu=Add, ldq
    r=Q, s=Branch, brch=15, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=2, alu=Add, rb=2, ldrb
    d=65535, r=Bus, rb=14, ldrb
    d=NIL, ldsym
    read=Vr, vr=5
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_stage_word
    d=Object, page=Fetch
    d=Register, ra=2, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=14, ldrb
    d=Register, ra=14, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=14, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
bb_stage_word:
    d=65535, r=Bus, rb=12, ldrb
    d=NIL, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_stage_mask
    d=Object, page=Fetch
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=9, alu=Add, ldq
    r=Q, s=Branch, brch=4, alu=Rotate, rb=2, ldrb
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=Sub, cin=One, ldq
    r=Q, rb=2, ldrb
    d=9, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=Add, ldq
    r=Q, rb=2, ldrb
    ra=2, s=Branch, brch=15, alu=And, rb=13, ldrb
    ra=2, shift=ArithmeticRight, rb=2, ldrb
    ra=2, shift=ArithmeticRight, rb=2, ldrb
    ra=2, shift=ArithmeticRight, rb=2, ldrb
    ra=2, shift=ArithmeticRight, rb=11, ldrb
    d=16, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=1, rb=6, ldrb
    d=bb_source_first, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_source_word
; Assemble two adjacent source words into a 32-bit window. Rotate by
; sourceX mod 16, then select its high half. Out-of-row words are zero;
; clipping and edge masks ensure those extra bits never affect the result.
bb_source_first:
    ra=2, s=Branch, brch=16, alu=Rotate, rb=12, ldrb
    ra=11, s=Branch, brch=1, alu=Add, rb=11, ldrb
    d=bb_source_second, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_source_word
bb_source_second:
    ra=12, rb=2, alu=Or, ldq
    r=Q, rb=12, ldrb
    ra=12, rb=13, alu=Rotate, ldq
    r=Q, s=Branch, brch=16, alu=Rotate, rb=12, ldrb
    ra=12, s=Bus, d=65535, alu=And, rb=12, ldrb
bb_stage_mask:
    ra=12, rb=14, alu=And, ldq
    r=Q, rb=12, ldrb
    d=24, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_stage_store
    ; Resolve the destination word for either preflight or the direct path.
    ra=0, rb=9, alu=Add, ldq
    r=Q, rb=11, ldrb
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb
    ra=2, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_direct_merge
    d=Register, ra=3, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=3, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    seq=Jump, brch=bb_stage_next
bb_direct_merge:
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=13, ldrb
    ra=13, s=Bus, d=bb_rule0, alu=Add, rb=13, ldrb
    ra=12, rb=2, ldrb
    d=Register, ra=13, seq=Bus
bb_stage_store:
    d=21, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=10, idx=Load
    d=Register, ra=12, mem=Write
bb_stage_next:
    ra=10, s=Branch, brch=1, alu=Add, rb=10, ldrb
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=9, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_stage_word
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=8, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_stage_row
    d=24, esp=Bus
    estk=Read
    d=Estk, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=bb_preflight_done
    d=Estk, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_refresh
    seq=Jump, brch=bb_staged
bb_preflight_done:
    ; Compare storage identities, not Form identities. This also covers a
    ; halftone sharing destination storage. Nonaliasing copies need no object.
    read=Vr, vr=3
    d=Object, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_stage_allocate
    read=Vr, vr=5
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_stage_allocate
    d=24, esp=Bus
    d=1, estk=Bus, seq=Jump, brch=bb_pass_start
bb_stage_allocate:
    d=24, esp=Bus
    d=2, estk=Bus
    d=31, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ; Allocate a word-format scratch object sized to the clipped word grid.
    ; Its class follows the destination bitmap; its body contains no pointers.
    ; The allocation can collect: Forms and source bitmaps remain rooted.
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    ra=2, s=Branch, brch=1, alu=Add, rb=2, ldrb
    d=Object, ra=2, page=Allocate, size=ra, scan=0
    d=21, esp=Bus
    d=Object, estk=Bus
    d=1, idx=Load
    d=1, mem=Write
    seq=Jump, brch=bb_pass_start
bb_staged:
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    d=21, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=4
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=13, ldrb
    ra=13, s=Bus, d=bb_rule0, alu=Add, rb=13, ldrb
    d=0, r=Bus, rb=8, ldrb
    d=2, r=Bus, rb=10, ldrb
; R8/R9 row/column, R10 scratch component, R11 destination component.
; R13 selects a three-instruction Boolean rule using the original 0..15 order.
bb_merge_row:
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=8, alu=Add, ldq
    r=Q, rb=2, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=11, ldrb
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=11, rb=2, alu=Add, ldq
    r=Q, s=Branch, brch=2, alu=Add, rb=11, ldrb
    d=0, r=Bus, rb=9, ldrb
bb_merge_word:
    read=Vr, vr=4
    d=Object, page=Fetch
    d=Register, ra=10, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=11, idx=Load
    mem=Read
    d=Object, r=Bus, rb=3, ldrb
    d=Register, ra=13, seq=Bus
bb_rule0:
    d=0, r=Bus, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule1:
    ra=2, rb=3, alu=And, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule2:
    ra=3, alu=Not, rb=4, ldrb
    ra=2, rb=4, alu=And, ldq
    seq=Jump, brch=bb_merged
bb_rule3:
    ra=2, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule4:
    ra=2, alu=Not, rb=4, ldrb
    ra=3, rb=4, alu=And, ldq
    seq=Jump, brch=bb_merged
bb_rule5:
    ra=3, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule6:
    ra=2, rb=3, alu=Xor, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule7:
    ra=2, rb=3, alu=Or, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule8:
    ra=2, rb=3, alu=Or, ldq
    r=Q, alu=Not, ldq
    seq=Jump, brch=bb_merged
bb_rule9:
    ra=2, rb=3, alu=Xor, ldq
    r=Q, alu=Not, ldq
    seq=Jump, brch=bb_merged
bb_rule10:
    ra=3, alu=Not, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule11:
    ra=3, alu=Not, rb=4, ldrb
    ra=2, rb=4, alu=Or, ldq
    seq=Jump, brch=bb_merged
bb_rule12:
    ra=2, alu=Not, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_rule13:
    ra=2, alu=Not, rb=4, ldrb
    ra=3, rb=4, alu=Or, ldq
    seq=Jump, brch=bb_merged
bb_rule14:
    ra=2, rb=3, alu=And, ldq
    r=Q, alu=Not, ldq
    seq=Jump, brch=bb_merged
bb_rule15:
    d=65535, r=Bus, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_merged:
    r=Q, rb=4, ldrb
    d=65535, r=Bus, rb=5, ldrb
    ra=9, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_not_first
    d=25, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
bb_not_first:
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=6, ldrb
    ra=9, s=Branch, brch=1, alu=Add, rb=2, ldrb
    ra=2, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_masked
    d=26, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=6, ldrb
    ra=5, rb=6, alu=And, ldq
    r=Q, rb=5, ldrb
bb_masked:
    ; D XOR ((new XOR D) AND writeMask) preserves untouched edge pixels.
    ra=4, rb=3, alu=Xor, ldq
    r=Q, s=Register, rb=5, alu=And, ldq
    r=Q, s=Register, rb=3, alu=Xor, ldq
    d=Q, mem=Write
    d=24, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_stage_next
    ra=10, s=Branch, brch=1, alu=Add, rb=10, ldrb
    ra=11, s=Branch, brch=1, alu=Add, rb=11, ldrb
    ra=9, s=Branch, brch=1, alu=Add, rb=9, ldrb
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=9, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_merge_word
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=8, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_merge_row
    seq=Jump, brch=bb_refresh
; Shared Form validator: input VR3, return address R7, failure address R15.
; Returns Object/VR3=bits, R2/R3=width/height, R4=16-bit stride. R5..7 and
; slot 0 are scratch; slot 30 retains the return address during word scanning.
; Validate layout and storage bounds here. The rectangle pass checks pixel
; words later, after clipping. Unrelated bitmap contents are not inspected.
bb_form:
    d=30, esp=Bus
    ra=7, estk=Alu
    read=Vr, vr=3
    d=Object, ldsym
    d=SymbolHigh, r=Bus, s=Branch, brch=192, alu=And, rb=6, ldrb
    ra=6, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=Symbol, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    read=Representation
    d=Object, r=Bus, s=Branch, brch=7, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=3, idx=Load
    mem=Read
    d=0, esp=Bus
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=3, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=3, ldsym
    d=SymbolHigh, r=Bus, s=Branch, brch=192, alu=And, rb=6, ldrb
    ra=6, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    d=Symbol, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=7, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    ra=2, s=Branch, brch=15, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=28, alu=Rotate, rb=4, ldrb
    ra=4, s=Branch, brch=2047, alu=And, rb=4, ldrb
    ra=4, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=6, ldrb
    ra=6, s=Branch, brch=1, alu=Add, rb=6, ldrb
    read=Size
    d=Object, r=Bus, s=Register, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_invalid
    seq=Jump, brch=bb_form_done
; Full validation is reserved for a refresh whose registered geometry differs
; from the destination Form. Ordinary copies validate only accessed words.
bb_full_pixels:
    d=30, esp=Bus
    ra=7, estk=Alu
    d=1, r=Bus, rb=5, ldrb
bb_validate:
    ra=5, rb=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_form_done
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    d=Register, ra=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb
    d=Register, ra=7, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=7, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
    seq=Jump, brch=bb_validate
bb_form_done:
    d=30, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=7, ldrb
    read=Vr, vr=3
    d=Register, ra=7, seq=Bus
; Clip one axis (R0=0 for x, 1 for y), with R7 as return address.
; Intersect parameter t with [0, extent), destination bounds, clip bounds,
; and source bounds when present. R12/R13 accumulate lower/upper limits.
; Adjust destination and source by the same lower limit; store the new extent.
; Inputs are bounded to 15 signed bits, so intermediate arithmetic fits i32.
bb_clip:
    ra=0, s=Branch, brch=5, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=0, s=Branch, brch=9, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=0, s=Branch, brch=7, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=11, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=0, s=Branch, brch=13, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=6, ldrb
    ra=0, s=Branch, brch=17, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=19, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=11, ldrb
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    ra=6, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    ra=6, rb=5, alu=Add, ldq
    r=Q, rb=6, ldrb
    d=0, r=Bus, rb=12, ldrb
    ra=4, rb=13, ldrb
    ra=2, s=Branch, brch=0, alu=SubReverse, cin=One, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_dest_low_done
    ra=14, rb=12, ldrb
bb_dest_low_done:
    ra=5, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_clip_low_done
    ra=14, rb=12, ldrb
bb_clip_low_done:
    ra=10, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_dest_high_done
    ra=14, rb=13, ldrb
bb_dest_high_done:
    ra=6, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_clip_high_done
    ra=14, rb=13, ldrb
bb_clip_high_done:
    d=NIL, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_clip_finish
    ra=3, s=Branch, brch=0, alu=SubReverse, cin=One, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_src_low_done
    ra=14, rb=12, ldrb
bb_src_low_done:
    ra=11, rb=3, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_src_high_done
    ra=14, rb=13, ldrb
bb_src_high_done:
bb_clip_finish:
    ra=13, rb=12, alu=Sub, cin=One, ldq, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    r=Q, rb=4, ldrb
    ra=2, rb=12, alu=Add, ldq
    r=Q, rb=2, ldrb
    ra=3, rb=12, alu=Add, ldq
    r=Q, rb=3, ldrb
    ra=0, s=Branch, brch=5, alu=Add, ldq
    d=Q, esp=Bus
    ra=2, estk=Alu
    ra=0, s=Branch, brch=9, alu=Add, ldq
    d=Q, esp=Bus
    ra=3, estk=Alu
    ra=0, s=Branch, brch=7, alu=Add, ldq
    d=Q, esp=Bus
    ra=4, estk=Alu
    d=Register, ra=7, seq=Bus
; Read source word R11 from the selected bitmap, using row base R6 and
; stride R3. Return raw word R2 (zero outside the row) through R7.
bb_source_word:
    d=0, r=Bus, rb=2, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Sign, brch=bb_source_zero
    ra=11, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bb_source_zero
    ra=6, rb=11, alu=Add, ldq
    r=Q, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb
    d=Register, ra=2, seq=ConditionalJump, cc=!Symbol, brch=bb_invalid
    ra=2, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_invalid
bb_source_zero:
    d=Register, ra=7, seq=Bus
; Explicit presentation: inspect rooted cursor/display Forms after the draw.
; Match their bitmap identities, including alias Forms. Upload changed
; 32-bit groups when geometry matches, otherwise a complete frame. The
; external endpoint accepts pixel words and knows nothing about guest Forms.
; R1 scans private-state components 38/39 and survives validation/upload.
bb_refresh:
    read=Vr, vr=3
    d=21, esp=Bus
    d=Object, estk=Bus
    d=38, r=Bus, rb=1, ldrb
bb_refresh_scan:
    d=29, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_success
    d=Symbol, page=Fetch
    d=Register, ra=1, idx=Load
    mem=Read
    d=Object, ldvr, vr=3, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_refresh_next
    ; Drawing has committed. A malformed registered Form now stops the runtime,
    ; rather than falling back and applying an already completed BitBlt twice.
    d=bad_state, r=Bus, rb=15, ldrb
    d=bb_refresh_form, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_form
bb_refresh_form:
    d=Object, ldvr, vr=4
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=bb_refresh_next
    ra=4, rb=10, ldrb
    ra=2, s=Branch, brch=31, alu=Add, rb=11, ldrb
    ra=11, s=Branch, brch=27, alu=Rotate, rb=11, ldrb
    ra=11, s=Branch, brch=1023, alu=And, rb=11, ldrb
    d=0x410, r=Bus, rb=0, ldrb
    ra=1, s=Branch, brch=38, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_refresh_geometry
    d=0x510, r=Bus, rb=0, ldrb
bb_refresh_geometry:
    ; Rectangles have the destination Form's geometry. A differently shaped
    ; alias, or a changed device geometry, requires a full replacement instead.
    d=17, esp=Bus
    estk=Read
    ra=2, s=Bus, d=Estk, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    d=18, esp=Bus
    estk=Read
    ra=3, s=Bus, d=Estk, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=0, s=Branch, brch=8, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=2, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=3, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=11, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    d=bb_refresh_next, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_patch
bb_refresh_full:
    d=bb_refresh_upload, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_full_pixels
bb_refresh_upload:
    d=bb_refresh_next, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_upload
bb_refresh_next:
    ra=1, s=Branch, brch=1, alu=Add, rb=1, ldrb
    ra=1, s=Branch, brch=40, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_scan
    seq=Jump, brch=bb_success
bb_invalid:
    d=Register, ra=15, seq=Bus
bb_success:
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    d=27, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=28, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, r=Bus, rb=1, ldrb, estk=Bus
    seq=Jump, brch=send_result
bb_failed:
    d=27, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=28, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb
    d=29, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=15, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, r=Bus, rb=1, ldrb, estk=Bus
    seq=Jump, brch=primitive_failed

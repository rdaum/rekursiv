; Per-pixel colour path. R8/R9 traverse the clipped rectangle. R0 caches the
; current halftone row; R10 component, R11 shift, R13 original destination word,
; R14 pixel mask. Source operands and map lookups remain rooted in the frame.
; No new objects are allocated once drawing starts.
bb_row:
    d=0, r=Bus, rb=9, ldrb
    d=0xffffffff, r=Bus, rb=0, ldrb
    read=Vr, vr=5
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_pixel
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=8, alu=Add, ldq
    d=Q, r=Bus, rb=2, ldrb
    d=24, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
bb_halftone_modulo:
    ra=2, rb=3, alu=Sub, cin=One, ldq, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_halftone_row
    d=Q, r=Bus, rb=2, ldrb, seq=Jump, brch=bb_halftone_modulo
bb_halftone_row:
    d=Symbol, page=Fetch
    ra=2, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
bb_pixel:
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=2, rb=9, alu=Add, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=3, rb=8, alu=Add, ldq
    d=Q, r=Bus, rb=3, ldrb
    d=bb_destination_pixel, r=Bus, rb=6, ldrb, seq=Jump, brch=bb_address
bb_destination_pixel:
    d=30, esp=Bus
    d=1, estk=Bus
    ra=2, rb=10, ldrb
    ra=3, rb=11, ldrb
    ra=4, rb=14, ldrb
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=10, idx=Load
    mem=Read
    d=Object, r=Bus, rb=13, ldrb
    seq=Jump, brch=bb_try_word
bb_pixel_operands:
    ra=14, rb=12, ldrb
    read=Vr, vr=4
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_source_mapped
    d=9, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=10, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    d=16, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=2, rb=9, alu=Add, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=3, rb=8, alu=Add, ldq
    d=Q, r=Bus, rb=3, ldrb
    d=bb_source_pixel, r=Bus, rb=6, ldrb, seq=Jump, brch=bb_address
bb_source_pixel:
    read=Vr, vr=4
    d=Object, page=Fetch
    d=Register, ra=2, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    d=30, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_source_word
    ra=3, s=Branch, brch=32, alu=SubReverse, cin=One, rb=3, ldrb
    ra=2, rb=3, alu=Rotate, ldq
    r=Q, s=Register, rb=4, alu=And, ldq
    d=Q, r=Bus, rb=12, ldrb
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_source_mapped
    d=Symbol, page=Fetch
    ra=12, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=12, ldrb
    seq=Jump, brch=bb_source_mapped
bb_source_word:
    ra=2, rb=12, ldrb
bb_source_mapped:
    ra=11, s=Branch, brch=32, alu=SubReverse, cin=One, rb=4, ldrb
    ra=0, rb=4, alu=Rotate, ldq
    r=Q, s=Register, rb=14, alu=And, ldq
    r=Q, s=Register, rb=12, alu=And, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=13, rb=4, alu=Rotate, ldq
    r=Q, s=Register, rb=14, alu=And, ldq
    d=Q, r=Bus, rb=3, ldrb
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=25, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_paint
    ra=4, s=Branch, brch=26, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_mask
    ra=4, shift=Left, rb=7, ldrb
    ra=4, rb=7, alu=Add, ldq
    r=Q, s=Bus, d=bb_rule0, alu=Add, rb=7, ldrb
    d=Register, ra=7, seq=Bus
bb_paint:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=bb_next_pixel
    ra=2, ldq, seq=Jump, brch=bb_merged
bb_mask:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=bb_next_pixel
    d=0, r=Bus, ldq, seq=Jump, brch=bb_merged
; Three instructions per Boolean rule; retain table padding.
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
    ra=3, ldq
    seq=Continue
    seq=Jump, brch=bb_merged
bb_merged:
    r=Q, s=Register, rb=14, alu=And, ldq
    r=Q, s=Register, rb=11, alu=Rotate, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=14, rb=11, alu=Rotate, ldq
    r=Q, alu=Not, ldq
    r=Q, s=Register, rb=13, alu=And, ldq
    r=Q, s=Register, rb=2, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=2, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_next_pixel
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=10, idx=Load
    d=Register, ra=2, mem=Write
bb_next_pixel:
    d=30, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=9, alu=Add, ldrb
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=9, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_pixel
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=8, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
    seq=Jump, brch=bb_refresh

; R2 x, R3 y, R4 depth, R5 stride -> R2 component, R3 shift, R4 mask.
; R6 return. Clobbers R5/R7/Q/PRODUCT, no rooted references.
bb_address:
    ra=2, rb=4, alu=MultiplyUnsigned
    alu=ProductLow, rb=7, ldrb
    ra=3, rb=5, alu=MultiplyUnsigned
    alu=ProductLow, rb=2, ldrb
    ra=7, s=Branch, brch=27, alu=Rotate, rb=5, ldrb
    ra=5, s=Bus, d=0x07ffffff, alu=And, rb=5, ldrb
    ra=5, rb=2, alu=Add, ldrb
    ra=2, s=Branch, brch=3, alu=Add, rb=2, ldrb
    ra=7, s=Branch, brch=31, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=32, alu=SubReverse, cin=One, rb=3, ldrb
    ra=3, rb=4, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=3, ldrb
    ra=4, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_mask32
    d=1, r=Bus, s=Register, rb=4, alu=Rotate, ldq
    r=Q, s=Branch, brch=1, alu=Sub, cin=One, rb=4, ldrb
    d=Register, ra=6, seq=Bus
bb_mask32:
    d=0xffffffff, r=Bus, rb=4, ldrb
    d=Register, ra=6, seq=Bus

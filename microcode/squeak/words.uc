; Word-at-a-time Boolean raster kernel, selected once after validation/clipping
; and source/halftone alias snapshots. No map; source is absent or same depth.
; Source and destination may have different word alignment. Edge masks preserve
; pixels outside the clipped rectangle; a source read crosses a word boundary
; only when a requested pixel needs that word. All object identities stay rooted
; in VR3/4/5, never in numeric address caches. Re-fetch after any other object.
;
; Frame31 is the row continuation (bb_pixel / bb_glyph_row / bb_word_row).
; R1 caches the Boolean-rule entry across rows. R0 is the current halftone row.
bb_choose_words:
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=1, ldrb
    ra=1, s=Branch, brch=15, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_row
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    d=NIL, seq=ConditionalJump, cc=!Symbol, brch=bb_row
    read=Vr, vr=4
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_words_ready
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
bb_words_ready:
    ra=1, shift=Left, rb=1, ldrb
    ra=1, s=Branch, brch=bb_word_rule0, alu=Add, rb=1, ldrb
    d=31, esp=Bus
    d=bb_word_row, estk=Bus, seq=Jump, brch=bb_row

; Build row addresses once. R4/R6 are physical source/destination components;
; R5/R7 are MSB-first bit offsets. R5=64 denotes no source. R9 is bits left.
; R11 caches the current source word, surviving destination fetches/refills.
bb_word_row:
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=13, ldrb
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=13, alu=MultiplyUnsigned
    alu=ProductLow, rb=7, ldrb
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=8, rb=3, alu=Add, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=14, ldrb
    ra=3, rb=14, alu=MultiplyUnsigned
    alu=ProductLow, rb=6, ldrb
    ra=7, s=Branch, brch=27, alu=Rotate, rb=14, ldrb
    ra=14, s=Bus, d=0x07ffffff, alu=And, rb=14, ldrb
    ra=14, rb=6, alu=Add, ldrb
    ra=6, s=Branch, brch=3, alu=Add, rb=6, ldrb
    ra=7, s=Branch, brch=31, alu=And, rb=7, ldrb
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb
    ra=9, rb=13, alu=MultiplyUnsigned
    alu=ProductLow, rb=9, ldrb
    d=64, r=Bus, rb=5, ldrb
    read=Vr, vr=4
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_word
    d=9, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, rb=13, alu=MultiplyUnsigned
    alu=ProductLow, rb=5, ldrb
    d=10, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=8, rb=3, alu=Add, ldrb
    d=16, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=14, ldrb
    ra=3, rb=14, alu=MultiplyUnsigned
    alu=ProductLow, rb=4, ldrb
    ra=5, s=Branch, brch=27, alu=Rotate, rb=14, ldrb
    ra=14, s=Bus, d=0x07ffffff, alu=And, rb=14, ldrb
    ra=14, rb=4, alu=Add, ldrb
    ra=4, s=Branch, brch=3, alu=Add, rb=4, ldrb
    ra=5, s=Branch, brch=31, alu=And, rb=5, ldrb
    d=Register, ra=4, idx=Load, read=Vr, vr=4
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=11, ldrb

; R10=min(bits remaining, space in destination word), R12 positioned write mask.
; A 32-bit mask is special: NUMERIK rotation counts are modulo 32.
bb_word:
    ra=7, s=Branch, brch=32, alu=SubReverse, cin=One, rb=10, ldrb
    ra=9, rb=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bb_word_count
    ra=9, rb=10, ldrb
bb_word_count:
    d=0xffffffff, r=Bus, rb=12, ldrb
    ra=10, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_word_destination
    ra=7, rb=10, alu=Add, ldq
    r=Q, s=Branch, brch=32, alu=SubReverse, cin=One, rb=14, ldrb
    d=1, r=Bus, s=Register, rb=10, alu=Rotate, ldq
    r=Q, s=Branch, brch=1, alu=Sub, cin=One, ldq
    r=Q, s=Register, rb=14, alu=Rotate, ldq
    r=Q, rb=12, ldrb
bb_word_destination:
    d=Register, ra=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=3, ldrb
    ra=5, s=Branch, brch=64, alu=Sub, cin=One, flags
    ra=0, rb=2, ldrb, seq=ConditionalJump, cc=Zero, brch=bb_word_combine
    ra=11, rb=5, alu=Rotate, ldq
    r=Q, rb=2, ldrb
    ra=5, rb=10, alu=Add, ldq
    r=Q, s=Branch, brch=32, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_word_cross_source
    ; All requested bits belong to the cached word. Wrapped bits are masked out.
    ra=5, rb=10, alu=Add, ldq
    r=Q, rb=5, ldrb, seq=Jump, brch=bb_word_position
bb_word_cross_source:
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    d=Register, ra=4, idx=Load, read=Vr, vr=4
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=11, ldrb
    d=1, r=Bus, s=Register, rb=5, alu=Rotate, ldq
    r=Q, s=Branch, brch=1, alu=Sub, cin=One, rb=13, ldrb
    ra=13, alu=Not, rb=14, ldrb
    ra=14, rb=2, alu=And, ldrb
    ra=11, rb=5, alu=Rotate, ldq
    r=Q, s=Register, rb=13, alu=And, ldrb
    ra=13, rb=2, alu=Or, ldrb
    ra=5, rb=10, alu=Add, ldq
    r=Q, s=Branch, brch=32, alu=Sub, cin=One, rb=5, ldrb
bb_word_position:
    ra=7, s=Branch, brch=32, alu=SubReverse, cin=One, rb=14, ldrb
    ra=2, rb=14, alu=Rotate, ldq
    r=Q, s=Register, rb=0, alu=And, ldq
    r=Q, rb=2, ldrb
bb_word_combine:
    d=Register, ra=1, seq=Bus

; Q is the Boolean result. Preserve untouched pixels and skip unchanged writes.
; R1/R4..R12/R15 survive; R2/R3/R13/R14 and Q are scratch.
bb_word_merged:
    r=Q, s=Register, rb=12, alu=And, ldq
    ra=12, alu=Not, rb=13, ldrb
    ra=3, rb=13, alu=And, ldrb
    r=Q, s=Register, rb=13, alu=Or, ldq
    r=Q, s=Register, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_word_next
    d=Register, ra=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    d=Q, mem=Write
bb_word_next:
    ra=9, rb=10, alu=Sub, cin=One, ldq, flags
    r=Q, rb=9, ldrb, seq=ConditionalJump, cc=Zero, brch=bb_word_next_row
    ra=6, s=Branch, brch=1, alu=Add, rb=6, ldrb
    d=0, r=Bus, rb=7, ldrb
    ra=5, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_word
    ; At an exact source boundary, defer the next read until more pixels exist.
    d=0, r=Bus, rb=5, ldrb
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    d=Register, ra=4, idx=Load, read=Vr, vr=4
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=11, ldrb, seq=Jump, brch=bb_word
bb_word_next_row:
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
    seq=Jump, brch=bb_refresh

; Two words per rule. Single-operation entries return in the first word;
; their padding is unreachable. Preserve the existing profile's rule-15 policy.
bb_word_rule0:
    d=0, r=Bus, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule1:
    ra=2, rb=3, alu=And, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule2:
    ra=3, alu=Not, rb=13, ldrb
    ra=2, rb=13, alu=And, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule3:
    ra=2, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule4:
    ra=2, alu=Not, rb=13, ldrb
    ra=3, rb=13, alu=And, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule5:
    ra=3, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule6:
    ra=2, rb=3, alu=Xor, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule7:
    ra=2, rb=3, alu=Or, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule8:
    ra=2, rb=3, alu=Or, ldq
    r=Q, alu=Not, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule9:
    ra=2, rb=3, alu=Xor, ldq
    r=Q, alu=Not, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule10:
    ra=3, alu=Not, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule11:
    ra=3, alu=Not, rb=13, ldrb
    ra=2, rb=13, alu=Or, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule12:
    ra=2, alu=Not, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue
bb_word_rule13:
    ra=2, alu=Not, rb=13, ldrb
    ra=3, rb=13, alu=Or, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule14:
    ra=2, rb=3, alu=And, ldq
    r=Q, alu=Not, ldq, seq=Jump, brch=bb_word_merged
bb_word_rule15:
    ra=3, ldq, seq=Jump, brch=bb_word_merged
    seq=Continue

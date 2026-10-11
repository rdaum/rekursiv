; Mapped monochrome glyphs: copy rule 3, source depth 1, destination depth 8.
; Clip and clone checks have already run. Other cases keep the general path.
; Cache both map entries once, then merge bytes in registers and write each
; touched destination word once. Source words are fetched only at boundaries.
; Halftone masking uses absolute destination bit positions, as in pixels.uc.
;
; No allocation during drawing. VR3/4/5 root destination/source/halftone bits;
; frame21 roots the colour map. A map aliasing the destination cannot be cached.
; Frame26/30 cache map bytes replicated into all four lanes; frame31 holds
; the row continuation. Four source bits can then map a full destination word.
bb_choose_glyphs:
    d=31, esp=Bus
    d=bb_pixel, estk=Bus
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_choose_words
    read=Vr, vr=4
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_choose_words
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_choose_words
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_choose_words
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_choose_words
    read=Vr, vr=3
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_choose_words
    d=Symbol, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=255, alu=And, rb=2, ldrb
    d=0x01010101, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=MultiplyUnsigned
    d=26, esp=Bus
    alu=ProductLow, estk=Alu
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=255, alu=And, rb=2, ldrb
    ra=2, rb=3, alu=MultiplyUnsigned
    d=30, esp=Bus
    alu=ProductLow, estk=Alu
    d=31, esp=Bus
    d=bb_glyph_row, estk=Bus
    seq=Jump, brch=bb_row

; R0 halftone word (from bb_row), R8 row relative to the clipped rectangle.
; Compute addresses once per row; clipped coordinates are nonnegative.
bb_glyph_row:
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=8, rb=3, alu=Add, ldrb
    ra=3, rb=5, alu=MultiplyUnsigned
    alu=ProductLow, rb=6, ldrb
    ra=2, s=Branch, brch=30, alu=Rotate, rb=14, ldrb
    ra=14, s=Bus, d=0x3fffffff, alu=And, rb=14, ldrb
    ra=14, rb=6, alu=Add, ldrb
    ra=6, s=Branch, brch=3, alu=Add, rb=6, ldrb
    ra=2, s=Branch, brch=3, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=3, alu=Rotate, rb=7, ldrb
    ra=7, s=Branch, brch=24, alu=SubReverse, cin=One, rb=7, ldrb
    d=0xff, r=Bus, s=Register, rb=7, alu=Rotate, ldq
    d=Q, r=Bus, rb=12, ldrb
    d=Register, ra=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=10, ldrb
    ra=10, rb=11, ldrb
    d=9, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=10, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=16, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=8, rb=3, alu=Add, ldrb
    ra=3, rb=5, alu=MultiplyUnsigned
    alu=ProductLow, rb=4, ldrb
    ra=2, s=Branch, brch=27, alu=Rotate, rb=14, ldrb
    ra=14, s=Bus, d=0x07ffffff, alu=And, rb=14, ldrb
    ra=14, rb=4, alu=Add, ldrb
    ra=4, s=Branch, brch=3, alu=Add, rb=4, ldrb
    ra=2, s=Branch, brch=31, alu=And, rb=14, ldrb
    ra=14, s=Branch, brch=32, alu=SubReverse, cin=One, rb=5, ldrb
    d=Register, ra=4, idx=Load, read=Vr, vr=4
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, s=Register, rb=14, alu=Rotate, ldq
    d=Q, r=Bus, rb=3, ldrb
    d=26, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=1, ldrb
    d=30, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb

; R1/R2 replicated mapped bytes, R3 source with next bit at MSB, R4 source component,
; R5 source bits left, R6 destination component, R7 destination shift,
; R9 pixels left, R10 accumulated word, R11 old word, R12 positioned byte mask.
; R13/R14 scratch. R15 retains the machine-fault continuation.
; The full-word path requires four valid cached source bits. Edge bytes and
; source-boundary fragments retain the scalar path without reading ahead.
bb_glyph_chunk:
    ra=7, s=Branch, brch=24, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_glyph_pixel
    ra=9, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_glyph_pixel
    ra=5, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bb_glyph_four
bb_glyph_pixel:
    ra=3, flags
    ra=1, rb=13, ldrb, seq=ConditionalJump, cc=!Sign, brch=bb_glyph_colour
    ra=2, rb=13, ldrb
bb_glyph_colour:
    ra=13, s=Branch, brch=255, alu=And, rb=13, ldrb
    ra=13, rb=7, alu=Rotate, ldq
    r=Q, s=Register, rb=0, alu=And, ldq
    d=Q, r=Bus, rb=13, ldrb
    ra=12, alu=Not, rb=14, ldrb
    ra=14, rb=10, alu=And, ldrb
    ra=13, rb=10, alu=Or, ldrb
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bb_glyph_flush
    ra=7, flags
    seq=ConditionalJump, cc=Zero, brch=bb_glyph_flush
    ra=7, s=Branch, brch=8, alu=Sub, cin=One, rb=7, ldrb
    ra=12, s=Branch, brch=24, alu=Rotate, rb=12, ldrb
    seq=Jump, brch=bb_glyph_source_next

; Spread the four source bits into byte lanes with multiplication. Products
; have disjoint bit groups, so no carry can contaminate the selected lane bits:
; (nibble * 0x00204081) & 0x01010101, then *255 gives each lane 00 or FF.
; Select between the two replicated colours, then apply the absolute halftone.
bb_glyph_four:
    ra=3, s=Branch, brch=4, alu=Rotate, rb=13, ldrb
    ra=13, s=Branch, brch=15, alu=And, rb=13, ldrb
    d=0x00204081, r=Bus, rb=14, ldrb
    ra=13, rb=14, alu=MultiplyUnsigned
    alu=ProductLow, rb=13, ldrb
    ra=13, s=Bus, d=0x01010101, alu=And, rb=13, ldrb
    d=255, r=Bus, rb=14, ldrb
    ra=13, rb=14, alu=MultiplyUnsigned
    alu=ProductLow, rb=13, ldrb
    ra=1, rb=2, alu=Xor, ldq
    r=Q, s=Register, rb=13, alu=And, ldrb
    ra=1, rb=13, alu=Xor, ldrb
    ra=0, rb=13, alu=And, ldq
    r=Q, rb=10, ldrb
    ra=9, s=Branch, brch=4, alu=Sub, cin=One, rb=9, ldrb
    ; The common flush/advance path consumes the fourth source bit. Consume
    ; the first three here, retaining its exact-boundary/no-read-after-end rule.
    ra=5, s=Branch, brch=3, alu=Sub, cin=One, rb=5, ldrb
    ra=3, s=Branch, brch=3, alu=Rotate, rb=3, ldrb
    ra=3, s=Bus, d=0xfffffff8, alu=And, rb=3, ldrb
    seq=Jump, brch=bb_glyph_flush

; Flush full words and partial row edges alike, preserving untouched bytes.
; Avoid writes if the merged word is unchanged. OBJEKT fetches can evict each
; other: always fetch and set the component again before a destination write.
bb_glyph_flush:
    ra=10, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_glyph_flushed
    d=Register, ra=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=10, mem=Write
bb_glyph_flushed:
    ra=9, flags
    seq=ConditionalJump, cc=Zero, brch=bb_glyph_next_row
    ra=6, s=Branch, brch=1, alu=Add, rb=6, ldrb
    d=Register, ra=6, idx=Load, read=Vr, vr=3
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=10, ldrb
    ra=10, rb=11, ldrb
    d=24, r=Bus, rb=7, ldrb
    d=0xff000000, r=Bus, rb=12, ldrb
bb_glyph_source_next:
    ra=5, s=Branch, brch=1, alu=Sub, cin=One, rb=5, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bb_glyph_source_word
    ra=3, shift=Left, rb=3, ldrb, seq=Jump, brch=bb_glyph_chunk
bb_glyph_source_word:
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    d=Register, ra=4, idx=Load, read=Vr, vr=4
    d=Object, page=Fetch
    mem=Read
    d=Object, r=Bus, rb=3, ldrb
    d=32, r=Bus, rb=5, ldrb, seq=Jump, brch=bb_glyph_chunk
bb_glyph_next_row:
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=8, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
    seq=Jump, brch=bb_refresh

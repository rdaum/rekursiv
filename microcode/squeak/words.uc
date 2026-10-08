; Aligned word fast path for Boolean rules. Frame26 holds pixels per word,
; or zero when mapping/alignment requires the general pixel path. Frame30
; records the actual advance, so leading and trailing pixels retain masks.
; Source aliases have already been copied before this path can write anything.
bb_choose_words:
    d=26, esp=Bus
    d=0, estk=Bus
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=15, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_row
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    d=NIL, seq=ConditionalJump, cc=!Symbol, brch=bb_row
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    read=Vr, vr=4
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_words_depth
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=9, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=3, alu=Sub, cin=One, ldrb
    ra=3, rb=2, alu=MultiplyUnsigned
    alu=ProductLow, ldq
    r=Q, s=Branch, brch=31, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_row
bb_words_depth:
    d=32, r=Bus, rb=3, ldrb
bb_words_divide:
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_words_ready
    ra=2, shift=Right, rb=2, ldrb
    ra=3, shift=Right, rb=3, ldrb
    seq=Jump, brch=bb_words_divide
bb_words_ready:
    d=26, esp=Bus
    ra=3, estk=Alu
    seq=Jump, brch=bb_row

; Destination address and old word are already in R10/R11/R13/R14.
; Use the word only when it lies entirely within the clipped rectangle.
bb_try_word:
    d=26, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bb_pixel_operands
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Branch, brch=32, alu=SubReverse, cin=One, rb=2, ldrb
    ra=2, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_pixel_operands
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, s=Register, rb=9, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=2, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=bb_pixel_operands
    d=30, esp=Bus
    ra=5, estk=Alu
    d=0, r=Bus, rb=11, ldrb
    d=0xffffffff, r=Bus, rb=14, ldrb
    seq=Jump, brch=bb_pixel_operands

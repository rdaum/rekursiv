; Publish only the destination words touched by the clipped rectangle. The
; device preserves unchanged words. Geometry changes require a full upload.
; Guest bitmap writes have already completed: any transport inconsistency is
; a machine fault, never a primitive fallback that repeats drawing.
bb_refresh:
    d=0, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=17, idx=Load
    mem=Read
    d=Object, ldsym
    d=1, esp=Bus
    estk=Read
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=bb_refresh_display
    d=3, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_success
    d=0x410, r=Bus, rb=0, ldrb
    seq=Jump, brch=bb_refresh_geometry
bb_refresh_display:
    d=0x510, r=Bus, rb=0, ldrb
bb_refresh_geometry:
    read=Vr, vr=3
    d=Object, ldvr, vr=4
    d=17, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=18, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    d=15, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=8, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=2, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=0, s=Branch, brch=12, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=3, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=0, s=Branch, brch=16, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=10, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=0, s=Branch, brch=32, alu=Add, rb=7, ldrb
    ra=7, io=Read
    ra=4, s=Bus, d=Device, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_full
    ra=0, s=Branch, brch=20, alu=Add, rb=7, ldrb
    ra=7, d=2, io=Write
    d=5, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=7, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=Add, ldq
    d=Q, r=Bus, rb=3, ldrb
    ra=2, rb=4, alu=MultiplyUnsigned
    alu=ProductLow, rb=9, ldrb
    ra=9, s=Branch, brch=27, alu=Rotate, rb=9, ldrb
    ra=9, s=Bus, d=0x07ffffff, alu=And, rb=9, ldrb
    ra=3, rb=4, alu=MultiplyUnsigned
    alu=ProductLow, rb=11, ldrb
    ra=11, s=Branch, brch=31, alu=Add, rb=11, ldrb
    ra=11, s=Branch, brch=27, alu=Rotate, rb=11, ldrb
    ra=11, s=Bus, d=0x07ffffff, alu=And, rb=11, ldrb
    d=6, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=8, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=12, ldrb
    ra=8, rb=12, alu=Add, ldrb
    read=Vr, vr=4
    d=Object, page=Fetch
bb_refresh_row:
    ra=8, rb=10, alu=MultiplyUnsigned
    alu=ProductLow, rb=2, ldrb
    ra=2, rb=9, alu=Add, ldq
    ra=0, s=Branch, brch=24, alu=Add, rb=7, ldrb
    ra=7, d=Q, io=Write
    r=Q, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    ra=9, rb=13, ldrb
    ra=0, s=Branch, brch=28, alu=Add, rb=7, ldrb
bb_refresh_word:
    mem=Read
    ra=7, d=Object, io=Write
    ra=13, s=Branch, brch=1, alu=Add, rb=13, ldrb
    ra=13, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_refresh_next_row
    idx=Increment, seq=Jump, brch=bb_refresh_word
bb_refresh_next_row:
    ra=8, s=Branch, brch=1, alu=Add, rb=8, ldrb
    ra=8, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_refresh_row
    ra=0, s=Branch, brch=20, alu=Add, rb=7, ldrb
    ra=7, d=1, io=Write, seq=Jump, brch=bb_success
bb_refresh_full:
    ra=3, rb=10, alu=MultiplyUnsigned
    alu=ProductLow, rb=11, ldrb
    d=bb_success, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_upload

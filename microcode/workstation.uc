; Generic peripheral exerciser. Every pixel and input read comes from LOGIK.
; Mouse movement moves the uploaded cursor. Key/button presses invert the
; stripe pattern. No Smalltalk image or host drawing algorithm is involved.
.entry start
start:
    d=0x40c, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x418, r=Bus, rb=0, ldrb
    d=16, ra=0, io=Write
    d=0x41c, r=Bus, rb=0, ldrb
    d=16, ra=0, io=Write
    d=0x420, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x424, r=Bus, rb=0, ldrb
    d=0, ra=0, io=Write
    d=16, r=Bus, rb=1, ldrb
    d=0x42c, r=Bus, rb=0, ldrb
cursor_words:
    d=0x80000000, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=cursor_words
    d=0x424, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x518, r=Bus, rb=0, ldrb
    d=320, ra=0, io=Write
    d=0x51c, r=Bus, rb=0, ldrb
    d=240, ra=0, io=Write
    d=0x520, r=Bus, rb=0, ldrb
    d=10, ra=0, io=Write
    d=0xaaaa5555, r=Bus, rb=6, ldrb
frame:
    d=0x524, r=Bus, rb=0, ldrb
    d=0, ra=0, io=Write
    d=2400, r=Bus, rb=1, ldrb
    d=0x52c, r=Bus, rb=0, ldrb
pixels:
    d=Register, ra=6, ldsym
    d=Symbol, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=pixels
    d=0x524, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
poll:
    d=0x310, r=Bus, rb=0, ldrb
    ra=0, io=Read
    d=Device, r=Bus, s=Bus, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=poll
    d=0x314, r=Bus, rb=0, ldrb
    ra=0, io=Read
    d=Device, r=Bus, rb=2, ldrb
    d=0x324, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x104, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    alu=Sub, ra=2, s=Bus, d=2, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=poll
    alu=Not, ra=6, rb=6, ldrb
    seq=Jump, brch=frame

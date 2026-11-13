; The display device. Publishes a 640x480 monochrome frame, then patches it.
;
; Word n of the frame (20 words per row, 9,600 in all) holds 9600 - n, so
; each 32-pixel column shows a binary count in its right half: fine vertical
; stripes at the right edge of the column, coarser to their left. The patch
; then blackens the top 16 rows.
;
; The frame replaces the debug panels once it is published. Before that:
;   0  0x44495350 ("DISP") when the program starts
.equ DEBUG = 0xf00
.equ DISPLAY = 0x510
.equ WORDS = 9600
.entry start
start:
    d=DEBUG, r=Bus, rb=0, ldrb
    d=0x44495350, ra=0, io=Write

    d=DISPLAY+0x08, r=Bus, rb=0, ldrb
    d=640, ra=0, io=Write
    d=DISPLAY+0x0c, r=Bus, rb=0, ldrb
    d=480, ra=0, io=Write
    d=DISPLAY+0x10, r=Bus, rb=0, ldrb
    d=20, ra=0, io=Write
    d=DISPLAY+0x14, r=Bus, rb=0, ldrb
    d=0, ra=0, io=Write

    d=DISPLAY+0x1c, r=Bus, rb=0, ldrb
    d=WORDS, r=Bus, rb=1, ldrb
fill:
    d=Register, ra=1, ldsym
    d=Symbol, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=fill
    d=DISPLAY+0x14, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write

    d=2, ra=0, io=Write
    d=DISPLAY+0x1c, r=Bus, rb=0, ldrb
    d=320, r=Bus, rb=1, ldrb
band:
    d=0xffffffff, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=band
    d=DISPLAY+0x14, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
forever:
    seq=Jump, brch=forever

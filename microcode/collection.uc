; Seven allocations in two 256-word semispaces force five collections.
; Each iteration retains the previous result/selection until the next allocation.
.equ CLASS = 0xa000000064
.entry start
start:
    d=7, r=Bus, rb=8, ldrb
loop:
    d=CLASS, page=Allocate, size=120, scan=0
    alu=Sub, ra=8, rb=8, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=loop
    halt

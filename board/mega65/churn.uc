; Collection under pager pressure. COUNT throwaway allocations overflow the
; pager table several times; nothing references them, so the collector drops
; them and none reaches the SD card. One object stays referenced from ESTKR
; and must survive every collection.
;
; Debug display:
;   0  0x444f4e45 ("DONE") when the allocations finish
;   1  field 2 of the surviving object               expect 0x000004d2
; The disk processor's row 6 (requests served) should stay 0.
.equ CLASS = 0xa000000064
.equ DEBUG = 0xf00
.equ COUNT = 6 * PAGER_ENTRIES
.entry start
start:
    d=COUNT, r=Bus, ldrb
    d=CLASS, page=Allocate, size=2, scan=1
    d=Object, estk=Bus
    d=2, idx=Load
    d=0xc2000004d2, mem=Write
loop:
    d=CLASS, page=Allocate, size=2, scan=1
    alu=Sub, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=loop
    d=Estk, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+4, r=Bus, rb=1, ldrb
    d=Symbol, ra=1, io=Write
    d=DEBUG, r=Bus, rb=1, ldrb
    d=0x444f4e45, ra=1, io=Write
forever:
    seq=Jump, brch=forever

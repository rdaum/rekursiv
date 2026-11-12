; Paging through the SD card. 1300 allocations overflow the 1,024-entry
; pager, so OBJEKT saves new victims to the disk processor's store. Fetching
; early objects by reference then refills them from the card.
;
; Object n (the nth allocation, identity n, opaque) holds 1301 - n in field 2.
; Debug display:
;   0  0x50414745 ("PAGE") once the allocations finish
;   1  field 2 of object 1, refilled from the card     expect 0x514 (1300)
;   2  field 2 of object 200, refilled                 expect 0x44d (1101)
;   3  field 2 of object 1300, still resident          expect 0x001
;   4  0x444f4e45 ("DONE")
;   7  a running counter
.equ CLASS = 0xa000000064
.equ DEBUG = 0xf00
.equ REF = 0x8000000000
.entry start
start:
    d=1300, r=Bus, rb=1, ldrb
allocate:
    d=CLASS, page=Allocate, size=2, scan=0
    d=2, idx=Load
    d=Register, ra=1, mem=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=allocate
    d=DEBUG, r=Bus, rb=0, ldrb
    d=0x50414745, ra=0, io=Write

    d=REF+1, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+4, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    d=REF+200, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+8, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    d=REF+1300, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+12, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    d=DEBUG+16, r=Bus, rb=0, ldrb
    d=0x444f4e45, ra=0, io=Write

    d=DEBUG+28, r=Bus, rb=0, ldrb
    d=0, r=Bus, rb=1, ldrb
forever:
    ra=1, rb=1, alu=Add, s=Branch, brch=1, ldrb
    d=Register, ra=1, ldsym
    d=Symbol, ra=0, io=Write
    seq=Jump, brch=forever

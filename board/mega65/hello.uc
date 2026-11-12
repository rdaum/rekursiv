; First MEGA65 bring-up program. It needs no backing store and no collector:
; the pager has enough slots that its allocations never collide.
;
; Results go to the debug display, eight words at 0xf00 that the board shows
; on screen in hexadecimal:
;   0  signature 0x524b5631 ("RKV1")
;   1  field 1 of a new object (11, through the cached representation)
;   2  field 2 (22, from object memory)
;   3  field 3 (33, from object memory)
;   4  free body words after 600 more allocations
;   5  field 2 of the first object, fetched again by reference (22)
;   7  a counter that keeps changing while the machine runs
.equ CLASS = 0xa000000064
.equ DEBUG = 0xf00
.entry start
start:
    d=DEBUG, r=Bus, rb=0, ldrb
    d=0x524b5631, ra=0, io=Write

    ; One object with three opaque fields. Keep its reference in ESTKR.
    d=CLASS, page=Allocate, size=3, scan=0
    d=Object, estk=Bus
    d=1, idx=Load
    d=11, mem=Write
    d=2, idx=Load
    d=22, mem=Write
    d=3, idx=Load
    d=33, mem=Write

    d=1, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+4, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+8, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write
    d=3, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+12, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    ; Six hundred more objects, then report the remaining body space.
    d=600, r=Bus, rb=1, ldrb
allocate:
    d=CLASS, page=Allocate, size=2, scan=0
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=allocate
    read=FreeWords
    d=Object, ldsym
    d=DEBUG+16, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    ; The first object is still resident; select it again by reference.
    d=Estk, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    d=DEBUG+20, r=Bus, rb=0, ldrb
    d=Symbol, ra=0, io=Write

    d=DEBUG+28, r=Bus, rb=0, ldrb
    d=0, r=Bus, rb=1, ldrb
forever:
    ra=1, rb=1, alu=Add, s=Branch, brch=1, ldrb
    d=Register, ra=1, ldsym
    d=Symbol, ra=0, io=Write
    seq=Jump, brch=forever

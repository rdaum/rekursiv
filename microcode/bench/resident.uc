; Bootstrap is outside measurement. Each iteration fetches, reads, increments,
; and writes a resident field through the ordinary OBJEKT command interface.
.entry start
start:
    d=0xa000000064, page=Allocate, size=2, scan=0
    idx=Two
    d=23, mem=Write
loop:
    d=0x8000000001, page=Fetch
    idx=Two
    mem=Read
    d=Object, r=Bus, rb=0, ldrb
    ra=0, rb=1, alu=Add, s=Branch, brch=1, ldrb
    d=Register, ra=1, mem=Write
    seq=Jump, brch=loop

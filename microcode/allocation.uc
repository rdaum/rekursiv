; Keep the first object across pager collisions, then fetch its second field.
.equ CLASS = 0xa000000064
.entry start
start:
    d=20, r=Bus, ldrb
    d=CLASS, page=Allocate, size=2, scan=1
    d=Object, estk=Bus
    d=2, idx=Load
    d=0xc2000004d2, mem=Write         ; compact signed integer 1234
loop:
    d=CLASS, page=Allocate, size=2, scan=1
    alu=Sub, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=loop
    d=Estk, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, estk=Bus
    halt

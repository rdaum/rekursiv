; Prepared-address stream with independent NUMERIK work during each RAM read.
; R0 = sum, R1 = remaining words, R2 = byte offset, R3 = processed words.
; PREPARE forwards the new index; PREPARED consumes the previous address.
; LAUNCH permits local work to retire before the reply. d=Object joins it.
.entry start
start: d=0xa000000064, page=Allocate, size=4, scan=0
idx=One
d=11, mem=Write, idx=Increment
d=22, mem=Write, idx=Increment
d=33, mem=Write, idx=Increment
d=44, mem=Write
idx=One, prepare
d=0, r=Bus, rb=0, ldrb
d=4, r=Bus, rb=1, ldrb
d=0, r=Bus, rb=2, ldrb
d=0, r=Bus, rb=3, ldrb
loop: mem=Read, prepared, prepare, idx=Increment, launch
ra=1, rb=1, alu=Sub, s=Branch, brch=1, cin=One, ldrb, flags
ra=2, rb=2, alu=Add, s=Branch, brch=4, ldrb
ra=3, rb=3, alu=Add, s=Branch, brch=1, ldrb
d=Object, ra=0, rb=0, alu=Add, s=Bus, ldrb, seq=ConditionalJump, cc=!Zero, brch=loop
halt

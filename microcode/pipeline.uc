; Prepared-address stream with independent NUMERIK work during each RAM read.
; R0 = sum, R1 = remaining words, R2 = byte offset, R3 = processed words.
; PREPARE forwards the new index; PREPARED consumes the previous address.
; LAUNCH permits local work to retire before the reply. d=Object joins it.
;
; Run: cargo run --locked -p rekursiv-sim -- --microcode microcode/pipeline.uc
; Allocates a four-word opaque object and writes raw 11,22,33,44 (no descriptor).
; On halt: R0=110, R1=0, R2=16, R3=4. R2 demonstrates independent arithmetic;
; it is a nominal four-byte offset, not an address into the 40-bit object RAM.
; Only one read is outstanding. Every iteration joins it before looping.
;
; Writes use old IDX even when idx=Increment appears on the same line. The
; first prepare primes component 1. Each loop reads the previous prepared
; address and prepares the incremented index. The last prepare therefore
; records an out-of-bounds component 5, but no access consumes it: deferred
; address validation does not fault merely because that unused address exists.
; The final add/branch tests the countdown flags: intervening offset/count
; updates and the sum do not have 'flags', so they preserve that condition.
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

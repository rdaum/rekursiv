; Allocation-pressure demonstration of the machine RAM collector.
;
; Run: cargo run --locked -p rekursiv-sim -- --microcode microcode/collection.uc
; The standard simulator installs ram-collector.uc and supplies 512 RAM words,
; split into two 256-word semispaces. Seven 120-word opaque allocations force
; five collections: two objects fit initially, then each further allocation
; needs collection. A larger memory configuration can change this count.
;
; The previous allocation remains rooted by selection/Object while the next
; request runs. Older results have no remaining roots and can be discarded.
; The collector preserves the previous object, leaving 136 words for retry.
; scan=0 means body words contain no reference edges; class edges still count.
; CLASS is arbitrary metadata; no guest-language object layout is involved.
;
; R8 is the countdown and must survive every collector entry. On halt R8=0;
; with a fresh allocator, next identity is 8. There is no explicit gc=Collect:
; failed Allocate requests enter recovery and retry through LOGIK hardware.
.equ CLASS = 0xa000000064
.entry start
start:
    d=7, r=Bus, rb=8, ldrb
loop:
    d=CLASS, page=Allocate, size=120, scan=0
    alu=Sub, ra=8, rb=8, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=loop
    halt

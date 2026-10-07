; Allocation, dirty eviction, and automatic refill demonstration.
;
; Run: cargo run --locked -p rekursiv-sim -- --microcode microcode/allocation.uc
; Requires the loader's standard RAM collector and backing-store service.
; With the simulator's 16 pager slots, 21 allocations force slot collisions.
; CLASS is an arbitrary class reference used as metadata, not a fetched object.
;
; R0 counts twenty throwaway allocations. ESTKR keeps the first object's full
; reference live while its pager entry is evicted; a root guarantees liveness,
; not residency. Its second component holds compact signed integer 1234.
; Fetch resolves the same identity after eviction and refills the saved body.
; On halt: R0=0, ESTKR=0xc2000004d2. The simulator's allocation example also
; checks that writeback occurred. Selection, IDX, Object and flags are scratch.
; These are generic objects: there is no Smalltalk descriptor in component 1.
.equ CLASS = 0xa000000064
.entry start
start:
    ; Omitted ra/rb select R0. Save the allocated reference before any command
    ; can replace Object, the response latch from the previous command.
    d=20, r=Bus, ldrb
    d=CLASS, page=Allocate, size=2, scan=1
    d=Object, estk=Bus
    d=2, idx=Load
    d=0xc2000004d2, mem=Write         ; compact signed integer 1234
loop:
    ; Sub computes R0 + ~1 + carry; cin=One makes this R0-1.
    ; Test the saved flags in the next instruction, after they have changed.
    d=CLASS, page=Allocate, size=2, scan=1
    alu=Sub, s=Branch, brch=1, cin=One, flags, ldrb
    seq=ConditionalJump, cc=!Zero, brch=loop
    ; ESTKR still holds the original identity even if its body has moved.
    d=Estk, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, estk=Bus
    halt

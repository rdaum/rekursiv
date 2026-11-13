; RAM collector. ROOT_COUNT comes from the hardware configuration.
; R0 = root index; R1 = body offset; R2 = size; R3 = minor; R4 = slot flags.
; LOGIK saves the interrupted arithmetic and sequencer state before entry.
;
; Loader entry, not a standalone mutator program: .collector tells LOGIK where
; to enter after Allocate/Fetch exhaustion or an explicit gc=Collect request.
; crates/rekursiv-asm/src/collector.rs relocates this source and injects bounds.
; ROOT_COUNT covers the frozen machine root interface (VRs, selection, code
; literals, live ESTK slots, explicit roots, etc.), not just 32 explicit roots.
; See ../docs/recovery.md for the exact root index layout and saved state.
;
; A collection is minor when enough words remain free. Objects allocated or
; refilled since the last collection are young; the rest are old. BeginMinor
; queues the persistence dependencies and every old object that has had a
; reference stored into it since; Mark then treats old objects as live without
; tracing them, and Commit drops the unmarked young objects without moving
; anything. A minor collection reclaims pager slots but no words. BeginMinor
; returns zero when too few words remain or the collection is an explicit
; gc=Collect, and a full collection follows.
;
; Algorithm: Begin queues the persistence dependencies -> mark the roots ->
; take each queued object once and mark what it names -> copy the marked
; objects in slot order -> commit -> return. The first mark of an object queues
; it, so each live object is traced once and cycles end. Copying in slot order
; keeps the new layout independent of the order of tracing. The book's
; collector instead repeats whole-table passes to a fixed point; this one
; departs from it so that tracing cost follows the live data.
;
; This collects resident RAM only: Mark never pages in an absent object and
; no backing record is deleted. Unreachable NEW objects and clean RAM copies
; may disappear; unsaved persistent changes and their descendants must survive.
;
; R3 is nonzero during a minor collection.
; gc=Next takes one queued slot and selects it without changing the mutator
; selection. gc=NextMarked selects the next marked slot in ascending order,
; starting from slot zero after Begin. Both return the slot's flags, or zero
; when none remains: 0 valid, 1 marked, 2 scanned, 3 retention required,
; 4 NEW. gc=Info selects 0 reference, 1 class, 2 body size. gc=Mark returns 1
; for a new mark.
; ReadBody/WriteBody take zero-based offsets, unlike one-based mutator IDX.
; WriteBody uses the maintenance read latch for data; D is still the offset.
;
; No published mapping changes before Commit. A copy/space/device failure
; leaves the original semispace authoritative and stops recovery. Successful
; Return restores the mutator and retries its failed command once (or advances
; beyond explicit Collect). Live data plus the pending request must fit in a
; semispace; collection cannot make oversized live heaps fit. No flush fallback
; is implemented. R0..R4 and collector flags are scratch, restored on return.
.collector collector
collector:
    gc=BeginMinor
    d=Object, r=Bus, rb=3, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=begun
    ; Begin clears per-collection marks and plans, chooses the inactive half,
    ; and marks and queues every object that persistence requires in RAM.
    gc=Begin
begun:
    d=0, r=Bus, rb=0, ldrb
roots:
    d=Register, ra=0, gc=Root
    d=Object, gc=Mark
    alu=Add, ra=0, rb=0, s=Branch, brch=1, ldrb
    alu=Sub, ra=0, s=Bus, d=ROOT_COUNT, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=roots

; Trace: every queued object contributes its class, and its body if scanned.
trace:
    gc=Next
    d=Object, r=Bus, rb=4, ldrb
    alu=Sub, ra=4, s=Bus, d=0, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=traced
    d=1, gc=Info                     ; class edge, including opaque objects
    d=Object, gc=Mark
    alu=And, ra=4, rb=4, s=Bus, d=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=trace
    d=2, gc=Info                     ; scanned body size
    d=Object, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=1, ldrb
trace_body:
    alu=Sub, ra=1, rb=2, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=trace
    d=Register, ra=1, gc=ReadBody
    d=Object, gc=Mark
    alu=Add, ra=1, rb=1, s=Branch, brch=1, ldrb
    seq=Jump, cc=Zero, brch=trace_body

; A minor collection copies nothing.
traced:
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=finish

; Plan and copy retained bodies into the inactive half, in slot order. COMMIT
; publishes every mapping together; until then the original heap is
; authoritative.
copy:
    gc=NextMarked
    d=Object, r=Bus, rb=4, ldrb
    alu=Sub, ra=4, s=Bus, d=0, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=finish
    gc=Stage
    d=2, gc=Info
    d=Object, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=1, ldrb
copy_body:
    ; Ascending matched read/write offsets let hardware track copy completion.
    alu=Sub, ra=1, rb=2, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=copy
    d=Register, ra=1, gc=ReadBody
    d=Register, ra=1, gc=WriteBody
    alu=Add, ra=1, rb=1, s=Branch, brch=1, ldrb
    seq=Jump, cc=Zero, brch=copy_body
finish:
    ; Commit atomically flips bases and semispace; Return resumes saved control.
    gc=Commit
    gc=Return

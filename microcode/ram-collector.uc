; Book-derived RAM collector. ROOT_COUNT and PAGER_ENTRIES come from the
; hardware configuration. This is a project format, not a historical binary.
; R0 = root/slot; R1 = body offset; R2 = size; R3 = changed; R4 = slot flags.
; LOGIK saves the interrupted arithmetic and sequencer state before entry.
;
; Loader entry, not a standalone mutator program: .collector tells LOGIK where
; to enter after Allocate/Fetch exhaustion or an explicit gc=Collect request.
; crates/rekursiv-asm/src/collector.rs relocates this source and injects bounds.
; ROOT_COUNT covers the frozen machine root interface (VRs, selection, code
; literals, live ESTK slots, explicit roots, etc.), not just 32 explicit roots.
; See ../docs/recovery.md for the exact root index layout and saved state.
;
; Algorithm: seed roots -> retain persistence dependencies -> repeat resident
; edge tracing until unchanged -> stage/copy marked slots -> commit -> return.
; This collects resident RAM only: Mark never pages in an absent object and
; no backing record is deleted. Unreachable NEW objects and clean RAM copies
; may disappear; unsaved persistent changes and their descendants must survive.
;
; gc=Slot selects the maintenance slot without changing the mutator selection.
; Returned bits: 0 valid, 1 marked, 2 scanned, 3 retention required, 4 NEW. gc=Info
; selects 0 reference, 1 class, 2 body size. gc=Mark returns 1 for a new mark,
; 0 otherwise; OR these replies into R3 to detect the tracing fixed point.
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
    ; Begin clears per-collection marks/plans and chooses the inactive half.
    gc=Begin
    d=0, r=Bus, rb=0, ldrb
roots:
    d=Register, ra=0, gc=Root
    d=Object, gc=Mark
    alu=Add, ra=0, rb=0, s=Branch, brch=1, ldrb
    alu=Sub, ra=0, s=Bus, d=ROOT_COUNT, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=roots

; Retain unsaved persistent state, then trace its descendants in the same
; fixed point as ordinary roots. SLOT bit 3 includes a partially saved NEW.
    d=0, r=Bus, rb=0, ldrb
dirty:
    ; Mask 9 requires both valid and retention-required. Bit 3 also covers
    ; protected persistence dependencies and a valid prepared-address root.
    d=Register, ra=0, gc=Slot
    d=Object, r=Bus, rb=4, ldrb
    alu=And, ra=4, rb=4, s=Bus, d=9, ldrb, flags
    alu=Sub, ra=4, s=Bus, d=9, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=dirty_next
    d=0, gc=Info
    d=Object, gc=Mark
dirty_next:
    alu=Add, ra=0, rb=0, s=Branch, brch=1, ldrb
    alu=Sub, ra=0, s=Bus, d=PAGER_ENTRIES, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=dirty

; Repeated passes avoid a recursive work stack. Mark before following edges;
; only a matching resident reference contributes a mark. No disk fetches.
pass:
    d=0, r=Bus, rb=3, ldrb
    d=0, r=Bus, rb=0, ldrb
trace_slot:
    ; Mask 3 requires valid+marked; unmarked slots cannot contribute edges.
    d=Register, ra=0, gc=Slot
    d=Object, r=Bus, rb=4, ldrb
    alu=And, ra=4, rb=4, s=Bus, d=3, ldrb, flags
    alu=Sub, ra=4, s=Bus, d=3, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=trace_next
    d=1, gc=Info                     ; class edge, including opaque objects
    d=Object, gc=Mark
    alu=Or, ra=3, rb=3, d=Object, s=Bus, ldrb
    d=Register, ra=0, gc=Slot
    ; Read the owner's flags again: R4 previously held only valid/marked bits.
    d=Object, r=Bus, rb=4, ldrb
    alu=And, ra=4, rb=4, s=Bus, d=4, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=trace_next
    d=2, gc=Info                     ; scanned body size
    d=Object, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=1, ldrb
trace_body:
    alu=Sub, ra=1, rb=2, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=trace_next
    d=Register, ra=1, gc=ReadBody
    d=Object, gc=Mark
    alu=Or, ra=3, rb=3, d=Object, s=Bus, ldrb
    alu=Add, ra=1, rb=1, s=Branch, brch=1, ldrb
    seq=Jump, cc=Zero, brch=trace_body
trace_next:
    alu=Add, ra=0, rb=0, s=Branch, brch=1, ldrb
    alu=Sub, ra=0, s=Bus, d=PAGER_ENTRIES, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=trace_slot
    alu=Sub, ra=3, s=Bus, d=0, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=pass

; Plan and copy retained bodies into the inactive half. COMMIT publishes
; every mapping together; until then the original heap remains authoritative.
    d=0, r=Bus, rb=0, ldrb
copy_slot:
    d=Register, ra=0, gc=Slot
    d=Object, r=Bus, rb=4, ldrb
    alu=And, ra=4, rb=4, s=Bus, d=3, ldrb, flags
    alu=Sub, ra=4, s=Bus, d=3, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=copy_next
    gc=Stage
    d=2, gc=Info
    d=Object, r=Bus, rb=2, ldrb
    d=0, r=Bus, rb=1, ldrb
copy_body:
    ; Ascending matched read/write offsets let hardware track copy completion.
    alu=Sub, ra=1, rb=2, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=copy_next
    d=Register, ra=1, gc=ReadBody
    d=Register, ra=1, gc=WriteBody
    alu=Add, ra=1, rb=1, s=Branch, brch=1, ldrb
    seq=Jump, cc=Zero, brch=copy_body
copy_next:
    alu=Add, ra=0, rb=0, s=Branch, brch=1, ldrb
    alu=Sub, ra=0, s=Bus, d=PAGER_ENTRIES, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=copy_slot
    gc=Commit
    ; Commit atomically flips bases and semispace; Return resumes saved control.
    gc=Return

; Primitive 103: CharacterScanner's text-run loop. Field numbers follow the
; archived Squeak 1.1 CharacterScanner (BitBlt fields 3..17, lastIndex 18,
; xTable 19, stopConditions 20). Strings are imported byte objects.
;
; First walk the run without mutations, validating every metric up to the
; first stop. Only then execute it. No late primitive failure may redraw a
; prefix (notably for XOR). A failed BitBlt preflight restores width/sourceX.
; The fallback tests receiver.stopConditions but returns through the argument
; stops; accelerate only when they are identical. Preserve its width/sourceX
; and lastIndex results, including crossed margins and empty runs.
;
; Scratch roots survive BitBlt's ESTK frame and collector/refill activity:
; 8 pass (0 validation, 1 execution), 9 current destX;
; 15/16 caller IP/SP, 17 failure, 18 string, 19 stops, 20 xTable;
; 21 stopIndex, 22 rightX, 23 display flag, 24 lastIndex, 25 nextDestX.
; During BitBlt preflight only, 24/25 instead root old width/sourceX values.
; All scratch roots are cleared on success/failure. No scheduler entry occurs
; inside this primitive; guest arguments remain rooted in the caller context.
primitive_scanner:
    ra=1, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0, esp=Bus
    ra=8, ldq
    d=15, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    ra=9, ldq
    d=16, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    ra=15, ldq
    d=17, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=20, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=7, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=4, ldq
    d=9, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    d=19, idx=Load
    mem=Read
    d=20, r=Bus, rb=14, ldrb
    ra=14, d=Object, ldroot
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=TRUE, seq=ConditionalJump, cc=Symbol, brch=scanner_display
    d=FALSE, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    d=23, r=Bus, rb=14, ldrb
    ra=14, d=0, ldroot
    seq=Jump, brch=scanner_arguments
scanner_display:
    d=23, r=Bus, rb=14, ldrb
    ra=14, d=1, ldroot
scanner_arguments:
    idx=Decrement
    mem=Read
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Object, ldroot
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=2, ldq
    d=22, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    idx=Decrement
    mem=Read
    d=18, r=Bus, rb=14, ldrb
    ra=14, d=Object, ldroot
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=3, ldq
    d=21, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=2, ldq
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_failed
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    ra=3, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=18, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    read=Type
    d=SPECIAL_6, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=5, ldrb
    ra=5, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=5, ldrb
    ra=5, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=5, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    read=Type
    d=SPECIAL_7, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=260, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=20, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    read=Type
    d=SPECIAL_7, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    ; Arrays must not alias fields that the scanner itself updates.
    read=Vr, vr=6
    d=Object, ldsym
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Root, seq=ConditionalJump, cc=Symbol, brch=scanner_failed
    d=20, r=Bus, rb=14, ldrb
    ra=14, d=Root, seq=ConditionalJump, cc=Symbol, brch=scanner_failed
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Root, ldsym
    read=Vr, vr=6
    d=Object, page=Fetch
    d=20, idx=Load
    mem=Read
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    d=23, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=2, ldrb
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_validate_run
    ; Optional scanner failures retain the original BitBlt fallback behavior.
    read=Vr, vr=6
    d=Object, page=Fetch
    d=6, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=2, s=Branch, brch=15, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=scanner_bitmap_probe
    ra=2, s=Branch, brch=25, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_bitmap_probe
    ra=2, s=Branch, brch=26, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_failed
scanner_bitmap_probe:
    d=9, idx=Load
    mem=Read
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Object, ldroot
    d=0xc200000000, mem=Write
    d=11, idx=Load
    mem=Read
    d=25, r=Bus, rb=14, ldrb
    ra=14, d=Object, ldroot
    d=0xc200000000, mem=Write
    d=2, csp=Bus
    d=scanner_bitmap_valid, cstk=Bus
    d=0, csp=Bus
    d=scanner_bitmap_invalid, r=Bus, rb=15, ldrb
    seq=Jump, brch=bb_enter
scanner_bitmap_valid:
    d=scanner_validate_run, r=Bus, rb=12, ldrb
    seq=Jump, brch=scanner_restore_probe
scanner_bitmap_invalid:
    d=scanner_failed, r=Bus, rb=12, ldrb
scanner_restore_probe:
    read=Vr, vr=6
    d=Object, page=Fetch
    d=9, idx=Load
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Root, mem=Write
    d=11, idx=Load
    d=25, r=Bus, rb=14, ldrb
    ra=14, d=Root, mem=Write
    d=0, esp=Bus
    d=Register, ra=12, seq=Bus
scanner_validate_run:
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=0, ldroot
    seq=Jump, brch=scanner_restart
scanner_execute_run:
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=1, ldroot
scanner_restart:
    ; Reload the original start index and destX for either pass.
    d=16, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=9, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=3, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    ra=2, ldq
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    read=Vr, vr=6
    d=Object, page=Fetch
    d=7, idx=Load
    mem=Read
    d=Object, r=Bus, rb=4, ldrb
    ra=4, ldq
    d=9, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
scanner_character:
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=2, ldrb
    d=21, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=3, ldrb
    ra=3, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_end
    ; Each imported String byte occupies one OBJEKT component.
    d=18, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    ra=2, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=3, alu=Add, rb=5, ldrb
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    d=Register, ra=5, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scanner_stop
    d=20, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    ; Fonts need entries only for their supported character range.
    ra=5, s=Branch, brch=1, alu=Add, rb=11, ldrb
    read=Size
    d=Object, r=Bus, s=Register, rb=11, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=Register, ra=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=6, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scanner_failed
    ra=7, rb=6, alu=Sub, cin=One, ldq
    d=Q, r=Bus, rb=7, ldrb
    ; Width and accumulated position must remain Squeak SmallIntegers.
    ra=7, s=Bus, d=0x40000000, alu=Add, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    d=9, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=4, ldrb
    ra=4, rb=7, alu=Add, ldq
    d=Q, r=Bus, rb=10, ldrb
    ra=10, s=Bus, d=0x40000000, alu=Add, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_failed
    ra=10, ldq
    d=25, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_margin
    ; Match fallback state even when this glyph crosses the right margin.
    read=Vr, vr=6
    d=Object, page=Fetch
    d=11, idx=Load
    ra=6, estk=Compact, compact=2
    d=Estk, mem=Write
    d=9, idx=Load
    ra=7, estk=Compact, compact=2
    d=Estk, mem=Write
scanner_margin:
    d=22, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, rb=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=scanner_crossed
    ; Validation and nondisplaying scans never enter BitBlt.
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_advance
    d=23, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_advance
    d=2, csp=Bus
    d=scanner_advance, cstk=Bus
    d=0, csp=Bus
    ; All raster operands and run metrics were checked before committing.
    d=bad_state, r=Bus, rb=15, ldrb
    seq=Jump, brch=bb_enter
scanner_advance:
    d=25, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=4, ldrb
    ra=4, ldq
    d=9, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_next
    read=Vr, vr=6
    d=Object, page=Fetch
    d=7, idx=Load
    ra=4, estk=Compact, compact=2
    d=Estk, mem=Write
scanner_next:
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=Add, rb=2, ldrb
    ra=2, ldq
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    seq=Jump, brch=scanner_character
scanner_end:
    d=21, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=2, ldrb
    ra=2, ldq
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Q, ldroot
    d=259, r=Bus, rb=5, ldrb, seq=Jump, brch=scanner_stop
scanner_crossed:
    d=260, r=Bus, rb=5, ldrb
scanner_stop:
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=11, ldrb
    ra=11, flags
    seq=ConditionalJump, cc=Zero, brch=scanner_execute_run
    d=24, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=2, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    d=18, idx=Load
    ra=2, estk=Compact, compact=2
    d=Estk, mem=Write
    d=19, r=Bus, rb=14, ldrb
    ra=14, d=Root, page=Fetch
    d=Register, ra=5, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    d=send_result, r=Bus, rb=12, ldrb, seq=Jump, brch=scanner_release
scanner_failed:
    d=primitive_failed, r=Bus, rb=12, ldrb
scanner_release:
    d=15, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=8, ldrb
    d=16, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=9, ldrb
    d=17, r=Bus, rb=14, ldrb
    ra=14, d=Root, r=Bus, rb=15, ldrb
    d=8, r=Bus, rb=14, ldrb
    ra=14, d=0, ldroot
    d=9, r=Bus, rb=14, ldrb
    ra=14, d=0, ldroot
    d=15, r=Bus, rb=14, ldrb
scanner_clear_roots:
    ra=14, d=0, ldroot
    ra=14, s=Branch, brch=1, alu=Add, rb=14, ldrb
    ra=14, s=Branch, brch=26, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=scanner_clear_roots
    d=0, esp=Bus, sp=Bus, csp=Bus
    d=0, estk=Bus
    d=6, r=Bus, rb=1, ldrb
    d=Register, ra=12, seq=Bus

; R0 is a validated 0..255 primitive number. Preserve all registers and
; roots; only Q is scratch. Each profile supplies 256 single-word jumps.
; Keep the table contiguous: its control address is computed, not fetched
; through NAM (the default hardware has only 256 NAM words, used by bytecodes).
dispatch_primitive:
    ra=0, s=Branch, brch=primitive_table, alu=Add, ldq
    d=Q, seq=Bus

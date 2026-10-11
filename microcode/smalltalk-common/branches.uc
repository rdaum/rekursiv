; Short offsets are (opcode & 7)+1. Long unconditional offsets are
; ((opcode & 7)-4)*256+extension, allowing backward branches. Conditional long
; offsets are (opcode & 3)*256+extension; all are relative to advanced IP.
;
; Shared Xerox/Squeak execution; layout constants come from the profile.
short_jump:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=1, alu=Add, rb=1, ldrb
    seq=Jump, brch=apply_jump
short_conditional:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=1, alu=Add, rb=1, ldrb
    seq=Jump, brch=conditional
long_jump:
    d=long_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
long_decoded:
    ra=13, s=Bus, d=7, alu=And, rb=1, ldrb
    ra=1, s=Branch, brch=4, alu=Sub, cin=One, rb=1, ldrb
    seq=Jump, brch=scale_offset
long_conditional:
    d=long_cond_decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
long_cond_decoded:
    ra=13, s=Bus, d=3, alu=And, rb=1, ldrb
scale_offset:
    ; Rotate then discard the wrapped byte: exact 32-bit left shift by eight,
    ; including negative backward-branch offsets.
    ra=1, s=Branch, brch=8, alu=Rotate, rb=1, ldrb
    ra=1, s=Bus, d=0xffffff00, alu=And, rb=1, ldrb
    ra=0, rb=1, alu=Add, ldq, ldrb
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=apply_jump
; peek leaves selected caller/top slot intact. Only full TRUE/FALSE identities
; are accepted. A non-Boolean sends the profile's mustBeBoolean selector without popping.
conditional:
    d=condition_value, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
condition_value:
    d=TRUE, seq=ConditionalJump, cc=Symbol, brch=condition_true
    d=FALSE, seq=ConditionalJump, cc=!Symbol, brch=boolean_failure
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=take_conditional
    ra=13, s=Branch, brch=172, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=take_conditional
    seq=Jump, brch=skip_conditional
condition_true:
    ra=13, s=Branch, brch=168, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=skip_conditional
    ra=13, s=Branch, brch=172, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=take_conditional
skip_conditional:
    d=0, r=Bus, rb=1, ldrb
take_conditional:
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
apply_jump:
    ra=1, rb=8, alu=Add, ldq, ldrb
    seq=Jump, brch=boundary

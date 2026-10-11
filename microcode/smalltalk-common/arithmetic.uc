; Arithmetic fast paths never discard arguments until type/range checks pass.
; Reconstructing and comparing the complete compact word checks the tag, not
; just its payload. A reference with a small identity cannot pass this check.
;
; Shared Xerox/Squeak execution; layout constants come from the profile.
arithmetic:
    ra=9, rb=12, alu=Sub, cin=One, ldq
    d=Q, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=arithmetic_argument, r=Bus, rb=6, ldrb, seq=Jump, brch=peek
arithmetic_argument:
    d=Symbol, r=Bus, rb=5, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failure
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failure
    ra=13, flags
    seq=ConditionalJump, cc=Zero, brch=integer_extended
    ra=13, s=Branch, brch=176, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=add
    ra=13, s=Branch, brch=177, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=subtract
    ra=13, s=Branch, brch=184, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=compare
    seq=ConditionalJump, cc=Zero, brch=multiply
    ra=13, s=Branch, brch=190, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bit_and
    seq=Jump, brch=bit_or
add:
    ra=4, rb=5, alu=Add, ldq
    seq=Jump, brch=integer_result
subtract:
    ra=4, rb=5, alu=Sub, cin=One, ldq
    seq=Jump, brch=integer_result

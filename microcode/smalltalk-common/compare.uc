; Shared Xerox/Squeak execution; layout constants come from the profile.
compare:
    ra=13, s=Branch, brch=178, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=less
    ra=13, s=Branch, brch=179, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=greater
    ra=13, s=Branch, brch=180, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=less_equal
    ra=13, s=Branch, brch=181, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=greater_equal
    ra=13, s=Branch, brch=182, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=equal
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=true_result
    seq=Jump, brch=false_result
less:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
greater:
    ra=4, rb=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
less_equal:
    ra=4, rb=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
greater_equal:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!CorrectedSign, brch=true_result
    seq=Jump, brch=false_result
equal:
    ra=4, rb=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=true_result
false_result:
    d=FALSE, ldsym, seq=Jump, brch=arithmetic_result
true_result:
    d=TRUE, ldsym
; With R15=0 this is a bytecode fast path: arithmetic left IDX on receiver,
; so replace it and clear the argument. Otherwise use the general primitive
; result path to refetch caller state and consume exactly R1 arguments.
arithmetic_result:
    ra=15, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_arithmetic_result
    d=Symbol, mem=Write
    idx=Increment
    d=NIL, mem=Write
    ra=9, s=Branch, brch=1, alu=Sub, cin=One, rb=9, ldrb
    seq=Jump, brch=boundary

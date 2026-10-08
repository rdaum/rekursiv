; Local numeric work with a control-flow boundary every four instructions.
.entry loop
loop:
    ra=0, rb=0, alu=Add, s=Branch, brch=1, ldrb, flags
    ra=0, rb=1, alu=Xor, s=Branch, brch=17, ldrb
    ra=1, rb=2, alu=Add, s=Branch, brch=3, ldrb
    seq=Jump, brch=loop

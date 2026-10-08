; Squeak Forms: bits, width, height, depth, offset at components 3..7.
; Device scanout is packed MSB-first, exactly like the guest's 32-bit Bitmap.
; Raster operations remain microcode; this transport only publishes raw words.
; R0 primitive/bank, R2 width, R3 height, R4 depth, R10 stride, R11 word count,
; VR4 bits, VR6 receiver. R1, R8/R9 survive; method caches are scratch.
primitive_bitmap:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldvr, vr=3
    d=bitmap_validated, r=Bus, rb=6, ldrb, seq=Jump, brch=bitmap_form
bitmap_validated:
    ra=0, s=Branch, brch=101, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_display
    ra=2, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=3, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=7, idx=Load
    mem=Read
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=12, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=13, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=12, s=Branch, brch=16, alu=Add, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=12, s=Branch, brch=0, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=13, s=Branch, brch=16, alu=Add, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    ra=13, s=Branch, brch=0, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=0x43c, r=Bus, rb=7, ldrb
    ra=12, ldq
    ra=7, d=Q, io=Write
    d=0x440, r=Bus, rb=7, ldrb
    ra=13, ldq
    ra=7, d=Q, io=Write
    ; Squeak's archived ioSetCursor uses identical data and mask bits:
    ; set pixels are opaque black, clear pixels transparent. RGB inversion
    ; would disappear against the desktop's middle grey.
    d=0x434, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    d=0x438, r=Bus, rb=7, ldrb
    ra=7, d=0, io=Write
    d=0x444, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    d=0x410, r=Bus, rb=0, ldrb
    d=bitmap_cursor_registered, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_upload
bitmap_cursor_registered:
    d=3, r=Bus, rb=7, ldrb
    read=Vr, vr=6
    ra=7, d=Object, ldroot, seq=Jump, brch=device_receiver_result
bitmap_display:
    d=0x510, r=Bus, rb=7, ldrb
    ra=7, io=Read
    ra=2, s=Bus, d=Device, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x514, r=Bus, rb=7, ldrb
    ra=7, io=Read
    ra=3, s=Bus, d=Device, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x534, r=Bus, rb=7, ldrb
    ra=7, d=0, io=Write
    d=0x538, r=Bus, rb=7, ldrb
    seq=Jump, brch=bitmap_palette
bitmap_palette_ready:
    d=0x510, r=Bus, rb=0, ldrb
    d=bitmap_display_registered, r=Bus, rb=15, ldrb, seq=Jump, brch=bitmap_upload
bitmap_display_registered:
    d=0, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=17, idx=Load
    read=Vr, vr=6
    d=Object, mem=Write, seq=Jump, brch=device_receiver_result

; Decode a pointer Form in VR3. Every check precedes output. Return R6, with
; bits rooted in VR4 and geometry ready for transport. Raw words need no tag
; decoding; the word-format descriptor bounds the used buffer.
bitmap_form:
    ; Compact checks use scratch slot zero, never a caller's rooted Form slot.
    d=0, esp=Bus
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=2, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=3, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=3, s=Branch, brch=16383, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, ldsym, r=Bus, rb=4, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=4, s=Branch, brch=32, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    ra=4, s=Branch, brch=1, alu=Sub, cin=One, rb=7, ldrb
    ra=4, rb=7, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, rb=4, alu=MultiplyUnsigned
    alu=ProductLow, rb=10, ldrb
    ra=10, s=Branch, brch=31, alu=Add, rb=10, ldrb
    ra=10, s=Branch, brch=27, alu=Rotate, rb=10, ldrb
    ra=10, s=Bus, d=0x07ffffff, alu=And, rb=10, ldrb
    ra=10, rb=3, alu=MultiplyUnsigned
    alu=ProductLow, rb=11, ldrb
    ra=11, s=Branch, brch=2, alu=Add, rb=7, ldrb
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Register, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Register, ra=6, seq=Bus

; Full replacement upload, return R15. Input geometry and VR4 from bitmap_form.
; Does not interpret palette indices or perform a guest drawing operation.
bitmap_upload:
    ra=0, s=Branch, brch=8, alu=Add, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=3, ldq
    ra=7, d=Q, io=Write
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=10, ldq
    ra=7, d=Q, io=Write
    ra=0, s=Branch, brch=32, alu=Add, rb=7, ldrb
    ra=4, ldq
    ra=7, d=Q, io=Write
    ra=0, s=Branch, brch=20, alu=Add, rb=7, ldrb
    ra=7, d=0, io=Write
    ra=0, s=Branch, brch=28, alu=Add, rb=7, ldrb
    read=Vr, vr=4
    d=Object, page=Fetch
    d=3, idx=Load
bitmap_upload_word:
    mem=Read
    ra=7, d=Object, io=Write
    ra=11, s=Branch, brch=1, alu=Sub, cin=One, rb=11, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=bitmap_upload_next
    ra=0, s=Branch, brch=20, alu=Add, rb=7, ldrb
    ra=7, d=1, io=Write
    d=Register, ra=15, seq=Bus
bitmap_upload_next:
    idx=Increment, seq=Jump, brch=bitmap_upload_word

; Palette specified by Squeak 1.1's platform SetUpPixmap. The external device
; is programmable and has no knowledge of Squeak's colour cube or grey ramp.
bitmap_palette:
    ra=7, d=0xffffff, io=Write
    ra=7, d=0x000000, io=Write
    ra=7, d=0x808080, io=Write
    ra=7, d=0xffff00, io=Write
    ra=7, d=0xff0000, io=Write
    ra=7, d=0x00ff00, io=Write
    ra=7, d=0x0000ff, io=Write
    ra=7, d=0x00ffff, io=Write
    ra=7, d=0xff00ff, io=Write
    ra=7, d=0x202020, io=Write
    ra=7, d=0x404040, io=Write
    ra=7, d=0x606060, io=Write
    ra=7, d=0x808080, io=Write
    ra=7, d=0x9f9f9f, io=Write
    ra=7, d=0xbfbfbf, io=Write
    ra=7, d=0xdfdfdf, io=Write
    ra=7, d=0x080808, io=Write
    ra=7, d=0x101010, io=Write
    ra=7, d=0x181818, io=Write
    ra=7, d=0x282828, io=Write
    ra=7, d=0x303030, io=Write
    ra=7, d=0x383838, io=Write
    ra=7, d=0x484848, io=Write
    ra=7, d=0x505050, io=Write
    ra=7, d=0x585858, io=Write
    ra=7, d=0x686868, io=Write
    ra=7, d=0x707070, io=Write
    ra=7, d=0x787878, io=Write
    ra=7, d=0x878787, io=Write
    ra=7, d=0x8f8f8f, io=Write
    ra=7, d=0x979797, io=Write
    ra=7, d=0xa7a7a7, io=Write
    ra=7, d=0xafafaf, io=Write
    ra=7, d=0xb7b7b7, io=Write
    ra=7, d=0xc7c7c7, io=Write
    ra=7, d=0xcfcfcf, io=Write
    ra=7, d=0xd7d7d7, io=Write
    ra=7, d=0xe7e7e7, io=Write
    ra=7, d=0xefefef, io=Write
    ra=7, d=0xf7f7f7, io=Write
    ra=7, d=0x000000, io=Write
    ra=7, d=0x003300, io=Write
    ra=7, d=0x006600, io=Write
    ra=7, d=0x009900, io=Write
    ra=7, d=0x00cc00, io=Write
    ra=7, d=0x00ff00, io=Write
    ra=7, d=0x000033, io=Write
    ra=7, d=0x003333, io=Write
    ra=7, d=0x006633, io=Write
    ra=7, d=0x009933, io=Write
    ra=7, d=0x00cc33, io=Write
    ra=7, d=0x00ff33, io=Write
    ra=7, d=0x000066, io=Write
    ra=7, d=0x003366, io=Write
    ra=7, d=0x006666, io=Write
    ra=7, d=0x009966, io=Write
    ra=7, d=0x00cc66, io=Write
    ra=7, d=0x00ff66, io=Write
    ra=7, d=0x000099, io=Write
    ra=7, d=0x003399, io=Write
    ra=7, d=0x006699, io=Write
    ra=7, d=0x009999, io=Write
    ra=7, d=0x00cc99, io=Write
    ra=7, d=0x00ff99, io=Write
    ra=7, d=0x0000cc, io=Write
    ra=7, d=0x0033cc, io=Write
    ra=7, d=0x0066cc, io=Write
    ra=7, d=0x0099cc, io=Write
    ra=7, d=0x00cccc, io=Write
    ra=7, d=0x00ffcc, io=Write
    ra=7, d=0x0000ff, io=Write
    ra=7, d=0x0033ff, io=Write
    ra=7, d=0x0066ff, io=Write
    ra=7, d=0x0099ff, io=Write
    ra=7, d=0x00ccff, io=Write
    ra=7, d=0x00ffff, io=Write
    ra=7, d=0x330000, io=Write
    ra=7, d=0x333300, io=Write
    ra=7, d=0x336600, io=Write
    ra=7, d=0x339900, io=Write
    ra=7, d=0x33cc00, io=Write
    ra=7, d=0x33ff00, io=Write
    ra=7, d=0x330033, io=Write
    ra=7, d=0x333333, io=Write
    ra=7, d=0x336633, io=Write
    ra=7, d=0x339933, io=Write
    ra=7, d=0x33cc33, io=Write
    ra=7, d=0x33ff33, io=Write
    ra=7, d=0x330066, io=Write
    ra=7, d=0x333366, io=Write
    ra=7, d=0x336666, io=Write
    ra=7, d=0x339966, io=Write
    ra=7, d=0x33cc66, io=Write
    ra=7, d=0x33ff66, io=Write
    ra=7, d=0x330099, io=Write
    ra=7, d=0x333399, io=Write
    ra=7, d=0x336699, io=Write
    ra=7, d=0x339999, io=Write
    ra=7, d=0x33cc99, io=Write
    ra=7, d=0x33ff99, io=Write
    ra=7, d=0x3300cc, io=Write
    ra=7, d=0x3333cc, io=Write
    ra=7, d=0x3366cc, io=Write
    ra=7, d=0x3399cc, io=Write
    ra=7, d=0x33cccc, io=Write
    ra=7, d=0x33ffcc, io=Write
    ra=7, d=0x3300ff, io=Write
    ra=7, d=0x3333ff, io=Write
    ra=7, d=0x3366ff, io=Write
    ra=7, d=0x3399ff, io=Write
    ra=7, d=0x33ccff, io=Write
    ra=7, d=0x33ffff, io=Write
    ra=7, d=0x660000, io=Write
    ra=7, d=0x663300, io=Write
    ra=7, d=0x666600, io=Write
    ra=7, d=0x669900, io=Write
    ra=7, d=0x66cc00, io=Write
    ra=7, d=0x66ff00, io=Write
    ra=7, d=0x660033, io=Write
    ra=7, d=0x663333, io=Write
    ra=7, d=0x666633, io=Write
    ra=7, d=0x669933, io=Write
    ra=7, d=0x66cc33, io=Write
    ra=7, d=0x66ff33, io=Write
    ra=7, d=0x660066, io=Write
    ra=7, d=0x663366, io=Write
    ra=7, d=0x666666, io=Write
    ra=7, d=0x669966, io=Write
    ra=7, d=0x66cc66, io=Write
    ra=7, d=0x66ff66, io=Write
    ra=7, d=0x660099, io=Write
    ra=7, d=0x663399, io=Write
    ra=7, d=0x666699, io=Write
    ra=7, d=0x669999, io=Write
    ra=7, d=0x66cc99, io=Write
    ra=7, d=0x66ff99, io=Write
    ra=7, d=0x6600cc, io=Write
    ra=7, d=0x6633cc, io=Write
    ra=7, d=0x6666cc, io=Write
    ra=7, d=0x6699cc, io=Write
    ra=7, d=0x66cccc, io=Write
    ra=7, d=0x66ffcc, io=Write
    ra=7, d=0x6600ff, io=Write
    ra=7, d=0x6633ff, io=Write
    ra=7, d=0x6666ff, io=Write
    ra=7, d=0x6699ff, io=Write
    ra=7, d=0x66ccff, io=Write
    ra=7, d=0x66ffff, io=Write
    ra=7, d=0x990000, io=Write
    ra=7, d=0x993300, io=Write
    ra=7, d=0x996600, io=Write
    ra=7, d=0x999900, io=Write
    ra=7, d=0x99cc00, io=Write
    ra=7, d=0x99ff00, io=Write
    ra=7, d=0x990033, io=Write
    ra=7, d=0x993333, io=Write
    ra=7, d=0x996633, io=Write
    ra=7, d=0x999933, io=Write
    ra=7, d=0x99cc33, io=Write
    ra=7, d=0x99ff33, io=Write
    ra=7, d=0x990066, io=Write
    ra=7, d=0x993366, io=Write
    ra=7, d=0x996666, io=Write
    ra=7, d=0x999966, io=Write
    ra=7, d=0x99cc66, io=Write
    ra=7, d=0x99ff66, io=Write
    ra=7, d=0x990099, io=Write
    ra=7, d=0x993399, io=Write
    ra=7, d=0x996699, io=Write
    ra=7, d=0x999999, io=Write
    ra=7, d=0x99cc99, io=Write
    ra=7, d=0x99ff99, io=Write
    ra=7, d=0x9900cc, io=Write
    ra=7, d=0x9933cc, io=Write
    ra=7, d=0x9966cc, io=Write
    ra=7, d=0x9999cc, io=Write
    ra=7, d=0x99cccc, io=Write
    ra=7, d=0x99ffcc, io=Write
    ra=7, d=0x9900ff, io=Write
    ra=7, d=0x9933ff, io=Write
    ra=7, d=0x9966ff, io=Write
    ra=7, d=0x9999ff, io=Write
    ra=7, d=0x99ccff, io=Write
    ra=7, d=0x99ffff, io=Write
    ra=7, d=0xcc0000, io=Write
    ra=7, d=0xcc3300, io=Write
    ra=7, d=0xcc6600, io=Write
    ra=7, d=0xcc9900, io=Write
    ra=7, d=0xcccc00, io=Write
    ra=7, d=0xccff00, io=Write
    ra=7, d=0xcc0033, io=Write
    ra=7, d=0xcc3333, io=Write
    ra=7, d=0xcc6633, io=Write
    ra=7, d=0xcc9933, io=Write
    ra=7, d=0xcccc33, io=Write
    ra=7, d=0xccff33, io=Write
    ra=7, d=0xcc0066, io=Write
    ra=7, d=0xcc3366, io=Write
    ra=7, d=0xcc6666, io=Write
    ra=7, d=0xcc9966, io=Write
    ra=7, d=0xcccc66, io=Write
    ra=7, d=0xccff66, io=Write
    ra=7, d=0xcc0099, io=Write
    ra=7, d=0xcc3399, io=Write
    ra=7, d=0xcc6699, io=Write
    ra=7, d=0xcc9999, io=Write
    ra=7, d=0xcccc99, io=Write
    ra=7, d=0xccff99, io=Write
    ra=7, d=0xcc00cc, io=Write
    ra=7, d=0xcc33cc, io=Write
    ra=7, d=0xcc66cc, io=Write
    ra=7, d=0xcc99cc, io=Write
    ra=7, d=0xcccccc, io=Write
    ra=7, d=0xccffcc, io=Write
    ra=7, d=0xcc00ff, io=Write
    ra=7, d=0xcc33ff, io=Write
    ra=7, d=0xcc66ff, io=Write
    ra=7, d=0xcc99ff, io=Write
    ra=7, d=0xccccff, io=Write
    ra=7, d=0xccffff, io=Write
    ra=7, d=0xff0000, io=Write
    ra=7, d=0xff3300, io=Write
    ra=7, d=0xff6600, io=Write
    ra=7, d=0xff9900, io=Write
    ra=7, d=0xffcc00, io=Write
    ra=7, d=0xffff00, io=Write
    ra=7, d=0xff0033, io=Write
    ra=7, d=0xff3333, io=Write
    ra=7, d=0xff6633, io=Write
    ra=7, d=0xff9933, io=Write
    ra=7, d=0xffcc33, io=Write
    ra=7, d=0xffff33, io=Write
    ra=7, d=0xff0066, io=Write
    ra=7, d=0xff3366, io=Write
    ra=7, d=0xff6666, io=Write
    ra=7, d=0xff9966, io=Write
    ra=7, d=0xffcc66, io=Write
    ra=7, d=0xffff66, io=Write
    ra=7, d=0xff0099, io=Write
    ra=7, d=0xff3399, io=Write
    ra=7, d=0xff6699, io=Write
    ra=7, d=0xff9999, io=Write
    ra=7, d=0xffcc99, io=Write
    ra=7, d=0xffff99, io=Write
    ra=7, d=0xff00cc, io=Write
    ra=7, d=0xff33cc, io=Write
    ra=7, d=0xff66cc, io=Write
    ra=7, d=0xff99cc, io=Write
    ra=7, d=0xffcccc, io=Write
    ra=7, d=0xffffcc, io=Write
    ra=7, d=0xff00ff, io=Write
    ra=7, d=0xff33ff, io=Write
    ra=7, d=0xff66ff, io=Write
    ra=7, d=0xff99ff, io=Write
    ra=7, d=0xffccff, io=Write
    ra=7, d=0xffffff, io=Write
    seq=Jump, brch=bitmap_palette_ready

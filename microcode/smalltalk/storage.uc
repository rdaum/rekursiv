; Heap construction and binding exchange; this file does not perform disk I/O.
; Primitive 70 new takes no arguments; 71 new: takes an indexed count; 72 become:
; takes the other object; 79 newMethod:header: takes byte count then header.
; Success uses VR5/send_result; validation failures use R15/primitive_failed.
;
; Class component 4 is the tagged instance specification: low 11 bits fixed
; fields, bit 12 indexable, bit 13 words, bit 14 pointers. new requires a fixed
; class; new: requires indexability and accepts an unsigned 16-bit count through
; positive_value. R2 selects descriptor kind, R4 total fields, R6 physical size
; including descriptor, R7 initialization cursor, R10 fixed count.
; All guest allocations here use scan=1. Raw descriptors/digits do not look like
; references, while pointers must be replaced with stored guest NIL before use.
;
; newMethod requires specification 0x1000 (indexable bytes, no fixed fields).
; VR3 roots the tagged header, R11 its literal count, R4 byte count. Allocate
; 2+literal_count+byte_count components; initialize literals to NIL and bytes to
; zero. Header-derived guest length still reserves two bytes per tagged prefix
; word, despite wider physical components. VR6 itself supplies the allocated
; object's class, allowing compatible subclasses. become delegates atomic publication
; to generic Exchange; it does not walk or rewrite references in microcode.
;
; Object allocation uses class instance specifications from guest memory. The
; allocator sees only a class reference, physical size, and generic scan flag.
; Pointer fields start at guest nil; byte/word fields start at raw zero.
primitive_new:
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=new_indexed_arity
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    seq=Jump, brch=new_specification
new_indexed_arity:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
new_specification:
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=7, s=Branch, brch=2047, alu=And, rb=10, ldrb
    d=0, r=Bus, rb=2, ldrb
    ra=7, s=Bus, d=0x4000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=new_kind_ready
    d=1, r=Bus, rb=2, ldrb
    ra=7, s=Bus, d=0x2000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=new_kind_ready
    d=2, r=Bus, rb=2, ldrb
new_kind_ready:
    ra=7, s=Bus, d=0x1000, alu=And, rb=7, ldrb
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=new_indexed_count
    ra=7, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ; Blue Book new creates pointer or word objects; byte allocation uses new:.
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=new_fixed_count
    d=1, r=Bus, rb=2, ldrb
new_fixed_count:
    ra=10, rb=4, ldrb, seq=Jump, brch=new_allocate
new_indexed_count:
    ra=7, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=new_count_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
new_count_ready:
    ra=4, rb=10, alu=Add, ldq
    d=Q, r=Bus, rb=4, ldrb
new_allocate:
    ra=4, s=Bus, d=0xffff0000, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, s=Branch, brch=1, alu=Add, rb=6, ldrb
    read=Vr, vr=6
    d=Object, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ra=4, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=2, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=new_descriptor
    ra=0, shift=Left, rb=0, ldrb
; R0 holds encoded byte length: pointer/word counts contribute 8 per field,
; byte counts 4. OR in kind R2, then initialize components 2 through R6.
new_descriptor:
    ra=0, rb=2, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    d=2, r=Bus, rb=7, ldrb
new_initialize:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_result
    d=Register, ra=7, idx=Load
    ra=2, flags
    seq=ConditionalJump, cc=!Zero, brch=new_zero_field
    d=NIL, mem=Write
    seq=Jump, brch=new_next_field
new_zero_field:
    d=0, mem=Write
new_next_field:
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=new_initialize

; Primitive 79 allocates the split physical CompiledMethod representation.
; Guest header/literal lengths and IP offsets retain Blue Book meanings.
; The receiver must describe indexable byte instances without fixed fields.
; Validate the class before reading its specification: an ordinary receiver or
; a word/byte object must fail without an invalid component access or allocation.
; This accepts compatible byte classes, including CompiledMethod subclasses;
; no class identity or language layout is built into the allocator itself.
primitive_new_method:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=7, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=7, s=Bus, d=0x1000, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=Object, r=Bus, s=Branch, brch=63, alu=And, rb=11, ldrb
    idx=Decrement
    mem=Read
    d=Object, ldsym
    d=new_method_count, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
; Keep the rooted header VR3 while allocation may collect. Physical prefix
; is descriptor+header+literals, while guest prefix counts two bytes per word.
new_method_count:
    ra=4, rb=11, alu=Add, ldq
    d=Q, r=Bus, s=Branch, brch=2, alu=Add, rb=6, ldrb
    read=Vr, vr=6
    d=Object, page=Allocate, size=ra, ra=6, scan=1
    d=Object, ldvr, vr=5
    ra=11, s=Branch, brch=1, alu=Add, shift=Left, rb=0, ldrb
    ra=0, rb=4, alu=Add, ldq
    d=Q, r=Bus, shift=Left, rb=0, ldrb
    ra=0, shift=Left, rb=0, ldrb
    ra=0, s=Branch, brch=3, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    idx=Increment
    read=Vr, vr=3
    d=Object, mem=Write
    d=3, r=Bus, rb=7, ldrb
    ra=11, s=Branch, brch=2, alu=Add, rb=11, ldrb
new_method_fields:
    ra=6, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=send_result
    d=Register, ra=7, idx=Load
    ra=11, rb=7, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=new_method_byte
    d=NIL, mem=Write
    seq=Jump, brch=new_method_next
new_method_byte:
    d=0, mem=Write
new_method_next:
    ra=7, s=Branch, brch=1, alu=Add, rb=7, ldrb
    seq=Jump, brch=new_method_fields

; Primitive 72 exchanges object bindings through a generic OBJEKT operation.
; All guest objects use scanned identities. The operation keeps references
; unchanged while exchanging class, size, flags, and body, including cold data.
primitive_become:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=6
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=Symbol, brch=primitive_failed
    d=Object, ldvr, vr=3
    read=Vr, vr=6
    d=Object, page=Exchange, vr=3
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

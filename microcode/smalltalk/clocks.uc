; Guest clock primitives use little-endian byte objects. The external device
; knows only UTC seconds, monotonic milliseconds, and an absolute 64-bit timer.
; Epoch conversion, byte packing, deadline expansion, and semaphore roots are
; runtime work. All operand checks precede guest writes or timer cancellation.
primitive_clock:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    d=clock_read, r=Bus, rb=6, ldrb, seq=Jump, brch=clock_byte_target
clock_read:
    d=0x200, r=Bus, rb=7, ldrb
    ra=0, s=Branch, brch=98, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=clock_read_device
    d=0x208, r=Bus, rb=7, ldrb
clock_read_device:
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=98, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=clock_store
    ; 1901-01-01 to 1970-01-01 is 2,177,452,800 seconds. The guest
    ; clock is the low 32 bits, including its specified unsigned rollover.
    ra=4, s=Bus, d=2177452800, alu=Add, rb=4, ldrb
clock_store:
    read=Vr, vr=4
    d=Object, page=Fetch
    d=2, idx=Load
    d=4, r=Bus, rb=3, ldrb
clock_store_byte:
    ra=4, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=clock_result
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb
    idx=Increment
    seq=Jump, brch=clock_store_byte
clock_result:
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

; Validate a byte object in VR4, leaving it selected. Return through R6.
clock_byte_target:
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=7, ldrb
    ra=7, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Register, ra=6, seq=Bus

primitive_timer:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=timer_cancel
    d=Object, page=Fetch
    read=Type
    d=0xa000000013, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=timer_read_bytes, r=Bus, rb=6, ldrb, seq=Jump, brch=clock_byte_target
timer_read_bytes:
    d=5, idx=Load
    d=0, r=Bus, rb=2, ldrb
    d=4, r=Bus, rb=3, ldrb
timer_read_byte:
    mem=Read
    d=Object, r=Bus, rb=4, ldrb
    ra=4, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, shift=Left, rb=2, ldrb
    ra=2, rb=4, alu=Or, ldq
    d=Q, r=Bus, rb=2, ldrb
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=timer_sample
    idx=Decrement
    seq=Jump, brch=timer_read_byte
timer_sample:
    d=0x208, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    d=0x20c, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=5, ldrb
    ; Expand a modular 32-bit deadline within the signed half-period. A
    ; nonpositive delta is already due; a positive wrap increments the epoch.
    ra=2, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=timer_due
    seq=ConditionalJump, cc=Zero, brch=timer_due
    seq=ConditionalJump, cc=Carry, brch=timer_arm
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    seq=Jump, brch=timer_arm
timer_due:
    ra=4, rb=2, ldrb
timer_arm:
    ; Cancel the old timer and any pending old expiry before publishing a new
    ; recipient. No event dispatch can run halfway through this primitive.
    d=0x218, r=Bus, rb=7, ldrb
    ra=7, d=0, io=Write
    d=28, r=Bus, rb=7, ldrb
    read=Vr, vr=3
    ra=7, d=Object, ldroot
    d=0x210, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=0x214, r=Bus, rb=7, ldrb
    ra=5, ldq
    ra=7, d=Q, io=Write
    d=0x218, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    seq=Jump, brch=clock_result
timer_cancel:
    d=0x218, r=Bus, rb=7, ldrb
    ra=7, d=0, io=Write
    d=28, r=Bus, rb=7, ldrb
    ra=7, d=NIL, ldroot
    seq=Jump, brch=clock_result

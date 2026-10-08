; Device primitives retain IP/SP and use the ordinary guest send result path.
device_receiver_result:
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

; Return a guest Point; the external device only returns numeric coordinates.
primitive_point_device:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0x510, r=Bus, rb=7, ldrb
    ra=0, s=Branch, brch=106, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_point_read
    d=0x300, r=Bus, rb=7, ldrb
device_point_read:
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    ra=7, s=Branch, brch=4, alu=Add, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=3, ldrb
    d=SPECIAL_12, page=Allocate, size=4, scan=1
    d=Object, ldvr, vr=5
    d=1, idx=Load
    d=32, mem=Write
    idx=Increment
    d=0x1000, mem=Write
    d=device_point_hash, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
device_point_hash:
    d=3, idx=Load
    ra=2, estk=Compact, compact=2
    d=Estk, mem=Write
    idx=Increment
    ra=3, estk=Compact, compact=2
    d=Estk, mem=Write, seq=Jump, brch=send_result

primitive_clock:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0x208, r=Bus, rb=7, ldrb
    ra=0, s=Branch, brch=135, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_milliseconds
    d=0x200, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=4, s=Bus, d=2177452800, alu=Add, rb=4, ldrb, seq=Jump, brch=positive_result
device_milliseconds:
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb, seq=Jump, brch=positive_result

; Squeak keeps registrations in its special-object array. Runtime roots mirror
; recipients only for boundary dispatch; replacements are published together.
primitive_semaphore_registration:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_18, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=device_semaphore_valid
    d=NIL, ldvr, vr=3
device_semaphore_valid:
    d=27, r=Bus, rb=2, ldrb
    d=25, r=Bus, rb=3, ldrb
    ra=0, s=Branch, brch=93, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_semaphore_store
    d=10, r=Bus, rb=2, ldrb
    d=33, r=Bus, rb=3, ldrb
    ra=0, s=Branch, brch=134, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_semaphore_store
    d=30, r=Bus, rb=2, ldrb
    d=20, r=Bus, rb=3, ldrb
device_semaphore_store:
    read=Vr, vr=3
    ra=2, d=Object, ldroot
    d=0, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=Register, ra=3, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write, seq=Jump, brch=device_receiver_result

primitive_timer:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_18, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=device_timer_sample
    d=NIL, ldvr, vr=3
    d=0, r=Bus, rb=2, ldrb
device_timer_sample:
    d=0x218, r=Bus, rb=7, ldrb
    ra=7, d=0, io=Write
    d=28, r=Bus, rb=7, ldrb
    read=Vr, vr=3
    ra=7, d=Object, ldroot
    d=0, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=32, idx=Load
    read=Vr, vr=3
    d=Object, mem=Write
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=device_receiver_result
    d=0x208, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    d=0x20c, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=5, ldrb
    ra=2, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=device_timer_due
    seq=ConditionalJump, cc=Zero, brch=device_timer_due
    seq=ConditionalJump, cc=Carry, brch=device_timer_arm
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    seq=Jump, brch=device_timer_arm
device_timer_due:
    ra=4, rb=2, ldrb
device_timer_arm:
    d=0x210, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=0x214, r=Bus, rb=7, ldrb
    ra=5, ldq
    ra=7, d=Q, io=Write
    d=0x218, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write, seq=Jump, brch=device_receiver_result

primitive_interrupt_key:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=12, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, ldroot, seq=Jump, brch=device_receiver_result

primitive_buttons:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=device_buttons_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=input_modifiers
device_buttons_ready:
    d=13, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, s=Branch, brch=7, alu=And, rb=2, ldrb
    ra=2, rb=4, alu=Or, ldq, seq=Jump, brch=integer_result
primitive_key:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=7, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=2, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=device_key_empty
    d=29, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=5, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=3, ldrb
    ra=3, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=109, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_result
    ra=3, s=Branch, brch=1, alu=Add, rb=3, ldrb
    ra=3, s=Branch, brch=63, alu=And, ldq
    ra=7, d=Q, ldroot
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, ldq
    d=7, r=Bus, rb=7, ldrb
    ra=7, d=Q, ldroot, seq=Jump, brch=positive_result
device_key_empty:
    d=NIL, ldvr, vr=5, seq=Jump, brch=send_result

primitive_free_bytes:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=112, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_free_bytes
    gc=Collect
device_free_bytes:
    read=FreeWords
    d=Object, r=Bus, shift=Left, rb=4, ldrb
    ra=4, shift=Left, rb=4, ldrb, seq=Jump, brch=positive_result

primitive_low_space_threshold:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=device_threshold_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
device_threshold_ready:
    d=11, r=Bus, rb=7, ldrb
    ra=4, ldq
    ra=7, d=Q, ldroot, seq=Jump, brch=device_receiver_result

; Sample free capacity every 1024 guest bytecodes. Collect before declaring
; low space, then signal once and disarm until the guest registers a new limit.
; Four guest bytes correspond to one resident raw word; metadata costs capacity.
check_low_space:
    d=4, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, flags
    seq=ConditionalJump, cc=!Zero, brch=low_space_signal
    d=14, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=2, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=low_space_sample
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, ldq
    ra=7, d=Q, ldroot, seq=Jump, brch=check_device_events
low_space_sample:
    ra=7, d=1023, ldroot
    d=11, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=2, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=check_device_events
    d=30, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=check_device_events
    d=0, r=Bus, rb=3, ldrb
low_space_read:
    read=FreeWords
    d=Object, r=Bus, shift=Left, rb=4, ldrb
    ra=4, shift=Left, rb=4, ldrb
    ra=4, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=check_device_events
    ra=3, flags
    seq=ConditionalJump, cc=!Zero, brch=low_space_signal
    d=1, r=Bus, rb=3, ldrb
    gc=Collect
    seq=Jump, brch=low_space_read
low_space_signal:
    d=4, r=Bus, rb=7, ldrb
    ra=7, d=0, ldroot
    d=11, r=Bus, rb=7, ldrb
    ra=7, d=0, ldroot
    d=30, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldvr, vr=6, seq=Jump, brch=event_recipient_ready

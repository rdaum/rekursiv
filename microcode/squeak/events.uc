; Bytecode-boundary interrupt service. The CPU owns all buffering, semaphore
; signalling and scheduling. External input is a FIFO of physical transitions.
; R8..R14 and VR0..2 are activation caches and survive this dispatcher.
; Roots 5/6/7 are keyboard read/write/count, 12 interrupt key, 13 physical held
; keys, 29 a 64-key raw ring. Roots 10/27/28 are semaphore recipients.
check_device_events:
    seq=ConditionalJump, cc=!Interrupt, brch=events_done
    d=0x100, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=1, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=input_event
    ra=4, s=Branch, brch=2, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=timer_event
    ; Storage completion has no registered Squeak recipient yet.
    d=0x104, r=Bus, rb=7, ldrb
    ra=7, d=4, io=Write, seq=Jump, brch=events_done
timer_event:
    d=28, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldvr, vr=6
    ra=7, d=NIL, ldroot
    d=0, r=Bus, rb=7, ldrb
    ra=7, d=Root, page=Fetch
    d=32, idx=Load
    d=NIL, mem=Write
    d=0x104, r=Bus, rb=7, ldrb
    ra=7, d=2, io=Write
    seq=Jump, brch=event_recipient_ready
input_event:
    d=0x314, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=3, ldrb
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_ack
    d=0x318, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=128, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=input_physical_key
    ra=3, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=input_ack
    d=input_character, r=Bus, rb=6, ldrb, seq=Jump, brch=input_modifiers
input_character:
    ; ASCII translation is a guest driver operation. Caps-lock XOR Shift
    ; changes letters; only Shift changes punctuation and digits.
    ra=2, s=Branch, brch=97, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=input_punctuation
    ra=2, s=Branch, brch=122, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=input_punctuation
    d=13, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=5, ldrb
    ra=5, s=Branch, brch=24, alu=Rotate, rb=5, ldrb
    ra=5, rb=4, alu=Xor, ldq
    r=Q, s=Branch, brch=8, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_encoded
    ra=2, s=Branch, brch=32, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=input_encoded
input_punctuation:
    ra=4, s=Branch, brch=8, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_encoded
    ra=2, s=Branch, brch=49, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_49
    ra=2, s=Branch, brch=50, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_50
    ra=2, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_51
    ra=2, s=Branch, brch=52, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_52
    ra=2, s=Branch, brch=53, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_53
    ra=2, s=Branch, brch=54, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_54
    ra=2, s=Branch, brch=55, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_55
    ra=2, s=Branch, brch=56, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_56
    ra=2, s=Branch, brch=57, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_57
    ra=2, s=Branch, brch=48, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_48
    ra=2, s=Branch, brch=45, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_45
    ra=2, s=Branch, brch=61, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_61
    ra=2, s=Branch, brch=91, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_91
    ra=2, s=Branch, brch=93, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_93
    ra=2, s=Branch, brch=92, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_92
    ra=2, s=Branch, brch=59, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_59
    ra=2, s=Branch, brch=39, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_39
    ra=2, s=Branch, brch=44, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_44
    ra=2, s=Branch, brch=46, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_46
    ra=2, s=Branch, brch=47, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_47
    ra=2, s=Branch, brch=96, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_shift_96
    seq=Jump, brch=input_encoded
input_shift_49:
    d=33, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_50:
    d=64, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_51:
    d=35, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_52:
    d=36, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_53:
    d=37, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_54:
    d=94, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_55:
    d=38, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_56:
    d=42, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_57:
    d=40, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_48:
    d=41, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_45:
    d=95, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_61:
    d=43, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_91:
    d=123, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_93:
    d=125, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_92:
    d=124, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_59:
    d=58, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_39:
    d=34, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_44:
    d=60, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_46:
    d=62, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_47:
    d=63, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_shift_96:
    d=126, r=Bus, rb=2, ldrb, seq=Jump, brch=input_encoded
input_encoded:
    ra=4, s=Branch, brch=5, alu=Rotate, rb=4, ldrb
    ra=4, rb=2, alu=Or, ldrb
    d=12, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, s=Register, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_interrupt_key
    d=7, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=3, ldrb
    ra=3, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=input_ring_full
    d=29, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldsym
    d=NIL, seq=ConditionalJump, cc=!Symbol, brch=input_ring_ready
    d=SPECIAL_7, page=Allocate, size=65, scan=0
    ra=7, d=Object, ldroot, ldsym
    d=1, idx=Load
    d=0, mem=Write
input_ring_ready:
    d=Symbol, page=Fetch
    d=6, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    d=Register, ra=2, mem=Write
    ra=4, s=Branch, brch=1, alu=Add, rb=4, ldrb
    ra=4, s=Branch, brch=63, alu=And, ldq
    ra=7, d=Q, ldroot
    ra=3, s=Branch, brch=1, alu=Add, ldq
    d=7, r=Bus, rb=7, ldrb
    ra=7, d=Q, ldroot
    seq=Jump, brch=input_ack
input_ring_full:
    ; Leave the key in the external FIFO until the guest consumes a character,
    ; but do not let that backpressure starve a due timer semaphore.
    d=0x100, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, s=Branch, brch=2, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=timer_event
    seq=Jump, brch=events_done
input_physical_key:
    ra=2, s=Branch, brch=142, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=input_ack
    ra=2, s=Branch, brch=128, alu=Sub, cin=One, rb=2, ldrb
    d=1, r=Bus, s=Register, rb=2, alu=Rotate, ldq
    d=Q, r=Bus, rb=2, ldrb
    d=13, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=4, ldrb
    ra=3, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_key_down
    ra=2, s=Bus, d=0xffffffff, alu=Xor, rb=2, ldrb
    ra=2, rb=4, alu=And, ldq, seq=Jump, brch=input_key_store
input_key_down:
    ra=2, rb=4, alu=Or, ldq
input_key_store:
    ra=7, d=Q, ldroot
input_ack:
    d=27, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldvr, vr=6
    seq=Jump, brch=input_pop
input_interrupt_key:
    d=10, r=Bus, rb=7, ldrb
    ra=7, d=Root, ldvr, vr=6
input_pop:
    d=0x324, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    d=0x104, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
event_recipient_ready:
    d=NIL, ldsym
    read=Vr, vr=6
    d=Object, seq=ConditionalJump, cc=Symbol, brch=event_delivered
    d=85, r=Bus, rb=0, ldrb
    d=1, r=Bus, rb=1, ldrb, seq=Jump, brch=scheduler_load
event_delivered:
    d=0, ldvr, vr=3
    d=0, ldvr, vr=4
    d=0, ldvr, vr=5
    d=0, ldvr, vr=6
    d=0, ldvr, vr=7
    seq=Jump, brch=check_process_switch
events_done:
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=cycle
scheduler_idle:
    seq=ConditionalJump, cc=!Interrupt, brch=scheduler_idle
    seq=Jump, brch=check_device_events

; Physical held bits -> Squeak modifier bits. Returns R4 through R6;
; R2/R3 survive while decoding a character event.
input_modifiers:
    d=13, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, rb=5, ldrb
    d=0, r=Bus, rb=4, ldrb
    ra=5, s=Branch, brch=768, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_control
    d=8, r=Bus, rb=4, ldrb
input_control:
    ra=5, s=Branch, brch=1024, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_option
    ra=4, s=Branch, brch=16, alu=Or, rb=4, ldrb
input_option:
    ra=5, s=Branch, brch=12288, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_command
    ra=4, s=Branch, brch=32, alu=Or, rb=4, ldrb
input_command:
    ra=5, s=Bus, d=16384, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=input_modifiers_done
    ra=4, s=Branch, brch=64, alu=Or, rb=4, ldrb
input_modifiers_done:
    d=Register, ra=6, seq=Bus

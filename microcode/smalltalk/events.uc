; Boundary dispatcher and primitive 93 input semaphore registration.
; check_device_events runs after pending switches and low-space checks. Drain
; already-buffered input notifications first, then inspect counted device IRQs.
; Input packet handling lives in input.uc. Timer delivery releases root28 once;
; storage notification uses root29 component 2 (currently a reserved recipient).
;
; For a non-nil recipient, scheduler_load receives VR6, R0=85 and R1=1. It must
; return to event_delivered, not send_result: an asynchronous signal has no caller
; operands. event_delivered clears scratch VRs and revisits check_process_switch
; before checking more events. Unknown status bits or inconsistent input IRQs
; stop through bad_state. Device faults remain machine faults.
;
; If no events remain, root31 chooses cycle versus scheduler_idle. Idle polls
; Interrupt while retiring microinstructions, allowing debugger stops and device
; progress. Primitive 93 accepts one Semaphore-or-nil argument and returns the
; receiver; it replaces root27 only after validation. It does not clear queued
; input words; subsequent notification delivery uses the new registration.
;
; Machine event delivery. Devices expose counted notifications, never guest
; references. Status 0x100 has bits input/timer/storage. A write to
; 0x104 consumes one notification from each selected source. A simultaneous
; arrival survives acknowledgement. Roots27/28 hold input/timer recipients;
; root29 holds private device state, including the input buffer and reserved
; storage recipient (component2).
; Slot30 belongs to lowspace.uc and slot31 is the raw idle flag. All slots
; belong to this runtime, not the RTL.
;
; Entry is after context materialization and pending process switches. Numeric
; scratch is R0..R7; R8..R14 and VR0..VR2 retain the current activation caches.
; Input packets enter the machine-owned word ring before acknowledgement.
; Other recipients are rooted in VR6 before acknowledgement. Collection can then
; interrupt delivery without asking the device to preserve a guest reference.
check_device_events:
    seq=Jump, brch=input_notifications
input_notifications_done:
    seq=ConditionalJump, cc=!Interrupt, brch=events_done
    d=0x100, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=4, s=Bus, d=0xfffffff8, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bad_state
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=events_done
    d=27, r=Bus, rb=5, ldrb
    d=1, r=Bus, rb=6, ldrb
event_select:
    ra=4, rb=6, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=event_selected
    ra=5, s=Branch, brch=1, alu=Add, rb=5, ldrb
    ra=6, shift=Left, rb=6, ldrb
    seq=Jump, brch=event_select
event_selected:
    ra=5, s=Branch, brch=27, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=input_packet_event
    ra=5, d=Root, ldvr, vr=6
    ra=5, s=Branch, brch=29, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=event_acknowledge
    read=Vr, vr=6
    d=Object, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=event_acknowledge
    d=Symbol, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=6
event_acknowledge:
    d=0x104, r=Bus, rb=7, ldrb
    ra=6, ldq
    ra=7, d=Q, io=Write
    ; Timer registrations are one-shot. VR6 retains the recipient while its
    ; registration root is released, including across collection during signal.
    ra=5, s=Branch, brch=28, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=event_recipient_ready
    ra=5, d=NIL, ldroot
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
    ; A retiring microcode loop leaves context and heap untouched. It can be
    ; stopped by the debugger and observes a level IRQ without host scheduling.
    seq=ConditionalJump, cc=!Interrupt, brch=scheduler_idle
    seq=Jump, brch=check_device_events

; InputState>>primInputSemaphore: accepts Semaphore or nil and returns self.
; Replacing/clearing the root is atomic at retirement. Events observed at the
; next boundary use the new registration; the device has no old guest pointer.
primitive_input_semaphore:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=input_semaphore_store
    d=Object, page=Fetch
    read=Type
    d=0xa000000013, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
input_semaphore_store:
    d=27, r=Bus, rb=7, ldrb
    read=Vr, vr=3
    ra=7, d=Object, ldroot
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

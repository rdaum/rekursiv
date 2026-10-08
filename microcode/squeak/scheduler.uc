; Squeak ProcessScheduler/Process/Semaphore operations, primitives 85..89.
; Queue operations and switching execute solely through OBJEKT fields.
; Pending process is rooted in expression stack slot 1 until bytecode boundary.
; Guest fields begin at component 3. Process fields: next 3, context 4,
; priority 5, list 6. Scheduler: lists 3, active 4. Queue: first 3, last 4.
; events.uc delivers device notifications through scheduler_load with R1=1.
primitive_scheduler:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=89, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_result
scheduler_load:
    d=SPECIAL_3, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    ra=0, s=Branch, brch=87, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_resume_receiver
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_suspend
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_18, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scheduler_failed
    ra=0, s=Branch, brch=85, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_signal
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scheduler_failed
    ra=2, flags
    seq=ConditionalJump, cc=Sign, brch=scheduler_failed
    seq=ConditionalJump, cc=Zero, brch=scheduler_wait
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=scheduler_result
scheduler_signal:
    d=3, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scheduler_wake_waiter
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scheduler_failed
    ra=2, flags
    seq=ConditionalJump, cc=Sign, brch=scheduler_failed
    ra=2, s=Bus, d=1073741823, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=scheduler_failed
    ra=2, s=Branch, brch=1, alu=Add, estk=Compact, compact=2
    d=Estk, mem=Write
    seq=Jump, brch=scheduler_result
scheduler_wake_waiter:
    read=Vr, vr=6
    d=Object, ldvr, vr=7
    d=scheduler_resume, r=Bus, rb=6, ldrb, seq=Jump, brch=queue_remove
scheduler_wait:
    read=Vr, vr=6
    d=Object, ldvr, vr=7
    read=Vr, vr=4
    d=Object, ldvr, vr=5
    d=scheduler_choose, r=Bus, rb=6, ldrb, seq=Jump, brch=queue_append
scheduler_suspend:
    read=Vr, vr=4
    d=Object, ldsym
    read=Vr, vr=6
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    seq=Jump, brch=scheduler_choose
scheduler_resume_receiver:
    read=Vr, vr=6
    d=Object, ldvr, vr=5
scheduler_resume:
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, flags
    seq=ConditionalJump, cc=!Zero, brch=scheduler_chosen
    read=Vr, vr=4
    d=Object, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_enqueue
    seq=ConditionalJump, cc=Sign, brch=scheduler_enqueue
    d=1, esp=Bus
    read=Vr, vr=5
    d=Object, estk=Bus
    d=1, sp=Bus
    d=0, esp=Bus
    read=Vr, vr=4
    d=Object, ldvr, vr=5
scheduler_enqueue:
    d=scheduler_result, r=Bus, rb=6, ldrb
scheduler_sleep:
    read=Vr, vr=5
    d=Object, page=Fetch
    d=5, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=3, ldrb
    ra=3, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=2, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    seq=Jump, brch=queue_append

queue_append:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=queue_append_empty
    idx=Increment
    mem=Read
    d=Object, page=Fetch
    d=3, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
    read=Vr, vr=7
    d=Object, page=Fetch
    seq=Jump, brch=queue_append_last
queue_append_empty:
    read=Vr, vr=5
    d=Object, mem=Write
queue_append_last:
    d=4, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
    read=Vr, vr=5
    d=Object, page=Fetch
    d=3, idx=Load
    d=NIL, mem=Write
    d=6, idx=Load
    read=Vr, vr=7
    d=Object, mem=Write
    d=Register, ra=6, seq=Bus
queue_remove:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    d=Symbol, mem=Write
    d=NIL, seq=ConditionalJump, cc=!Symbol, brch=queue_remove_link
    idx=Increment
    d=NIL, mem=Write
queue_remove_link:
    read=Vr, vr=5
    d=Object, page=Fetch
    d=3, idx=Load
    d=NIL, mem=Write
    d=6, idx=Load
    d=NIL, mem=Write
    d=Register, ra=6, seq=Bus
scheduler_choose:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=3, ldrb
scheduler_choose_priority:
    ra=3, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_no_runnable
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=3, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scheduler_choose_found
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb
    seq=Jump, brch=scheduler_choose_priority
scheduler_choose_found:
    d=scheduler_chosen, r=Bus, rb=6, ldrb, seq=Jump, brch=queue_remove
scheduler_chosen:
    d=1, esp=Bus
    read=Vr, vr=5
    d=Object, estk=Bus
    d=1, sp=Bus
    d=0, esp=Bus
scheduler_result:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=event_delivered
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_suspend_result
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
scheduler_suspend_result:
    d=NIL, ldvr, vr=5, seq=Jump, brch=send_result
scheduler_no_runnable:
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=1, ldroot
    seq=Jump, brch=scheduler_result
scheduler_failed:
    ra=1, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    seq=Jump, brch=bad_state

check_process_switch:
    d=Sp, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=check_idle
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=7
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=0, ldroot
    d=SPECIAL_3, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    read=Vr, vr=4
    d=Object, page=Fetch
    d=4, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    read=Vr, vr=3
    d=Object, page=Fetch
    d=4, idx=Load
    read=Vr, vr=7
    d=Object, mem=Write
    read=Vr, vr=7
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldvr, vr=0
    d=NIL, mem=Write
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

check_idle:
    seq=Jump, brch=check_low_space

; Guest ProcessScheduler, LinkedList, Process and Semaphore objects implement
; scheduling policy. Their pointer fields are ordinary OBJEKT components.
; Scheduler fields: lists=2, active=3. Process: next=2, context=3, priority=4,
; list=5. Queue: first=2, last=3. Semaphore adds excessSignals=4.
;
; Primitive entry keeps VR6 as the receiver/result. VR3 scheduler, VR4 active
; process, VR5 candidate/link, VR7 queue. R0 primitive, R2/R3 numeric scratch,
; R6 helper continuation. A pending process is rooted in hardware expression
; stack slot1 with SP=1 and ESP=0 until the completed-bytecode boundary.
primitive_scheduler:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=0, s=Branch, brch=89, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_result
scheduler_load:
    d=0xa000000004, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    ra=0, s=Branch, brch=87, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_resume_receiver
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_suspend
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Type
    d=0xa000000013, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scheduler_failed
    ra=0, s=Branch, brch=85, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_signal
    d=4, idx=Load
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
    d=2, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=scheduler_wake_waiter
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=scheduler_failed
    ra=2, flags
    seq=ConditionalJump, cc=Sign, brch=scheduler_failed
    ra=2, s=Branch, brch=16383, alu=Sub, cin=One, flags
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
    d=Object, r=Bus, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ; An idle processor has no active runnable process to preempt or enqueue.
    ; Keep the blocked process on its semaphore queue until its own signal.
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=Root, r=Bus, flags
    seq=ConditionalJump, cc=!Zero, brch=scheduler_chosen
    read=Vr, vr=4
    d=Object, page=Fetch
    d=4, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=3, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bad_state
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_enqueue
    seq=ConditionalJump, cc=Sign, brch=scheduler_enqueue
    ; New process preempts. Queue the old process, and defer the actual switch.
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
    d=4, idx=Load
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=bad_state
    seq=ConditionalJump, cc=Sign, brch=bad_state
    read=Vr, vr=3
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb
    ra=3, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bad_state
    ra=2, s=Branch, brch=1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    seq=Jump, brch=queue_append

; Tail-call queue helpers preserve R0, R1, R8/R9 and all caller numeric caches.
queue_append:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=NIL, ldsym
    d=Object, seq=ConditionalJump, cc=Symbol, brch=queue_append_empty
    idx=Increment
    mem=Read
    d=Object, page=Fetch
    d=2, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
    read=Vr, vr=7
    d=Object, page=Fetch
    seq=Jump, brch=queue_append_last
queue_append_empty:
    read=Vr, vr=5
    d=Object, mem=Write
queue_append_last:
    d=3, idx=Load
    read=Vr, vr=5
    d=Object, mem=Write
    read=Vr, vr=5
    d=Object, page=Fetch
    d=2, idx=Load
    d=NIL, mem=Write
    d=5, idx=Load
    read=Vr, vr=7
    d=Object, mem=Write
    d=Register, ra=6, seq=Bus
queue_remove:
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldsym
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
    d=Symbol, mem=Write
    d=NIL, seq=ConditionalJump, cc=!Symbol, brch=queue_remove_link
    idx=Increment
    d=NIL, mem=Write
queue_remove_link:
    read=Vr, vr=5
    d=Object, page=Fetch
    d=2, idx=Load
    d=NIL, mem=Write
    d=5, idx=Load
    d=NIL, mem=Write
    d=Register, ra=6, seq=Bus
scheduler_choose:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=2, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb
scheduler_choose_priority:
    ra=3, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_no_runnable
    read=Vr, vr=5
    d=Object, page=Fetch
    ra=3, s=Branch, brch=1, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=7
    read=Vr, vr=7
    d=Object, page=Fetch
    d=2, idx=Load
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
    ; R1=0 is a guest primitive. R1=1 is a boundary event: no caller stack
    ; operands belong to that signal, so it must never enter send_result.
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=event_delivered
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=scheduler_suspend_result
    read=Vr, vr=6
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result
scheduler_suspend_result:
    d=NIL, ldvr, vr=5, seq=Jump, brch=send_result
scheduler_no_runnable:
    ; Finish the wait/suspend send and materialize its continuation before idle.
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=1, ldroot
    seq=Jump, brch=scheduler_result
scheduler_failed:
    ra=1, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ; An asynchronous signal has no guest fallback activation.
    seq=Jump, brch=bad_state

; Enter only after save_context materialized the completed bytecode. The pending
; process has a root even after boundary cleared the ordinary scratch VRs.
check_process_switch:
    d=Sp, r=Bus, flags
    seq=ConditionalJump, cc=Zero, brch=check_low_space
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=7
    d=0, esp=Bus, sp=Bus
    d=0, estk=Bus
    d=31, r=Bus, rb=7, ldrb
    ra=7, d=0, ldroot
    d=0xa000000004, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=4
    read=Vr, vr=4
    d=Object, page=Fetch
    d=3, idx=Load
    read=Vr, vr=0
    d=Object, mem=Write
    read=Vr, vr=3
    d=Object, page=Fetch
    d=3, idx=Load
    read=Vr, vr=7
    d=Object, mem=Write
    read=Vr, vr=7
    d=Object, page=Fetch
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=0
    d=NIL, mem=Write
    d=0, r=Bus, rb=15, ldrb, seq=Jump, brch=load_context

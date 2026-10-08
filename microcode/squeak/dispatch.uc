; Squeak 1.1 primitive table, independent of the Xerox primitive numbers.
; R0 primitive, R1 arity, VR6 receiver, VR7 method, R15 failure continuation.
; Declared failures execute the guest method. Unimplemented required operations
; stop with status 8 at primitive_unimplemented, retaining number and operands.
dispatch_primitive:
    ; Optional character-scanner accelerator: the archived method contains the
    ; complete Smalltalk loop, which calls our BitBlt primitive for each glyph.
    ra=0, s=Branch, brch=103, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=65, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=66, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=67, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_stream
    ra=0, s=Branch, brch=105, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_replace
    ra=0, s=Branch, brch=145, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_fill
    ra=0, s=Branch, brch=154, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_transfer
    ra=0, s=Branch, brch=158, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_transfer
    ra=0, s=Branch, brch=155, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_seek
    ra=0, s=Branch, brch=121, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_path
    ra=0, s=Branch, brch=142, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_path
    ra=0, s=Branch, brch=161, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_directory_separator
    ra=0, s=Branch, brch=153, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_open
    ra=0, s=Branch, brch=150, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_control
    ra=0, s=Branch, brch=151, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_control
    ra=0, s=Branch, brch=152, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_control
    ra=0, s=Branch, brch=157, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_file_control
    ra=0, s=Branch, brch=112, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_free_bytes
    ra=0, s=Branch, brch=130, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_free_bytes
    ra=0, s=Branch, brch=131, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_free_bytes
    ra=0, s=Branch, brch=125, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_low_space_threshold
    ra=0, s=Branch, brch=96, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitblt
    ra=0, s=Branch, brch=93, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_semaphore_registration
    ra=0, s=Branch, brch=134, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_semaphore_registration
    ra=0, s=Branch, brch=124, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_semaphore_registration
    ra=0, s=Branch, brch=136, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_timer
    ra=0, s=Branch, brch=133, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_interrupt_key
    ra=0, s=Branch, brch=107, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_buttons
    ra=0, s=Branch, brch=108, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_key
    ra=0, s=Branch, brch=109, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_key
    ra=0, s=Branch, brch=90, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_point_device
    ra=0, s=Branch, brch=106, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_point_device
    ra=0, s=Branch, brch=135, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_clock
    ra=0, s=Branch, brch=137, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_clock
    ra=0, s=Branch, brch=122, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_receiver_result
    ra=0, s=Branch, brch=101, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitmap
    ra=0, s=Branch, brch=102, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_bitmap
    ra=0, s=Branch, brch=40, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=41, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=42, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=43, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=44, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=45, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=46, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=47, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=48, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=49, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=50, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=51, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=52, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=53, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=54, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_float
    ra=0, s=Branch, brch=79, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new_method
    ra=0, s=Branch, brch=19, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_integer
    ra=0, s=Branch, brch=60, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=61, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=62, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=63, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=64, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=68, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=69, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=73, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=74, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_indexed
    ra=0, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new
    ra=0, s=Branch, brch=71, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_new
    ra=0, s=Branch, brch=75, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_hash
    ra=0, s=Branch, brch=81, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value
    ra=0, s=Branch, brch=82, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_value_array
    ra=0, s=Branch, brch=83, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_perform
    ra=0, s=Branch, brch=84, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_perform
    ra=0, s=Branch, brch=85, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=86, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=87, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=88, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=89, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_scheduler
    ra=0, s=Branch, brch=110, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_identity
    ra=0, s=Branch, brch=111, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_class
    ra=0, s=Branch, brch=129, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_special_objects
    ra=0, s=Branch, brch=20, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=21, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=22, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=23, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=24, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=25, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=26, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=27, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=28, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=29, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=30, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=31, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=33, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=34, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=35, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=36, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=37, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=38, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=39, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=72, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=76, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=80, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=91, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=92, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=94, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=98, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=99, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=100, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=115, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=116, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=117, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=118, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=119, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ; Reserved/disabled entries are explicit primitiveFail in the archived
    ; initializePrimitiveTable. They must enter guest fallback, not stop the VM.
    ra=0, s=Branch, brch=0, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=19, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=55, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=4, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
    ra=0, s=Branch, brch=120, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=123, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=126, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
    ra=0, s=Branch, brch=148, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
    ra=0, s=Branch, brch=163, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=5, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
    ra=0, s=Branch, brch=171, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    ra=0, s=Branch, brch=179, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=70, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
    ra=0, s=Branch, brch=254, alu=Sub, cin=One, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_failed
primitive_unimplemented:
    d=8, r=Bus, rb=15, ldrb
    halt
primitive_special_objects:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0, r=Bus, rb=0, ldrb
    ra=0, d=Root, ldvr, vr=5, seq=Jump, brch=send_result

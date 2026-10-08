; External file service. The processor alone decodes paths, opaque handles and
; byte/word buffers. Device registers never receive an oop or guest address.
; The mounted external volume is volatile; guest writes do not alter host files.
; Handle ByteArrays contain little-endian id32 + a runtime-format cookie.
primitive_path:
    ra=0, s=Branch, brch=121, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=file_vm_path
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_image_name_store
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=1, r=Bus, rb=10, ldrb
    seq=Jump, brch=file_path_read
file_vm_path:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0, r=Bus, rb=10, ldrb
file_path_read:
    d=0x740, r=Bus, rb=7, ldrb
    ra=10, ldq
    ra=7, d=Q, io=Write
    d=0x744, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    d=SPECIAL_6, ldsym
    d=file_path_allocated, r=Bus, rb=6, ldrb, seq=Jump, brch=file_allocate_bytes
file_path_allocated:
    d=0x74c, r=Bus, rb=7, ldrb
    d=3, idx=Load
file_path_byte:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=send_result
    ra=7, io=Read
    d=Device, mem=Write
    idx=Increment
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=file_path_byte

file_image_name_store:
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Type
    d=SPECIAL_6, ldsym
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    d=file_image_name_valid, r=Bus, rb=6, ldrb, seq=Jump, brch=file_bytes
file_image_name_valid:
    ra=2, s=Branch, brch=4096, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x708, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=0x70c, r=Bus, rb=7, ldrb
    d=3, idx=Load
file_image_name_byte:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=file_image_name_commit
    mem=Read
    ra=7, d=Object, io=Write
    idx=Increment
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=file_image_name_byte
file_image_name_commit:
    d=0x700, r=Bus, rb=7, ldrb
    ra=7, d=8, io=Write
    d=device_receiver_result, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status

primitive_directory_separator:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=0x740, r=Bus, rb=7, ldrb
    ra=7, d=2, io=Write
    d=0x74c, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, s=Branch, brch=3, alu=Add, ldq
    d=SPECIAL_24, page=Fetch
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=5, seq=Jump, brch=send_result

primitive_file_open:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=0, r=Bus, rb=10, ldrb
    d=FALSE, seq=ConditionalJump, cc=Symbol, brch=file_open_flag
    d=1, r=Bus, rb=10, ldrb
    d=TRUE, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
file_open_flag:
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=3
    d=file_open_name, r=Bus, rb=6, ldrb, seq=Jump, brch=file_bytes
file_open_name:
    ra=2, s=Branch, brch=4096, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x708, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=0x70c, r=Bus, rb=7, ldrb
    d=3, idx=Load
file_open_path_byte:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=file_open_command
    mem=Read
    ra=7, d=Object, io=Write
    idx=Increment
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=file_open_path_byte
file_open_command:
    d=0x710, r=Bus, rb=7, ldrb
    ra=10, ldq
    ra=7, d=Q, io=Write
    d=0x700, r=Bus, rb=7, ldrb
    ra=7, d=1, io=Write
    d=file_opened, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status
file_opened:
    d=0x714, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=10, ldrb
    d=8, r=Bus, rb=2, ldrb
    d=SPECIAL_26, ldsym
    d=file_handle_allocated, r=Bus, rb=6, ldrb, seq=Jump, brch=file_allocate_bytes
file_handle_allocated:
    d=3, idx=Load
    d=4, r=Bus, rb=3, ldrb
file_handle_byte:
    ra=10, s=Branch, brch=255, alu=And, ldq
    d=Q, mem=Write
    idx=Increment
    ra=10, s=Branch, brch=24, alu=Rotate, rb=10, ldrb
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=file_handle_byte
    d=82, mem=Write
    idx=Increment
    d=75, mem=Write
    idx=Increment
    d=70, mem=Write
    idx=Increment
    d=49, mem=Write, seq=Jump, brch=send_result

; VR3 byte object -> selected object, R2 length; return through R6.
file_bytes:
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=3, alu=And, rb=4, ldrb
    ra=4, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    d=Register, ra=6, seq=Bus

; Allocate length R2 bytes of class SYMBOL. VR5 roots the result. R2/R6 and
; R8..R12 survive; R0/R3/R7/R13/Q scratch. Return through R6, object selected.
file_allocate_bytes:
    ra=2, s=Branch, brch=2, alu=Add, rb=3, ldrb
    d=Symbol, page=Allocate, size=ra, ra=3, scan=1
    d=Object, ldvr, vr=5
    ra=2, shift=Left, rb=3, ldrb
    ra=3, shift=Left, rb=3, ldrb
    ra=3, s=Branch, brch=2, alu=Or, ldq
    d=1, idx=Load
    d=Q, mem=Write
    ra=2, s=Branch, brch=0, alu=SubReverse, cin=One, rb=3, ldrb
    ra=3, s=Branch, brch=3, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=8, alu=Or, rb=3, ldrb
    ra=3, s=Branch, brch=12, alu=Rotate, ldq
    d=2, idx=Load
    d=Q, mem=Write
    d=file_bytes_hashed, r=Bus, rb=7, ldrb, seq=Jump, brch=allocation_hash
file_bytes_hashed:
    d=Register, ra=6, seq=Bus

file_status:
    d=0x704, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=Register, ra=6, seq=Bus

; Validate the runtime handle format and select its device id. Stale imported
; SQFile structs fail before any file command. Return through R6.
file_handle:
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=34, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=7, idx=Load
    mem=Read
    d=Object, r=Bus, s=Branch, brch=82, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=75, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=70, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    idx=Increment
    mem=Read
    d=Object, r=Bus, s=Branch, brch=49, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=6, idx=Load
    d=0, r=Bus, rb=4, ldrb
    d=4, r=Bus, rb=3, ldrb
file_handle_decode:
    mem=Read
    d=Object, r=Bus, rb=2, ldrb
    ra=2, s=Bus, d=0xffffff00, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=4, s=Branch, brch=8, alu=Rotate, rb=4, ldrb
    ra=2, rb=4, alu=Or, ldrb
    idx=Decrement
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=file_handle_decode
    d=0x714, r=Bus, rb=7, ldrb
    ra=4, ldq
    ra=7, d=Q, io=Write
    d=Register, ra=6, seq=Bus

primitive_file_control:
    ra=1, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    d=file_control_selected, r=Bus, rb=6, ldrb, seq=Jump, brch=file_handle
file_control_selected:
    d=6, r=Bus, rb=2, ldrb
    ra=0, s=Branch, brch=151, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=file_control_issue
    d=2, r=Bus, rb=2, ldrb
file_control_issue:
    d=0x700, r=Bus, rb=7, ldrb
    ra=2, ldq
    ra=7, d=Q, io=Write
    d=file_control_result, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status
file_control_result:
    ra=0, s=Branch, brch=151, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=device_receiver_result
    d=0x718, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    ra=0, s=Branch, brch=152, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_position_result
    d=0x728, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=157, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=positive_result
    ra=2, rb=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=primitive_true
    seq=Jump, brch=primitive_false
file_position_result:
    ra=2, rb=4, ldrb, seq=Jump, brch=positive_result

primitive_file_seek:
    ra=1, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym
    d=file_seek_position, r=Bus, rb=6, ldrb, seq=Jump, brch=positive_value
file_seek_position:
    ra=4, rb=10, ldrb
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=7, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldvr, vr=3
    d=file_seek_selected, r=Bus, rb=6, ldrb, seq=Jump, brch=file_handle
file_seek_selected:
    d=0x718, r=Bus, rb=7, ldrb
    ra=10, ldq
    ra=7, d=Q, io=Write
    d=0x700, r=Bus, rb=7, ldrb
    ra=7, d=5, io=Write
    d=device_receiver_result, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status

; Read/write arguments: opaque handle, indexable buffer, one-based index,
; count in elements. Word elements are four little-endian bytes on this port.
; Validate all bounds before a file command. Partial reads preserve untouched
; bytes in the final word and return the number of complete elements received.
primitive_file_transfer:
    ra=1, s=Branch, brch=4, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    read=Vr, vr=0
    d=Object, page=Fetch
    ra=9, s=Branch, brch=8, alu=Add, ldq
    d=Q, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=11, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=11, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    idx=Decrement
    mem=Read
    d=Object, ldsym, r=Bus, rb=12, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=primitive_failed
    ra=12, flags
    seq=ConditionalJump, cc=Sign, brch=primitive_failed
    seq=ConditionalJump, cc=Zero, brch=primitive_failed
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=4
    idx=Decrement
    mem=Read
    d=Object, ldvr, vr=3
    d=file_transfer_selected, r=Bus, rb=6, ldrb, seq=Jump, brch=file_handle
file_transfer_selected:
    read=Vr, vr=4
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=3, alu=And, rb=13, ldrb
    ra=13, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_transfer_word_length
    ra=13, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    ra=2, shift=Right, rb=2, ldrb
    ra=2, shift=Right, rb=2, ldrb
    seq=Jump, brch=file_transfer_bounds
file_transfer_word_length:
    ra=2, s=Branch, brch=28, alu=Rotate, rb=2, ldrb
    ra=2, s=Bus, d=0x0fffffff, alu=And, rb=2, ldrb
file_transfer_bounds:
    ra=12, s=Branch, brch=1, alu=Sub, cin=One, rb=3, ldrb
    ra=3, rb=11, alu=Add, ldq
    r=Q, s=Register, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=CorrectedSign, brch=file_transfer_extent_ok
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
file_transfer_extent_ok:
    ra=13, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=file_transfer_byte_count
    ra=11, shift=Left, rb=11, ldrb
    ra=11, shift=Left, rb=11, ldrb
file_transfer_byte_count:
    ra=11, s=Bus, d=1048576, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=!Carry, brch=primitive_failed
    d=0x720, r=Bus, rb=7, ldrb
    ra=11, ldq
    ra=7, d=Q, io=Write
    ra=0, s=Branch, brch=158, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_transfer_begin
    d=0x700, r=Bus, rb=7, ldrb
    ra=7, d=3, io=Write
    d=file_read_count, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status
file_read_count:
    d=0x720, r=Bus, rb=7, ldrb
    ra=7, io=Read
    d=Device, r=Bus, rb=11, ldrb
file_transfer_begin:
    ra=11, rb=5, ldrb
    ra=12, s=Branch, brch=2, alu=Add, ldq
    d=Q, idx=Load
    d=0, r=Bus, rb=14, ldrb
    d=0x724, r=Bus, rb=7, ldrb
file_transfer_byte:
    ra=5, flags
    seq=ConditionalJump, cc=Zero, brch=file_transfer_done
    mem=Read
    d=Object, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=158, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_write_byte
    ra=7, io=Read
    d=Device, r=Bus, rb=2, ldrb
    ra=13, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_read_byte_store
    ra=2, rb=14, alu=Rotate, ldq
    d=Q, r=Bus, rb=2, ldrb
    d=255, r=Bus, s=Register, rb=14, alu=Rotate, ldq
    r=Q, alu=Not, ldq
    r=Q, s=Register, rb=10, alu=And, ldq
    r=Q, s=Register, rb=2, alu=Or, ldq
    d=Q, mem=Write, seq=Jump, brch=file_transfer_next
file_read_byte_store:
    d=Register, ra=2, mem=Write, seq=Jump, brch=file_transfer_next
file_write_byte:
    ra=14, s=Branch, brch=32, alu=SubReverse, cin=One, rb=2, ldrb
    ra=10, rb=2, alu=Rotate, ldq
    r=Q, s=Branch, brch=255, alu=And, ldq
    ra=7, d=Q, io=Write
file_transfer_next:
    ra=5, s=Branch, brch=1, alu=Sub, cin=One, rb=5, ldrb
    ra=13, s=Branch, brch=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=file_transfer_next_field
    ra=14, s=Branch, brch=8, alu=Add, rb=14, ldrb
    ra=14, s=Branch, brch=32, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=file_transfer_byte
    d=0, r=Bus, rb=14, ldrb
file_transfer_next_field:
    idx=Increment, seq=Jump, brch=file_transfer_byte
file_transfer_done:
    ra=0, s=Branch, brch=158, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=file_transfer_result
    d=0x700, r=Bus, rb=7, ldrb
    ra=7, d=4, io=Write
    d=file_transfer_result, r=Bus, rb=6, ldrb, seq=Jump, brch=file_status
file_transfer_result:
    ra=11, rb=4, ldrb
    ra=13, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=positive_result
    ra=4, shift=Right, rb=4, ldrb
    ra=4, shift=Right, rb=4, ldrb, seq=Jump, brch=positive_result

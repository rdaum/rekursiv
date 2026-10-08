; Colour BitBlt. All guest decoding, clipping, pixel mapping, overlap handling
; and drawing execute on the processor through OBJEKT. No host raster helper.
; This path handles Boolean rules 0..15 and transparent paint/mask 25/26.
; Other supported-by-image raster rules remain explicit stops until implemented.
;
; Rooted ESTK frame (SP31): 1 dest Form, 2 source Form, 3 halftone;
; 4 rule, 5..14 coordinates exactly as the BitBlt fields; 15/16 strides,
; 17..20 extents, 21 colour map, 22/23 depths, 24 halftone height,
; 25 clone root, 26 scratch, 27/28 saved IP/SP, 29 failure, 30/31 scratch.
; During drawing, frame31 selects the mapped glyph path; that path caches
; map[0]/map[1] in frame26/30. The general path uses 26/30 for word advances.
; VR3 destination bits, VR4 source bits, VR5 halftone bits. VR6 receiver,
; VR7 fallback method, VR0..2 activation. All source aliases are cloned before
; the first write, so paging/collection cannot invalidate source pixels.
primitive_bitblt:
    ra=1, flags
    seq=ConditionalJump, cc=!Zero, brch=primitive_failed
    d=27, esp=Bus
    ra=8, estk=Alu
    d=28, esp=Bus
    ra=9, estk=Alu
    d=29, esp=Bus
    ra=15, estk=Alu
    d=31, sp=Bus
    d=bb_failed, r=Bus, rb=15, ldrb
    read=Vr, vr=6
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, s=Branch, brch=17, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_failed
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_failed
    d=3, idx=Load
    mem=Read
    d=1, esp=Bus
    d=Object, estk=Bus
    d=4, idx=Load
    mem=Read
    d=2, esp=Bus
    d=Object, estk=Bus
    d=5, idx=Load
    mem=Read
    d=3, esp=Bus
    d=Object, estk=Bus
    d=6, idx=Load
    mem=Read
    d=4, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=7, idx=Load
    mem=Read
    d=5, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=8, idx=Load
    mem=Read
    d=6, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=9, idx=Load
    mem=Read
    d=7, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=10, idx=Load
    mem=Read
    d=8, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=11, idx=Load
    mem=Read
    d=9, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=12, idx=Load
    mem=Read
    d=10, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=13, idx=Load
    mem=Read
    d=11, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=14, idx=Load
    mem=Read
    d=12, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=15, idx=Load
    mem=Read
    d=13, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=16, idx=Load
    mem=Read
    d=14, esp=Bus
    d=Object, ldsym, r=Bus, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    d=Object, estk=Bus
    d=17, idx=Load
    mem=Read
    d=21, esp=Bus
    d=Object, estk=Bus
    d=4, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=0, ldrb
    ra=0, s=Branch, brch=15, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Carry, brch=bb_rule_valid
    ra=0, s=Branch, brch=25, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_rule_valid
    ra=0, s=Branch, brch=26, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_rule_valid
    ra=0, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_failed
    ra=0, s=Branch, brch=17, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_failed
    ; Required extended combination rule, rather than a false success.
    d=96, r=Bus, rb=0, ldrb, seq=Jump, brch=primitive_unimplemented
bb_rule_valid:
    ra=0, flags
    seq=ConditionalJump, cc=Zero, brch=bb_ignore_source
    ra=0, s=Branch, brch=5, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_ignore_source
    ra=0, s=Branch, brch=10, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_ignore_source
    ra=0, s=Branch, brch=15, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_decode_destination
bb_ignore_source:
    d=2, esp=Bus
    d=NIL, estk=Bus
    d=3, esp=Bus
    d=NIL, estk=Bus
bb_decode_destination:
    d=1, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    d=bb_destination_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=bitmap_form
bb_destination_ready:
    d=25, esp=Bus
    read=Vr, vr=4
    d=Object, estk=Bus
    d=15, esp=Bus
    ra=10, estk=Alu
    d=17, esp=Bus
    ra=2, estk=Alu
    d=18, esp=Bus
    ra=3, estk=Alu
    d=22, esp=Bus
    ra=4, estk=Alu
    d=2, esp=Bus
    estk=Read
    d=Estk, ldsym, ldvr, vr=3
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_without_source
    d=bb_source_ready, r=Bus, rb=6, ldrb, seq=Jump, brch=bitmap_form
bb_source_ready:
    d=16, esp=Bus
    ra=10, estk=Alu
    d=19, esp=Bus
    ra=2, estk=Alu
    d=20, esp=Bus
    ra=3, estk=Alu
    d=23, esp=Bus
    ra=4, estk=Alu
    seq=Jump, brch=bb_colour_map
bb_without_source:
    d=NIL, ldvr, vr=4
    d=21, esp=Bus
    d=NIL, estk=Bus
    d=9, esp=Bus
    d=0, estk=Bus
    d=10, esp=Bus
    d=0, estk=Bus
    seq=Jump, brch=bb_halftone
bb_colour_map:
    d=21, esp=Bus
    estk=Read
    d=Estk, ldsym
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_no_map
    d=Symbol, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_failed
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=2, s=Branch, brch=8, alu=SubReverse, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_failed
    d=1, r=Bus, s=Register, rb=2, alu=Rotate, ldq
    r=Q, s=Branch, brch=2, alu=Add, rb=2, ldrb
    read=Size
    d=Object, r=Bus, s=Register, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_failed
    seq=Jump, brch=bb_halftone
bb_no_map:
    ; Indexed pixels without a map mask/zero-extend. RGB size conversion needs
    ; component mapping; reject it until that path has independent coverage.
    d=22, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    d=23, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=2, rb=3, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_halftone
    ra=2, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_failed
    ra=3, s=Branch, brch=16, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_failed
bb_halftone:
    d=3, esp=Bus
    estk=Read
    d=Estk, ldsym, ldvr, vr=5
    d=NIL, seq=ConditionalJump, cc=Symbol, brch=bb_forms_ready
    d=Symbol, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=2, ldrb, flags
    seq=ConditionalJump, cc=Zero, brch=bb_halftone_form
    ra=2, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=2, ldrb
    seq=Jump, brch=bb_halftone_height
bb_halftone_form:
    read=Size
    d=Object, r=Bus, s=Branch, brch=6, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_failed
    d=3, idx=Load
    mem=Read
    d=Object, ldvr, vr=5
    d=5, idx=Load
    mem=Read
    d=Object, ldsym, r=Bus, rb=2, ldrb, estk=Compact, compact=2
    d=Estk, seq=ConditionalJump, cc=!Symbol, brch=bb_failed
    read=Vr, vr=5
    d=Object, page=Fetch
    read=Representation
    d=Object, r=Bus, s=Branch, brch=3, alu=And, rb=3, ldrb
    ra=3, s=Branch, brch=1, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=bb_failed
    read=Size
    d=Object, r=Bus, s=Branch, brch=2, alu=Sub, cin=One, rb=3, ldrb
    ra=3, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_failed
bb_halftone_height:
    ra=2, flags
    seq=ConditionalJump, cc=Zero, brch=bb_failed
    seq=ConditionalJump, cc=Sign, brch=bb_failed
    d=24, esp=Bus
    ra=2, estk=Alu
bb_forms_ready:
    d=25, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=3
    d=0, r=Bus, rb=0, ldrb
    d=bb_clip_y, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_clip
bb_clip_y:
    d=1, r=Bus, rb=0, ldrb
    d=bb_clipped, r=Bus, rb=7, ldrb, seq=Jump, brch=bb_clip
bb_clipped:
    ; Clone aliases before mutation. Separate views with identical bits also
    ; alias even if the Form objects differ.
    read=Vr, vr=3
    d=Object, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=bb_clone_halftone
    d=bb_cloned_source, r=Bus, rb=6, ldrb, seq=Jump, brch=bb_clone
bb_cloned_source:
    d=25, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=4
bb_clone_halftone:
    read=Vr, vr=3
    d=Object, ldsym
    read=Vr, vr=5
    d=Object, seq=ConditionalJump, cc=!Symbol, brch=bb_begin_pixels
    d=bb_cloned_halftone, r=Bus, rb=6, ldrb, seq=Jump, brch=bb_clone
bb_cloned_halftone:
    d=25, esp=Bus
    estk=Read
    d=Estk, ldvr, vr=5
bb_begin_pixels:
    d=bad_state, r=Bus, rb=15, ldrb
    d=0, r=Bus, rb=8, ldrb
    d=0, r=Bus, rb=9, ldrb
    seq=Jump, brch=bb_choose_glyphs

; Clone destination bitmap VR3 into opaque raw storage, returned in frame25.
; Only raw words are copied. Caller continuation R6 and all bitmap roots survive.
bb_clone:
    read=Vr, vr=3
    d=Object, page=Fetch
    read=Size
    d=Object, r=Bus, rb=2, ldrb
    d=SPECIAL_4, page=Allocate, size=ra, ra=2, scan=0
    d=25, esp=Bus
    d=Object, estk=Bus
    d=1, r=Bus, rb=3, ldrb
bb_clone_word:
    read=Vr, vr=3
    d=Object, page=Fetch
    d=Register, ra=3, idx=Load
    mem=Read
    d=Object, ldsym
    d=25, esp=Bus
    estk=Read
    d=Estk, page=Fetch
    d=Register, ra=3, idx=Load
    d=Symbol, mem=Write
    ra=3, rb=2, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Zero, brch=bb_clone_done
    ra=3, s=Branch, brch=1, alu=Add, rb=3, ldrb
    seq=Jump, brch=bb_clone_word
bb_clone_done:
    d=Register, ra=6, seq=Bus
bb_clip:
    ra=0, s=Branch, brch=5, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=2, ldrb
    ra=0, s=Branch, brch=9, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=3, ldrb
    ra=0, s=Branch, brch=7, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=4, ldrb
    ra=0, s=Branch, brch=11, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=5, ldrb
    ra=0, s=Branch, brch=13, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=6, ldrb
    ra=0, s=Branch, brch=17, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=10, ldrb
    ra=0, s=Branch, brch=19, alu=Add, ldq
    d=Q, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=11, ldrb
    ra=4, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    ra=6, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    ra=6, rb=5, alu=Add, ldq
    r=Q, rb=6, ldrb
    d=0, r=Bus, rb=12, ldrb
    ra=4, rb=13, ldrb
    ra=2, s=Branch, brch=0, alu=SubReverse, cin=One, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_dest_low_done
    ra=14, rb=12, ldrb
bb_dest_low_done:
    ra=5, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_clip_low_done
    ra=14, rb=12, ldrb
bb_clip_low_done:
    ra=10, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_dest_high_done
    ra=14, rb=13, ldrb
bb_dest_high_done:
    ra=6, rb=2, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_clip_high_done
    ra=14, rb=13, ldrb
bb_clip_high_done:
    d=NIL, ldsym
    read=Vr, vr=4
    d=Object, seq=ConditionalJump, cc=Symbol, brch=bb_clip_finish
    ra=3, s=Branch, brch=0, alu=SubReverse, cin=One, rb=14, ldrb
    ra=14, rb=12, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=Sign, brch=bb_src_low_done
    ra=14, rb=12, ldrb
bb_src_low_done:
    ra=11, rb=3, alu=Sub, cin=One, ldq
    r=Q, rb=14, ldrb
    ra=14, rb=13, alu=Sub, cin=One, flags
    seq=ConditionalJump, cc=!Sign, brch=bb_src_high_done
    ra=14, rb=13, ldrb
bb_src_high_done:
bb_clip_finish:
    ra=13, rb=12, alu=Sub, cin=One, ldq, flags
    seq=ConditionalJump, cc=Zero, brch=bb_success
    seq=ConditionalJump, cc=Sign, brch=bb_success
    r=Q, rb=4, ldrb
    ra=2, rb=12, alu=Add, ldq
    r=Q, rb=2, ldrb
    ra=3, rb=12, alu=Add, ldq
    r=Q, rb=3, ldrb
    ra=0, s=Branch, brch=5, alu=Add, ldq
    d=Q, esp=Bus
    ra=2, estk=Alu
    ra=0, s=Branch, brch=9, alu=Add, ldq
    d=Q, esp=Bus
    ra=3, estk=Alu
    ra=0, s=Branch, brch=7, alu=Add, ldq
    d=Q, esp=Bus
    ra=4, estk=Alu
    d=Register, ra=7, seq=Bus
bb_invalid:
    d=Register, ra=15, seq=Bus
bb_success:
    ; Release the prepared bitmap root after the final pending write completes.
    idx=Clear, prepare
    read=Vr, vr=6
    d=Object, ldvr, vr=5
    d=27, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=28, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, r=Bus, rb=1, ldrb, estk=Bus
    seq=Jump, brch=send_result
bb_failed:
    d=27, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=8, ldrb
    d=28, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=9, ldrb
    d=29, esp=Bus
    estk=Read
    d=Estk, r=Bus, rb=15, ldrb
    d=0, esp=Bus, sp=Bus
    d=0, r=Bus, rb=1, ldrb, estk=Bus
    seq=Jump, brch=primitive_failed

; Generic peripheral exerciser. Every pixel and input read comes from LOGIK.
; Mouse movement moves the uploaded cursor. Key/button presses invert the
; stripe pattern. No Smalltalk image or host drawing algorithm is involved.
;
; Run: cargo run --release --locked -p rekursiv-emulator
; The emulator installs the generic workstation peripherals. This program
; loops forever; close its window, or use --headless --steps N for a bounded run.
; No object allocation, guest image, or collector is needed by this program.
;
; R0=device byte address, R1=upload countdown, R2=packet kind, R6=pixel pattern.
; SYMBOL bridges R6 to D while R0 selects the device address. Device is the
; read-reply latch; io=Read must finish before the next line consumes it.
; Cursor bank: 0x40c link; 0x418/41c dimensions; 0x420 stride; 0x424 control;
; 0x42c pixel data. Display bank uses corresponding 0x5xx addresses.
; Control 0 begins a replacement upload; 1 publishes the complete frame.
; Device contracts and packet kinds are defined in ../docs/devices.md.
.entry start
start:
    ; Link the visible cursor to the mouse, then upload a 16x16 cursor.
    ; One 32-bit device word per row; 0x80000000 draws its leftmost pixel.
    d=0x40c, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x418, r=Bus, rb=0, ldrb
    d=16, ra=0, io=Write
    d=0x41c, r=Bus, rb=0, ldrb
    d=16, ra=0, io=Write
    d=0x420, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x424, r=Bus, rb=0, ldrb
    d=0, ra=0, io=Write
    d=16, r=Bus, rb=1, ldrb
    d=0x42c, r=Bus, rb=0, ldrb
cursor_words:
    d=0x80000000, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=cursor_words
    d=0x424, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x518, r=Bus, rb=0, ldrb
    ; 320x240 display, ten 32-pixel words per row: 2400 words per frame.
    d=320, ra=0, io=Write
    d=0x51c, r=Bus, rb=0, ldrb
    d=240, ra=0, io=Write
    d=0x520, r=Bus, rb=0, ldrb
    d=10, ra=0, io=Write
    d=0xaaaa5555, r=Bus, rb=6, ldrb
frame:
    ; Stage a complete stripe frame. Only the final control write publishes it.
    d=0x524, r=Bus, rb=0, ldrb
    d=0, ra=0, io=Write
    d=2400, r=Bus, rb=1, ldrb
    d=0x52c, r=Bus, rb=0, ldrb
pixels:
    d=Register, ra=6, ldsym
    d=Symbol, ra=0, io=Write
    alu=Sub, ra=1, rb=1, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=pixels
    d=0x524, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
poll:
    ; Peek status and kind, consume one raw packet, then acknowledge its IRQ.
    ; Kind 2 (key/button down) inverts the pattern; motion and releases merely
    ; return to polling. Linked cursor movement is handled by the peripheral.
    d=0x310, r=Bus, rb=0, ldrb
    ra=0, io=Read
    d=Device, r=Bus, s=Bus, alu=And, flags
    seq=ConditionalJump, cc=Zero, brch=poll
    d=0x314, r=Bus, rb=0, ldrb
    ra=0, io=Read
    d=Device, r=Bus, rb=2, ldrb
    d=0x324, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    d=0x104, r=Bus, rb=0, ldrb
    d=1, ra=0, io=Write
    alu=Sub, ra=2, s=Bus, d=2, cin=One, flags
    seq=ConditionalJump, cc=!Zero, brch=poll
    alu=Not, ra=6, rb=6, ldrb
    seq=Jump, brch=frame

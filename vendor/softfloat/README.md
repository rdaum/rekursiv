# Berkeley SoftFloat 3e numeric model

Source: https://www.jhauser.us/arithmetic/SoftFloat-3e.zip. Archive SHA-256:
`21130ce885d35c1fe73fc1e1bf2244178167e05c6747cad5f450cc991714c746`.

This is an unmodified binary32/binary64 source subset used by the architectural test oracle and native microcode
emulator. RTL arithmetic executes in HardFloat; reference results never drive the Verilator machine.
The separately selected native emulator uses SoftFloat to emulate NUMERIK instructions. The BSD
license is in [COPYING.txt](COPYING.txt). The project supplies a portable platform header and C
entry point in `crates/rekursiv-model/reference/`.

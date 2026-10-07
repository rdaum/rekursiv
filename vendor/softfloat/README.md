# Berkeley SoftFloat 3e test oracle

Source: https://www.jhauser.us/arithmetic/SoftFloat-3e.zip.
Archive SHA-256: `21130ce885d35c1fe73fc1e1bf2244178167e05c6747cad5f450cc991714c746`.

This is an unmodified source subset used only by the architectural test oracle.
Guest arithmetic executes in RTL; reference results never drive the simulated machine.
The BSD license is in [COPYING.txt](COPYING.txt).
The project supplies a portable platform header and C entry point in `crates/rekursiv-model/reference/`.

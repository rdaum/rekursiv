# Berkeley HardFloat Release 1

Source: https://www.jhauser.us/arithmetic/HardFloat-1.zip (2019-07-29).
Archive SHA-256: `6b3757c9fbfa2230c6a2b84605e39372cb589dd7500e979c4f0b8ecc8a03b14b`.

These files are a subset of the release, using the RISC-V specialization.
One compatibility patch removes the duplicate `wire sqrtOpOut` declaration from `divSqrtRecFN_small.v`; modern SystemVerilog rejects the redundant ANSI-port declaration.
That specialization selects canonical NaNs and integer conversion results; it does not add a RISC-V instruction decoder.
The BSD license is in [COPYING.txt](COPYING.txt), and each source retains its copyright notice.

Project integration lives in `rtl/numerik_fp32.sv`. `rtl/hardfloat.sv` collects the source files and scopes vendor lint exclusions.

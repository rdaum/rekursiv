# Non-project Vivado build of the MEGA65 R6 rekursiv core. Run via build.sh.
set part xc7a200tfbg484-2
set here [file dirname [file normalize [info script]]]
set root [file normalize $here/../..]
set out  $here/build
file mkdir $out

read_verilog [list $root/vendor/picorv32/picorv32.v]
read_verilog -sv [concat [glob $here/rtl/*.sv] \
    [list $root/rtl/hardfloat.sv $root/rtl/logik.sv $root/rtl/logik_io.sv \
          $root/rtl/logik_sequencer.sv $root/rtl/logik_stacks.sv $root/rtl/logik_store.sv \
          $root/rtl/numerik.sv $root/rtl/numerik_alu.sv $root/rtl/numerik_fp32.sv \
          $root/rtl/numerik_fp64.sv $root/rtl/objekt.sv $root/rtl/objekt_directory.sv \
          $root/rtl/objekt_exchange.sv $root/rtl/objekt_gc.sv $root/rtl/objekt_transfer.sv]]
read_mem $out/boot.mem
read_mem $out/firmware.mem
read_xdc $here/mega65r6.xdc

synth_design -top mega65_top -part $part \
    -include_dirs [list $root $root/rtl $root/vendor/hardfloat/source $root/vendor/hardfloat/source/RISCV]
opt_design
place_design
route_design

report_utilization    -file $out/utilization.rpt
report_utilization -hierarchical -hierarchical_depth 3 -file $out/utilization_modules.rpt
report_timing_summary -file $out/timing.rpt
report_drc            -file $out/drc.rpt

if {[get_property SLACK [get_timing_paths -setup]] < 0 ||
    [get_property SLACK [get_timing_paths -hold]]  < 0} {
    error "timing not met, see $out/timing.rpt"
}
write_bitstream -force $out/rekursiv.bit

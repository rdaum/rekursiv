// Runs rekursiv_system from build/boot.mem and prints its debug display.
// Built and run by sim.sh.
module system_tb;
    logic clk = 0, rst = 1;
    always #5 clk = ~clk;
    logic [31:0] debug [8];
    logic booted, halted, service;
    logic [3:0] fault;
    logic [15:0] pc;
    rekursiv_system #(.BOOT_FILE("build/boot.mem")) machine (
        .clk_i(clk), .rst_i(rst), .debug_o(debug), .booted_o(booted),
        .halted_o(halted), .service_o(service), .fault_o(fault), .pc_o(pc));
    initial begin
        repeat (4) @(posedge clk);
        rst = 0;
        repeat (200000) @(posedge clk);
        $display("booted=%0d halted=%0d service=%0d fault=%0d pc=%0d", booted, halted, service, fault, pc);
        for (int i = 0; i < 8; i++) $display("debug[%0d] = %08x", i, debug[i]);
        $finish;
    end
endmodule

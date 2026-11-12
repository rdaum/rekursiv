// Runs rekursiv_system from build/sim/boot.mem and prints its debug display.
// Built and run by sim.sh.
module system_tb;
    logic clk = 0, rst = 1;
    integer cycles;
    always #5 clk = ~clk;
    logic [31:0] debug [8];
    logic booted, halted, service;
    logic [3:0] fault;
    logic [15:0] pc;
    logic sd_clk, sd_cs_n, sd_mosi, sd_miso;
    logic [31:0] disk [8];
    rekursiv_system #(.BOOT_FILE("build/sim/boot.mem"), .FIRMWARE("build/sim/firmware.mem")) machine (
        .clk_i(clk), .rst_i(rst),
        .sd_clk_o(sd_clk), .sd_cs_n_o(sd_cs_n), .sd_mosi_o(sd_mosi), .sd_miso_i(sd_miso),
        .disk_display_o(disk), .debug_o(debug), .booted_o(booted),
        .halted_o(halted), .service_o(service), .fault_o(fault), .pc_o(pc));
    sd_card_model #(.IMAGE("build/sim/card.img"), .SECTORS(81920)) card (
        .sd_clk_i(sd_clk), .sd_cs_n_i(sd_cs_n), .sd_mosi_i(sd_mosi), .sd_miso_o(sd_miso));
    initial begin
        repeat (4) @(posedge clk);
        rst = 0;
        if (!$value$plusargs("cycles=%d", cycles)) cycles = 2000000;
        repeat (cycles) @(posedge clk);
        $display("booted=%0d halted=%0d service=%0d fault=%0d pc=%0d", booted, halted, service, fault, pc);
        for (int i = 0; i < 8; i++) $display("debug[%0d] = %08x   disk[%0d] = %08x", i, debug[i], i, disk[i]);
        $finish;
    end
endmodule

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
    logic m_cke, m_cs_n, m_ras_n, m_cas_n, m_we_n, m_dq_oe, memory_tested, memory_failed;
    logic [1:0] m_ba;
    logic [12:0] m_a;
    logic [15:0] m_dq_out, m_dq_in;
    sdram_model ram (
        .clk_i(~clk), .cke_i(m_cke), .cs_n_i(m_cs_n), .ras_n_i(m_ras_n), .cas_n_i(m_cas_n),
        .we_n_i(m_we_n), .ba_i(m_ba), .a_i(m_a), .dq_i(m_dq_out), .dq_driven_i(m_dq_oe), .dq_o(m_dq_in));
    logic [31:0] disk [8];
    rekursiv_system #(.FIRMWARE("build/sim/loader.mem")) machine (
        .clk_i(clk), .rst_i(rst),
        .sd_clk_o(sd_clk), .sd_cs_n_o(sd_cs_n), .sd_mosi_o(sd_mosi), .sd_miso_i(sd_miso),
        .disk_display_o(disk), .debug_o(debug), .booted_o(booted),
        .sdram_cke_o(m_cke), .sdram_cs_n_o(m_cs_n), .sdram_ras_n_o(m_ras_n), .sdram_cas_n_o(m_cas_n),
        .sdram_we_n_o(m_we_n), .sdram_ba_o(m_ba), .sdram_a_o(m_a), .sdram_dqml_o(), .sdram_dqmh_o(),
        .sdram_dq_o(m_dq_out), .sdram_dq_oe_o(m_dq_oe), .sdram_dq_i(m_dq_in),
        .memory_tested_o(memory_tested), .memory_failed_o(memory_failed), .memory_failed_address_o(),
        .halted_o(halted), .service_o(service), .fault_o(fault), .pc_o(pc));
    sd_card_model #(.IMAGE("build/sim/card.img"), .SECTORS(81920)) card (
        .sd_clk_i(sd_clk), .sd_cs_n_i(sd_cs_n), .sd_mosi_i(sd_mosi), .sd_miso_o(sd_miso));
    initial begin
        repeat (4) @(posedge clk);
        rst = 0;
        if (!$value$plusargs("cycles=%d", cycles)) cycles = 2000000;
        repeat (cycles) @(posedge clk);
        $display("booted=%0d halted=%0d service=%0d fault=%0d pc=%0d memory tested=%0d failed=%0d sdram violations=%0d",
                 booted, halted, service, fault, pc, memory_tested, memory_failed, ram.errors);
        $display("disk processor trap=%0d pc=%h", machine.disk.cpu.trap, machine.disk.cpu.reg_pc);
        for (int i = 0; i < 8; i++) $display("debug[%0d] = %08x   disk[%0d] = %08x", i, debug[i], i, disk[i]);
        $finish;
    end
endmodule

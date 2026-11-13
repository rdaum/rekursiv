// The cached object memory against the chip model: random reads and writes
// over addresses chosen to collide in the cache, checked against a
// reference, with the completion sometimes held. Run by sim/object_memory.sh.
module object_memory_tb;
    logic clk = 0, rst = 1;
    always #5 clk = ~clk;

    logic valid = 0, ready, write = 0, rsp_valid, rsp_ready = 1, rsp_error;
    logic [23:0] addr = 0;
    logic [39:0] wdata = 0, rdata;
    logic test_done, test_failed;
    logic cke, cs_n, ras_n, cas_n, we_n, dq_oe;
    logic [1:0] ba;
    logic [12:0] a;
    logic [15:0] dq_out, dq_in;

    // A 64-word cache makes collisions and evictions frequent.
    object_memory #(.CACHE_BITS(6), .QUEUE_BITS(2)) dut (
        .clk_i(clk), .rst_i(rst),
        .mem_valid_i(valid), .mem_ready_o(ready), .mem_write_i(write), .mem_addr_i(addr),
        .mem_data_i(wdata), .mem_rsp_valid_o(rsp_valid), .mem_rsp_ready_i(rsp_ready),
        .mem_rsp_data_o(rdata), .mem_rsp_error_o(rsp_error),
        .test_done_o(test_done), .test_failed_o(test_failed), .test_address_o(),
        .sdram_cke_o(cke), .sdram_cs_n_o(cs_n), .sdram_ras_n_o(ras_n), .sdram_cas_n_o(cas_n),
        .sdram_we_n_o(we_n), .sdram_ba_o(ba), .sdram_a_o(a), .sdram_dqml_o(), .sdram_dqmh_o(),
        .sdram_dq_o(dq_out), .sdram_dq_oe_o(dq_oe), .sdram_dq_i(dq_in));

    sdram_model chip (
        .clk_i(~clk), .cke_i(cke), .cs_n_i(cs_n), .ras_n_i(ras_n), .cas_n_i(cas_n),
        .we_n_i(we_n), .ba_i(ba), .a_i(a), .dq_i(dq_out), .dq_driven_i(dq_oe), .dq_o(dq_in));

    logic [39:0] reference [logic [23:0]];
    int errors = 0, reads = 0, writes = 0, fast_reads = 0;
    longint cycle = 0;
    always @(posedge clk) cycle <= cycle + 1;
    initial begin
        #40_000_000_000;
        $display("timeout: state %0d queue %0d chip_busy %0d test_done %0d", dut.state, dut.queue_count, dut.chip_busy, test_done);
        $finish;
    end

    // Signals change at falling edges and are read there, so a handshake
    // seen high at a falling edge completes at the rising edge that follows.
    task automatic access(input logic w, input logic [23:0] at, input logic [39:0] data);
        longint start;
        @(negedge clk);
        valid = 1; write = w; addr = at; wdata = data;
        while (!ready) @(negedge clk);
        start = cycle;
        @(negedge clk);
        valid = 0;
        // Sometimes hold the completion for a few cycles.
        rsp_ready = 0;
        repeat ($urandom_range(0, 3) == 0 ? $urandom_range(1, 4) : 0) @(negedge clk);
        rsp_ready = 1;
        while (!rsp_valid) @(negedge clk);
        if (w) begin reference[at] = data; writes++; end
        else begin
            reads++;
            if (cycle - start <= 3) fast_reads++;
            if (reference.exists(at) && rdata !== reference[at]) begin
                errors++;
                if (errors < 10) $display("read %h = %h, expected %h", at, rdata, reference[at]);
            end
        end
        if (rsp_error) begin errors++; $display("unexpected error at %h", at); end
        @(negedge clk);
    endtask

    initial begin
        repeat (5) @(posedge clk);
        rst = 0;
        while (!test_done) @(posedge clk);
        if (test_failed) begin $display("power-on check failed"); $finish; end
        // Three regions that share cache indexes, 96 words each.
        for (int i = 0; i < 40000; i++) begin
            logic [23:0] at;
            at = 24'($urandom_range(0, 2)) * 24'h010040 + 24'($urandom_range(0, 95));
            access($urandom_range(0, 2) == 0, at, {$urandom, $urandom} & 40'hff_ffff_ffff);
        end
        // Every written word must have reached the chip: read each back
        // after the cache has been filled with other addresses.
        for (int i = 0; i < 64; i++) access(0, 24'h200000 + 24'(i), 0);
        foreach (reference[at]) access(0, at, 0);
        $display("%0d writes, %0d reads (%0d answered from the cache), %0d errors, violations %0d",
                 writes, reads, fast_reads, errors, chip.errors);
        $finish;
    end
endmodule

// SDRAM controller against the chip model: address-line and data patterns,
// then random writes and reads checked against a reference. Run by sim/sdram.sh.
module sdram_tb;
    logic clk = 0, rst = 1;
    always #5 clk = ~clk;

    logic valid, ready, write, rsp_valid;
    logic [22:0] addr;
    logic [63:0] wdata, rdata;
    logic cke, cs_n, ras_n, cas_n, we_n, dq_oe;
    logic [1:0] ba;
    logic [12:0] a;
    logic [15:0] dq_out, dq_in;

    sdram #(.CLK_HZ(12_600_000)) dut (
        .clk_i(clk), .rst_i(rst),
        .valid_i(valid), .ready_o(ready), .write_i(write), .addr_i(addr), .wdata_i(wdata),
        .rsp_valid_o(rsp_valid), .rdata_o(rdata),
        .sdram_cke_o(cke), .sdram_cs_n_o(cs_n), .sdram_ras_n_o(ras_n), .sdram_cas_n_o(cas_n),
        .sdram_we_n_o(we_n), .sdram_ba_o(ba), .sdram_a_o(a), .sdram_dqml_o(), .sdram_dqmh_o(),
        .sdram_dq_o(dq_out), .sdram_dq_oe_o(dq_oe), .sdram_dq_i(dq_in));

    sdram_model chip (
        .clk_i(~clk), .cke_i(cke), .cs_n_i(cs_n), .ras_n_i(ras_n), .cas_n_i(cas_n),
        .we_n_i(we_n), .ba_i(ba), .a_i(a), .dq_i(dq_out), .dq_driven_i(dq_oe), .dq_o(dq_in));

    logic [63:0] reference [logic [22:0]];
    int errors = 0;

    task automatic access(input logic w, input logic [22:0] at, input logic [63:0] data);
        @(posedge clk);
        while (!ready) @(posedge clk);
        valid <= 1; write <= w; addr <= at; wdata <= data;
        @(posedge clk);
        valid <= 0;
        while (!rsp_valid) @(posedge clk);
        if (w) reference[at] = data;
        else if (rdata !== (reference.exists(at) ? reference[at] : 64'd0)) begin
            errors++;
            if (errors < 10) $display("read %h = %h, expected %h", at, rdata, reference[at]);
        end
    endtask

    initial begin
        valid = 0; write = 0; addr = 0; wdata = 0;
        repeat (4) @(posedge clk);
        rst = 0;
        // Every address line, both data polarities.
        for (int pass = 0; pass < 2; pass++) begin
            for (int i = 0; i <= 23; i++)
                access(1, i == 0 ? 23'd0 : 23'd1 << (i - 1),
                       {64{pass[0]}} ^ (64'hdead_beef_0123_4567 + 64'(i) * 64'h0101_0101_0101_0101));
            for (int i = 0; i <= 23; i++) access(0, i == 0 ? 23'd0 : 23'd1 << (i - 1), 0);
        end
        // Random traffic over a small set of addresses spread across banks.
        for (int i = 0; i < 4000; i++) begin
            logic [22:0] at;
            at = {2'($urandom), 13'($urandom_range(0, 3)), 8'($urandom_range(0, 255))};
            access($urandom_range(0, 1) == 1, at, {$urandom, $urandom});
        end
        if (errors == 0 && chip.errors == 0) $display("PASS (%0d refreshes)", chip.refreshes);
        else $display("FAIL: %0d data errors, %0d protocol violations", errors, chip.errors);
        $finish;
    end
endmodule

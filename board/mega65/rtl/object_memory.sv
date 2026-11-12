// Object memory in SDRAM, behind OBJEKT's memory request/completion channel.
//
// After reset it first checks the chip: a distinct pattern and then its
// complement at address 0 and at each single-address-bit address, which
// finds stuck or shorted address and data lines in a few hundred cycles.
// test_done_o rises when the check ends; test_failed_o and test_address_o
// report the first mismatch. OBJEKT's channel is served only after a pass.
module object_memory #(
    parameter integer MEMORY_WORDS = 8388608,
    parameter integer CLK_HZ = 12_600_000
)(
    input  logic        clk_i,
    input  logic        rst_i,

    input  logic        mem_valid_i,
    output logic        mem_ready_o,
    input  logic        mem_write_i,
    input  logic [23:0] mem_addr_i,
    input  logic [39:0] mem_data_i,
    output logic        mem_rsp_valid_o,
    input  logic        mem_rsp_ready_i,
    output logic [39:0] mem_rsp_data_o,
    output logic        mem_rsp_error_o,

    output logic        test_done_o,
    output logic        test_failed_o,
    output logic [22:0] test_address_o,

    output logic        sdram_cke_o,
    output logic        sdram_cs_n_o,
    output logic        sdram_ras_n_o,
    output logic        sdram_cas_n_o,
    output logic        sdram_we_n_o,
    output logic [1:0]  sdram_ba_o,
    output logic [12:0] sdram_a_o,
    output logic        sdram_dqml_o,
    output logic        sdram_dqmh_o,
    output logic [15:0] sdram_dq_o,
    output logic        sdram_dq_oe_o,
    input  logic [15:0] sdram_dq_i
);
    logic        valid, ready, write, rsp_valid;
    logic [22:0] addr;
    logic [63:0] wdata, rdata;

    sdram #(.CLK_HZ(CLK_HZ)) chip (
        .clk_i, .rst_i,
        .valid_i(valid), .ready_o(ready), .write_i(write), .addr_i(addr), .wdata_i(wdata),
        .rsp_valid_o(rsp_valid), .rdata_o(rdata),
        .sdram_cke_o, .sdram_cs_n_o, .sdram_ras_n_o, .sdram_cas_n_o, .sdram_we_n_o,
        .sdram_ba_o, .sdram_a_o, .sdram_dqml_o, .sdram_dqmh_o,
        .sdram_dq_o, .sdram_dq_oe_o, .sdram_dq_i);

    // ---------------------------------------------------------------------
    // Power-on check: phases 0 write, 1 verify, 2 write complement, 3 verify.
    // ---------------------------------------------------------------------
    logic [1:0]  phase;
    logic [4:0]  index;
    logic        test_busy;
    wire  [22:0] test_addr = index == 0 ? 23'd0 : 23'd1 << (index - 1'b1);
    wire  [63:0] pattern = {64{phase[1]}} ^
        (64'hdead_beef_0123_4567 + {59'd0, index} * 64'h0101_0101_0101_0101);

    // ---------------------------------------------------------------------
    // OBJEKT's channel: one request, held until its completion is accepted.
    // ---------------------------------------------------------------------
    logic busy, completed;
    wire  in_range = {8'd0, mem_addr_i} < 32'(MEMORY_WORDS) && !mem_addr_i[23];
    wire  mem_accept = mem_valid_i && mem_ready_o;

    assign mem_ready_o = test_done_o && !test_failed_o && !busy && (ready || !in_range);
    assign mem_rsp_valid_o = completed;

    always_comb begin
        if (!test_done_o) begin
            valid = !test_busy;
            write = !phase[0];
            addr  = test_addr;
            wdata = pattern;
        end else begin
            valid = mem_accept && in_range;
            write = mem_write_i;
            addr  = mem_addr_i[22:0];
            wdata = {24'd0, mem_data_i};
        end
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            phase <= '0;
            index <= '0;
            test_busy <= 1'b0;
            test_done_o <= 1'b0;
            test_failed_o <= 1'b0;
            test_address_o <= '0;
            busy <= 1'b0;
            completed <= 1'b0;
        end else if (!test_done_o) begin
            if (valid && ready) test_busy <= 1'b1;
            if (rsp_valid) begin
                test_busy <= 1'b0;
                if (phase[0] && rdata != pattern) begin
                    test_done_o <= 1'b1;
                    test_failed_o <= 1'b1;
                    test_address_o <= test_addr;
                end else if (index == 5'd23) begin
                    index <= '0;
                    phase <= phase + 1'b1;
                    if (phase == 2'd3) test_done_o <= 1'b1;
                end else index <= index + 1'b1;
            end
        end else begin
            if (mem_accept) begin
                busy <= 1'b1;
                mem_rsp_error_o <= !in_range;
                if (!in_range) completed <= 1'b1;
            end
            if (busy && rsp_valid) begin
                completed <= 1'b1;
                mem_rsp_data_o <= rdata[39:0];
            end
            if (completed && mem_rsp_ready_i) begin
                completed <= 1'b0;
                busy <= 1'b0;
            end
        end
    end
endmodule

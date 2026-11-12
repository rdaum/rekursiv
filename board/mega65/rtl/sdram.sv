// SDRAM controller for the MEGA65 R6's IS42S16320F (32M x 16: 13 row bits,
// 10 column bits, 4 banks). It presents 2^23 words of 64 bits; each word is
// four consecutive columns, read or written as one burst of four, so a word
// never crosses a row. Machine words use the low 40 bits.
//
// Everything runs on the machine clock, which must be slow enough that every
// SDRAM interval except the power-up wait fits in one cycle (below about
// 16 MHz for the -6 part: tRC is 60 ns). The board sends the chip the
// inverted clock, so the chip samples commands mid-cycle and its read data
// (CAS latency 2) is stable at the controller's edges.
//
// One request at a time: valid_i && ready_o accepts it, and rsp_valid_o
// pulses when it has completed, with rdata_o for a read. Refresh runs
// between requests.
module sdram #(
    parameter integer CLK_HZ = 12_600_000
)(
    input  logic        clk_i,
    input  logic        rst_i,

    input  logic        valid_i,
    output logic        ready_o,
    input  logic        write_i,
    input  logic [22:0] addr_i,
    input  logic [63:0] wdata_i,
    output logic        rsp_valid_o,
    output logic [63:0] rdata_o,

    // Chip pins. dq is split for the board's tristate buffers.
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
    // {ras_n, cas_n, we_n}
    localparam logic [2:0] NOP = 3'b111, ACTIVE = 3'b011, READ = 3'b101, WRITE = 3'b100,
        PRECHARGE = 3'b010, REFRESH = 3'b001, LOAD_MODE = 3'b000;
    // Burst length 4, sequential, CAS latency 2, burst writes.
    localparam logic [12:0] MODE = 13'b000_0_00_010_0_010;

    // 200 us after power-up; one refresh per 7.8 us (8,192 rows in 64 ms).
    localparam integer POWER_UP_CYCLES = CLK_HZ / 5000 + 1;
    localparam integer REFRESH_CYCLES = CLK_HZ / 1_000_000 * 7;

    typedef enum logic [3:0] {
        POWER_UP, INIT_PRECHARGE, INIT_REFRESH, INIT_MODE, IDLE,
        REFRESH_WAIT, ACTIVATE, READ_WAIT, WRITE_DATA, RECOVER
    } state_t;
    state_t state;

    logic [2:0]  command;
    logic [15:0] timer;
    logic [3:0]  step;
    logic [9:0]  refresh_timer;
    logic        refresh_due;
    logic        request_write;
    logic [22:0] request_addr;
    logic [63:0] request_data;
    logic [15:0] dq_in;

    assign {sdram_ras_n_o, sdram_cas_n_o, sdram_we_n_o} = command;
    assign sdram_cs_n_o = 1'b0;
    assign sdram_cke_o  = 1'b1;
    assign sdram_dqml_o = 1'b0;
    assign sdram_dqmh_o = 1'b0;
    assign ready_o = state == IDLE && !refresh_due;

    always_ff @(posedge clk_i) dq_in <= sdram_dq_i;

    always_ff @(posedge clk_i) begin
        command       <= NOP;
        rsp_valid_o   <= 1'b0;
        sdram_dq_oe_o <= 1'b0;
        if (rst_i) begin
            state <= POWER_UP;
            timer <= 16'(POWER_UP_CYCLES);
            step <= '0;
            refresh_timer <= '0;
            refresh_due <= 1'b0;
        end else begin
            if (refresh_timer == 10'(REFRESH_CYCLES)) begin
                refresh_timer <= '0;
                refresh_due <= 1'b1;
            end else refresh_timer <= refresh_timer + 1'b1;

            unique case (state)
                POWER_UP: begin
                    timer <= timer - 1'b1;
                    if (timer == 0) state <= INIT_PRECHARGE;
                end
                INIT_PRECHARGE: begin
                    command <= PRECHARGE;
                    sdram_a_o <= 13'b0_0100_0000_0000;   // A10: all banks
                    step <= 4'd8;
                    timer <= 16'd1;
                    state <= INIT_REFRESH;
                end
                // Eight refreshes, each followed by an idle cycle.
                INIT_REFRESH: begin
                    if (timer != 0) timer <= timer - 1'b1;
                    else if (step != 0) begin
                        command <= REFRESH;
                        step <= step - 1'b1;
                        timer <= 16'd1;
                    end else state <= INIT_MODE;
                end
                INIT_MODE: begin
                    command <= LOAD_MODE;
                    sdram_ba_o <= 2'b00;
                    sdram_a_o <= MODE;
                    timer <= 16'd1;
                    state <= REFRESH_WAIT;
                end
                IDLE: begin
                    if (refresh_due) begin
                        command <= REFRESH;
                        refresh_due <= 1'b0;
                        timer <= 16'd1;
                        state <= REFRESH_WAIT;
                    end else if (valid_i) begin
                        request_write <= write_i;
                        request_addr <= addr_i;
                        request_data <= wdata_i;
                        command <= ACTIVE;
                        sdram_ba_o <= addr_i[22:21];
                        sdram_a_o <= addr_i[20:8];
                        state <= ACTIVATE;
                    end
                end
                REFRESH_WAIT: begin
                    if (timer != 0) timer <= timer - 1'b1;
                    else state <= IDLE;
                end
                // The row is open; issue the burst with auto precharge (A10).
                ACTIVATE: begin
                    sdram_a_o <= {2'b00, 1'b1, request_addr[7:0], 2'b00};
                    step <= '0;
                    if (request_write) begin
                        command <= WRITE;
                        sdram_dq_o <= request_data[15:0];
                        sdram_dq_oe_o <= 1'b1;
                        state <= WRITE_DATA;
                    end else begin
                        command <= READ;
                        state <= READ_WAIT;
                    end
                end
                // READ is registered at edge E and the chip samples it at
                // E + 0.5. With CAS latency 2, beat n is stable at edge
                // E + 2 + n, where dq_in captures it. This state sees it one
                // edge later, when step is 2 + n.
                READ_WAIT: begin
                    step <= step + 1'b1;
                    unique case (step)
                        4'd2: rdata_o[15:0]  <= dq_in;
                        4'd3: rdata_o[31:16] <= dq_in;
                        4'd4: rdata_o[47:32] <= dq_in;
                        4'd5: begin
                            rdata_o[63:48] <= dq_in;
                            rsp_valid_o <= 1'b1;
                            state <= IDLE;
                        end
                        default: begin end
                    endcase
                end
                WRITE_DATA: begin
                    step <= step + 1'b1;
                    unique case (step)
                        4'd0: begin sdram_dq_o <= request_data[31:16]; sdram_dq_oe_o <= 1'b1; end
                        4'd1: begin sdram_dq_o <= request_data[47:32]; sdram_dq_oe_o <= 1'b1; end
                        4'd2: begin sdram_dq_o <= request_data[63:48]; sdram_dq_oe_o <= 1'b1; end
                        default: begin
                            timer <= 16'd2;
                            state <= RECOVER;
                        end
                    endcase
                end
                // Write recovery and the automatic precharge.
                RECOVER: begin
                    if (timer != 0) timer <= timer - 1'b1;
                    else begin
                        rsp_valid_o <= 1'b1;
                        state <= IDLE;
                    end
                end
                default: state <= POWER_UP;
            endcase
        end
    end
endmodule

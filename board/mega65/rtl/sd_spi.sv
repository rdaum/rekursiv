// SD card controller in SPI mode: card setup, then single 512-byte sector
// reads and writes. Supports SDHC/SDXC (block addressing) and standard
// capacity cards (byte addressing, 512-byte blocks).
//
// Setup runs at CLK_HZ / (2 * SLOW_DIVIDE), at most 400 kHz; transfers run
// at CLK_HZ / (2 * FAST_DIVIDE). A request is accepted with valid && ready
// once ready_o has risen after setup. Read data streams out one byte per
// read_byte_o pulse with its index. A write pulls each byte by presenting
// write_index_o and sampling write_data_i on the following cycle. done_o
// pulses once per request with error_o. A failed setup holds failed_o.
module sd_spi #(
    parameter integer SLOW_DIVIDE = 16,
    parameter integer FAST_DIVIDE = 1
)(
    input  logic        clk_i,
    input  logic        rst_i,

    // Card pins
    output logic        sd_clk_o,
    output logic        sd_cs_n_o,
    output logic        sd_mosi_o,
    input  logic        sd_miso_i,

    // Requests
    input  logic        valid_i,
    output logic        ready_o,
    input  logic        write_i,
    input  logic [31:0] sector_i,
    output logic        done_o,
    output logic        error_o,
    output logic        failed_o,
    output logic        sdhc_o,
    // Diagnostics: {state, last R1, setup attempts}
    output logic [20:0] debug_o,

    output logic        read_byte_o,
    output logic [8:0]  read_index_o,
    output logic [7:0]  read_data_o,

    output logic [8:0]  write_index_o,
    input  logic [7:0]  write_data_i
);
    // ---------------------------------------------------------------------
    // Byte engine, SPI mode 0: MOSI changes while SCK is low, MISO is
    // sampled on the rising edge.
    // ---------------------------------------------------------------------
    logic        fast;
    logic        xfer_start, xfer_busy;
    logic [7:0]  xfer_out, xfer_in, shift_out;
    logic [3:0]  bit_count;
    logic [7:0]  divide_count;
    wire  [7:0]  divide = fast ? 8'(FAST_DIVIDE - 1) : 8'(SLOW_DIVIDE - 1);

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            xfer_busy <= 1'b0;
            sd_clk_o  <= 1'b0;
            sd_mosi_o <= 1'b1;
            xfer_in   <= 8'hff;
            bit_count <= '0;
            divide_count <= '0;
        end else if (xfer_start) begin
            xfer_busy    <= 1'b1;
            shift_out    <= xfer_out;
            sd_mosi_o    <= xfer_out[7];
            bit_count    <= 4'd8;
            divide_count <= divide;
        end else if (xfer_busy) begin
            if (divide_count != 0) divide_count <= divide_count - 1'b1;
            else begin
                divide_count <= divide;
                if (!sd_clk_o) begin
                    sd_clk_o <= 1'b1;
                    xfer_in  <= {xfer_in[6:0], sd_miso_i};
                end else begin
                    sd_clk_o  <= 1'b0;
                    shift_out <= {shift_out[6:0], 1'b1};
                    sd_mosi_o <= shift_out[6];
                    bit_count <= bit_count - 1'b1;
                    if (bit_count == 4'd1) begin
                        xfer_busy <= 1'b0;
                        sd_mosi_o <= 1'b1;
                    end
                end
            end
        end
    end

    // ---------------------------------------------------------------------
    // Command sequencer
    // ---------------------------------------------------------------------
    typedef enum logic [4:0] {
        POWER_UP, IDLE_CMD, IF_COND, IF_COND_TAIL, APP_CMD, OP_COND, READ_OCR, OCR_TAIL,
        SET_BLOCK, READY, READ_CMD, READ_TOKEN, READ_DATA, READ_CRC,
        WRITE_CMD, WRITE_GAP, WRITE_TOKEN, WRITE_DATA, WRITE_CRC, WRITE_RESPONSE, WRITE_BUSY,
        FINISH, RETRY, FAILED
    } state_t;

    // Each command first clocks 0xff until the card releases MISO (S_GAP),
    // then sends its six bytes (S_SEND) and polls for R1 (S_R1).
    typedef enum logic [2:0] { S_IDLE, S_GAP, S_GAP_WAIT, S_SEND, S_R1, S_DONE } command_phase_t;

    state_t         state;
    command_phase_t phase;
    logic [47:0]    command;
    logic [2:0]     command_bytes;
    logic [7:0]     r1;
    logic [7:0]     waits;
    logic [19:0]    retries;
    logic [7:0]     attempts;
    logic [9:0]     count;
    logic           sdhc, request_write, request_error;
    logic [31:0]    request_sector;
    logic           byte_pending;

    assign ready_o       = state == READY;
    assign failed_o      = state == FAILED;
    assign sdhc_o        = sdhc;
    assign debug_o       = {state, r1, attempts};
    assign write_index_o = count[8:0];

    wire [31:0] block_address = sdhc ? request_sector : {request_sector[22:0], 9'b0};

    // A finished byte transfer: xfer_busy fell on this cycle.
    logic xfer_busy_q;
    always_ff @(posedge clk_i) xfer_busy_q <= xfer_busy;
    wire xfer_done = xfer_busy_q && !xfer_busy;

    function automatic logic [47:0] frame(input logic [5:0] index, input logic [31:0] argument,
                                          input logic [7:0] crc);
        frame = {2'b01, index, argument, crc};
    endfunction

    always_ff @(posedge clk_i) begin
        xfer_start  <= 1'b0;
        done_o      <= 1'b0;
        read_byte_o <= 1'b0;
        if (rst_i) begin
            state <= POWER_UP;
            phase <= S_IDLE;
            fast <= 1'b0;
            sd_cs_n_o <= 1'b1;
            count <= '0;
            retries <= '0;
            sdhc <= 1'b0;
            attempts <= '0;
            byte_pending <= 1'b0;
            error_o <= 1'b0;
        end else if (byte_pending) begin
            if (xfer_done) byte_pending <= 1'b0;
        end else if (phase == S_GAP) begin
            phase <= S_GAP_WAIT;
            waits <= 8'd64;
            xfer_out <= 8'hff;
            xfer_start <= 1'b1;
            byte_pending <= 1'b1;
        end else if (phase == S_GAP_WAIT && xfer_in != 8'hff && waits != 0) begin
            waits <= waits - 1'b1;
            xfer_out <= 8'hff;
            xfer_start <= 1'b1;
            byte_pending <= 1'b1;
        end else if (phase == S_GAP_WAIT) begin
            phase <= S_SEND;
        end else if (phase == S_SEND) begin
            // Shift out the six command bytes, then poll for R1.
            if (command_bytes != 0) begin
                xfer_out <= command[47:40];
                command <= {command[39:0], 8'hff};
                command_bytes <= command_bytes - 1'b1;
                xfer_start <= 1'b1;
                byte_pending <= 1'b1;
            end else begin
                phase <= S_R1;
                waits <= 8'd64;
                xfer_out <= 8'hff;
                xfer_start <= 1'b1;
                byte_pending <= 1'b1;
            end
        end else if (phase == S_R1) begin
            if (!xfer_in[7]) begin
                r1 <= xfer_in;
                phase <= S_DONE;
            end else if (waits == 0) begin
                r1 <= 8'hff;
                phase <= S_DONE;
            end else begin
                waits <= waits - 1'b1;
                xfer_out <= 8'hff;
                xfer_start <= 1'b1;
                byte_pending <= 1'b1;
            end
        end else begin
            // phase is S_IDLE or S_DONE: advance the card state.
            phase <= S_IDLE;
            unique case (state)
                // At least 74 clocks with CS high, then CMD0.
                POWER_UP: begin
                    if (count < 20) begin
                        count <= count + 1'b1;
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end else begin
                        sd_cs_n_o <= 1'b0;
                        command <= frame(6'd0, 32'd0, 8'h95);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= IDLE_CMD;
                    end
                end
                IDLE_CMD: if (phase == S_DONE) begin
                    if (r1 == 8'h01) begin
                        command <= frame(6'd8, 32'h000001aa, 8'h87);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= IF_COND;
                    end else state <= RETRY;
                end
                // CMD8's R7 trails R1 with four bytes; skip them.
                IF_COND: if (phase == S_DONE) begin
                    count <= 10'd4;
                    state <= r1 == 8'h01 ? IF_COND_TAIL : FAILED;
                end
                IF_COND_TAIL: begin
                    if (count != 0) begin
                        count <= count - 1'b1;
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end else begin
                        retries <= 20'hfffff;
                        command <= frame(6'd55, 32'd0, 8'h65);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= APP_CMD;
                    end
                end
                APP_CMD: if (phase == S_DONE) begin
                    command <= frame(6'd41, 32'h40000000, 8'h77);
                    command_bytes <= 3'd6;
                    phase <= S_GAP;
                    state <= OP_COND;
                end
                OP_COND: if (phase == S_DONE) begin
                    if (r1 == 8'h00) begin
                        command <= frame(6'd58, 32'd0, 8'hfd);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= READ_OCR;
                    end else if (r1 == 8'h01 && retries != 0) begin
                        retries <= retries - 1'b1;
                        command <= frame(6'd55, 32'd0, 8'h65);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= APP_CMD;
                    end else state <= RETRY;
                end
                // OCR bit 30 (CCS) marks block addressing.
                READ_OCR: if (phase == S_DONE) begin
                    count <= 10'd4;
                    state <= r1 == 8'h00 ? OCR_TAIL : FAILED;
                end
                OCR_TAIL: begin
                    if (count != 0) begin
                        if (count == 10'd3) sdhc <= xfer_in[6];
                        count <= count - 1'b1;
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end else if (sdhc) begin
                        fast <= 1'b1;
                        state <= READY;
                    end else begin
                        command <= frame(6'd16, 32'd512, 8'hff);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                        state <= SET_BLOCK;
                    end
                end
                SET_BLOCK: if (phase == S_DONE) begin
                    fast <= 1'b1;
                    state <= r1 == 8'h00 ? READY : RETRY;
                end
                READY: if (valid_i) begin
                    request_write  <= write_i;
                    request_sector <= sector_i;
                    request_error  <= 1'b0;
                    state <= write_i ? WRITE_CMD : READ_CMD;
                    command_bytes <= 3'd0;
                end
                // -------------------------------------------------------------
                READ_CMD: begin
                    if (command_bytes == 0 && phase != S_DONE) begin
                        command <= frame(6'd17, block_address, 8'hff);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                    end else if (phase == S_DONE) begin
                        if (r1 != 8'h00) begin request_error <= 1'b1; state <= FINISH; end
                        else begin
                            retries <= 20'hfffff;
                            xfer_out <= 8'hff;
                            xfer_start <= 1'b1;
                            byte_pending <= 1'b1;
                            state <= READ_TOKEN;
                        end
                    end
                end
                // Each visit sees the byte just received.
                READ_TOKEN: begin
                    if (xfer_in == 8'hfe) begin
                        count <= '0;
                        state <= READ_DATA;
                    end else if (xfer_in != 8'hff || retries == 0) begin
                        request_error <= 1'b1;
                        state <= FINISH;
                    end else retries <= retries - 1'b1;
                    if (xfer_in == 8'hfe || (xfer_in == 8'hff && retries != 0)) begin
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end
                end
                READ_DATA: begin
                    read_byte_o  <= 1'b1;
                    read_index_o <= count[8:0];
                    read_data_o  <= xfer_in;
                    if (count == 10'd511) begin
                        count <= 10'd2;
                        state <= READ_CRC;
                    end else count <= count + 1'b1;
                    xfer_out <= 8'hff;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                end
                READ_CRC: begin
                    if (count == 10'd1) state <= FINISH;
                    else begin
                        count <= count - 1'b1;
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end
                end
                // -------------------------------------------------------------
                WRITE_CMD: begin
                    if (command_bytes == 0 && phase != S_DONE) begin
                        command <= frame(6'd24, block_address, 8'hff);
                        command_bytes <= 3'd6;
                        phase <= S_GAP;
                    end else if (phase == S_DONE) begin
                        if (r1 != 8'h00) begin request_error <= 1'b1; state <= FINISH; end
                        else state <= WRITE_GAP;
                    end
                end
                // One idle byte, then the start token.
                WRITE_GAP: begin
                    xfer_out <= 8'hff;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                    state <= WRITE_TOKEN;
                end
                WRITE_TOKEN: begin
                    xfer_out <= 8'hfe;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                    count <= '0;
                    state <= WRITE_DATA;
                end
                // write_index_o has shown `count` since the previous byte.
                WRITE_DATA: begin
                    xfer_out <= write_data_i;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                    if (count == 10'd511) begin
                        count <= 10'd2;
                        state <= WRITE_CRC;
                    end else count <= count + 1'b1;
                end
                WRITE_CRC: begin
                    xfer_out <= 8'hff;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                    if (count == 10'd1) begin
                        retries <= 20'd16;
                        state <= WRITE_RESPONSE;
                    end else count <= count - 1'b1;
                end
                // Data response xxx0_0101 means accepted.
                WRITE_RESPONSE: begin
                    if (xfer_in[4:0] == 5'b00101) begin
                        retries <= 20'hfffff;
                        state <= WRITE_BUSY;
                    end else if (xfer_in != 8'hff || retries == 0) begin
                        request_error <= 1'b1;
                        state <= FINISH;
                    end else retries <= retries - 1'b1;
                    xfer_out <= 8'hff;
                    xfer_start <= 1'b1;
                    byte_pending <= 1'b1;
                end
                // The card holds MISO low while it programs the block.
                WRITE_BUSY: begin
                    if (xfer_in == 8'hff) state <= FINISH;
                    else if (retries == 0) begin request_error <= 1'b1; state <= FINISH; end
                    else begin
                        retries <= retries - 1'b1;
                        xfer_out <= 8'hff;
                        xfer_start <= 1'b1;
                        byte_pending <= 1'b1;
                    end
                end
                FINISH: begin
                    done_o  <= 1'b1;
                    error_o <= request_error;
                    state   <= READY;
                end
                // Start setup again with CS high; give up after 200 tries.
                RETRY: begin
                    sd_cs_n_o <= 1'b1;
                    fast <= 1'b0;
                    sdhc <= 1'b0;
                    count <= '0;
                    attempts <= attempts + 1'b1;
                    state <= attempts == 8'd199 ? FAILED : POWER_UP;
                end
                FAILED: begin end
                default: state <= FAILED;
            endcase
        end
    end

    wire unused = &{1'b0, request_write};
endmodule

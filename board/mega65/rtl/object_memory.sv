// Object memory in SDRAM, behind OBJEKT's memory request/completion channel,
// with a block-RAM cache of recently used words in front of the chip.
//
// The cache is direct-mapped by word address and holds 2**CACHE_BITS words.
// A read that hits completes two cycles after it is accepted; the chip takes
// about nine. A write updates the cache, completes one cycle after it is
// accepted, and joins a short queue that drains to the chip in the
// background. The chip therefore receives every write, in order, and a cache
// entry can be replaced at any time without being written back. A read that
// misses waits for the queue to empty before it reads the chip, so it never
// sees a word older than a queued write. The cache is emptied after reset.
//
// After reset it first checks the chip: a distinct pattern and then its
// complement at address 0 and at each single-address-bit address, which
// finds stuck or shorted address and data lines in a few hundred cycles.
// test_done_o rises when the check ends; test_failed_o and test_address_o
// report the first mismatch. OBJEKT's channel is served only after a pass.
module object_memory #(
    parameter integer MEMORY_WORDS = 8388608,
    parameter integer CLK_HZ = 25_200_000,
    parameter integer CACHE_BITS = 14,
    parameter integer QUEUE_BITS = 4
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
    // The cache: {valid, tag, word} per entry, one write port and one
    // registered read port.
    // ---------------------------------------------------------------------
    localparam integer TAG_BITS = 23 - CACHE_BITS;
    localparam integer ENTRY_BITS = 41 + TAG_BITS;
    logic [ENTRY_BITS-1:0] cache [1 << CACHE_BITS];
    logic [ENTRY_BITS-1:0] cache_q, cache_d;
    logic                  cache_we;
    logic [CACHE_BITS-1:0] cache_wa;
    always_ff @(posedge clk_i) begin
        if (cache_we) cache[cache_wa] <= cache_d;
        cache_q <= cache[mem_addr_i[CACHE_BITS-1:0]];
    end

    // The write queue.
    logic [62:0] queue [1 << QUEUE_BITS];
    logic [QUEUE_BITS-1:0] queue_head, queue_tail;
    logic [QUEUE_BITS:0]   queue_count;
    wire  queue_empty = queue_count == 0;
    wire  queue_full = queue_count[QUEUE_BITS];
    wire  [62:0] queue_front = queue[queue_head];

    // ---------------------------------------------------------------------
    // OBJEKT's channel: one request, held until its completion is accepted.
    // ---------------------------------------------------------------------
    typedef enum logic [2:0] {CLEAR, IDLE, LOOKUP, MISS, FILL, DONE} state_t;
    state_t state;
    logic [CACHE_BITS-1:0] clear_index;
    logic [22:0] request_addr;
    logic        chip_busy, chip_reading;
    wire  in_range = {8'd0, mem_addr_i} < 32'(MEMORY_WORDS) && !mem_addr_i[23];
    wire  mem_accept = mem_valid_i && mem_ready_o;
    wire  hit = cache_q[ENTRY_BITS-1] && cache_q[ENTRY_BITS-2:40] == request_addr[22:CACHE_BITS];

    // A write needs room in the queue.
    assign mem_ready_o = test_done_o && !test_failed_o && state == IDLE &&
                         !(mem_write_i && in_range && queue_full);
    assign mem_rsp_valid_o = state == DONE;

    // The chip serves the power-on check, then queued writes, then a missed
    // read once the queue is empty.
    wire  read_chip = state == MISS && queue_empty && !chip_busy;
    wire  write_chip = !queue_empty && !chip_busy;
    always_comb begin
        if (!test_done_o) begin
            valid = !test_busy;
            write = !phase[0];
            addr  = test_addr;
            wdata = pattern;
        end else begin
            valid = read_chip || write_chip;
            write = write_chip;
            addr  = write_chip ? queue_front[62:40] : request_addr;
            wdata = {24'd0, queue_front[39:0]};
        end
    end

    always_comb begin
        cache_we = 1'b0;
        cache_wa = request_addr[CACHE_BITS-1:0];
        cache_d = {1'b1, request_addr[22:CACHE_BITS], rdata[39:0]};
        if (state == CLEAR) begin
            cache_we = 1'b1;
            cache_wa = clear_index;
            cache_d = '0;
        end else if (mem_accept && mem_write_i && in_range) begin
            cache_we = 1'b1;
            cache_wa = mem_addr_i[CACHE_BITS-1:0];
            cache_d = {1'b1, mem_addr_i[22:CACHE_BITS], mem_data_i};
        end else if (state == FILL) cache_we = 1'b1;
    end

    always_ff @(posedge clk_i) begin
        if (mem_accept && mem_write_i && in_range)
            queue[queue_tail] <= {mem_addr_i[22:0], mem_data_i};
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            phase <= '0;
            index <= '0;
            test_busy <= 1'b0;
            test_done_o <= 1'b0;
            test_failed_o <= 1'b0;
            test_address_o <= '0;
            state <= CLEAR;
            clear_index <= '0;
            chip_busy <= 1'b0;
            chip_reading <= 1'b0;
            queue_head <= '0;
            queue_tail <= '0;
            queue_count <= '0;
        end else begin
            if (state == CLEAR) begin
                clear_index <= clear_index + 1'b1;
                if (&clear_index) state <= IDLE;
            end
            if (!test_done_o) begin
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
                // The chip: one transfer at a time.
                if (valid && ready) begin
                    chip_busy <= 1'b1;
                    chip_reading <= read_chip;
                    if (write_chip) queue_head <= queue_head + 1'b1;
                end
                if (chip_busy && rsp_valid) chip_busy <= 1'b0;
                queue_count <= queue_count
                    + (QUEUE_BITS+1)'(mem_accept && mem_write_i && in_range)
                    - (QUEUE_BITS+1)'(valid && ready && write_chip);
                if (mem_accept && mem_write_i && in_range) queue_tail <= queue_tail + 1'b1;

                unique case (state)
                    IDLE: if (mem_accept) begin
                        request_addr <= mem_addr_i[22:0];
                        mem_rsp_error_o <= !in_range;
                        state <= (mem_write_i || !in_range) ? DONE : LOOKUP;
                    end
                    LOOKUP: begin
                        mem_rsp_data_o <= cache_q[39:0];
                        state <= hit ? DONE : MISS;
                    end
                    MISS: if (chip_busy && chip_reading && rsp_valid) begin
                        mem_rsp_data_o <= rdata[39:0];
                        state <= FILL;
                    end
                    // The fill writes the cache from rdata, which the chip
                    // holds until its next transfer.
                    FILL: state <= DONE;
                    DONE: if (mem_rsp_ready_i) state <= IDLE;
                    default: begin end
                endcase
            end
        end
    end
endmodule

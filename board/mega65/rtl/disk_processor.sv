// The disk processor: a PicoRV32 (RV32IM) running firmware from block RAM.
// It serves OBJEKT's backing-store channel through a mailbox and reaches the
// SD card through the sector engine. Everything here runs on the machine
// clock, so the mailbox needs no clock crossing.
//
// Memory map (32-bit words; byte enables honoured in RAM only):
//   0x0000_0000  RAM, RAM_BYTES, loaded from FIRMWARE ($readmemh, 32-bit words)
//   0x1000_0000  SD engine
//                  +0x00 status (r): 0 ready, 1 busy, 2 setup failed, 3 SDHC,
//                                    4 last request failed; 31:27 controller
//                                    state, 26:19 last R1, 18:11 attempts
//                  +0x04 sector (w)
//                  +0x08 command (w): 1 read into the buffer, 2 write from it
//                  +0x200-0x3ff sector buffer, little-endian words
//   0x2000_0000  store mailbox
//                  +0x00 (r) bit 0: a request is waiting
//                  +0x04 op       +0x08/0c ref lo/hi   +0x10/14 class lo/hi
//                  +0x18 size     +0x1c cond           +0x20 offset
//                  +0x24/28 data lo/hi
//                  +0x40 status (w)  +0x44/48 ref lo/hi  +0x4c/50 class lo/hi
//                  +0x54 size       +0x58 cond         +0x5c/60 data lo/hi
//                  +0x64 (w) send the response; the request slot frees once
//                        OBJEKT accepts it
//   0x3000_0000  display rows 0-7 (w)
//   0x4000_0000  machine boot port
//                  +0x00 status (r): 0 LOGIK halted, 1 memory check done,
//                        2 memory check failed, 3 machine started,
//                        4 a programming write is still pending
//                  +0x04 (w) low half of a boot_stream record
//                  +0x08 (w) high half; a programming record (kind 1) is then
//                        written to LOGIK, a service record (kind 3) runs an
//                        OBJEKT service operation, a start record (kind 2)
//                        starts the processor
module disk_processor #(
    parameter integer RAM_BYTES = 65536,
    parameter FIRMWARE = "firmware.mem"
)(
    input  logic        clk_i,
    input  logic        rst_i,

    // OBJEKT's backing-store channel
    input  logic        store_valid_i,
    output logic        store_ready_o,
    input  logic [3:0]  store_op_i,
    input  logic [39:0] store_ref_i,
    input  logic [39:0] store_class_i,
    input  logic [23:0] store_size_i,
    input  logic        store_cond_i,
    input  logic [23:0] store_offset_i,
    input  logic [39:0] store_data_i,
    output logic        store_rsp_valid_o,
    input  logic        store_rsp_ready_i,
    output logic [3:0]  store_rsp_status_o,
    output logic [39:0] store_rsp_ref_o,
    output logic [39:0] store_rsp_class_o,
    output logic [23:0] store_rsp_size_o,
    output logic        store_rsp_cond_o,
    output logic [39:0] store_rsp_data_o,

    // SD card (SPI mode)
    output logic        sd_clk_o,
    output logic        sd_cs_n_o,
    output logic        sd_mosi_o,
    input  logic        sd_miso_i,

    // Machine boot port
    output logic [63:0] boot_record_o,
    output logic        boot_valid_o,
    input  logic        boot_ready_i,
    output logic        start_o,
    input  logic [3:0]  machine_status_i,

    output logic [31:0] display_o [8]
);
    localparam integer RAM_WORDS = RAM_BYTES / 4;

    logic        mem_valid, mem_instr, mem_ready;
    logic [31:0] mem_addr, mem_wdata, mem_rdata;
    logic [3:0]  mem_wstrb;

    /* verilator lint_off PINCONNECTEMPTY */
    picorv32 #(
        .ENABLE_COUNTERS(1), .ENABLE_COUNTERS64(0),
        .ENABLE_MUL(1), .ENABLE_DIV(1),
        .COMPRESSED_ISA(0), .BARREL_SHIFTER(1),
        .PROGADDR_RESET(32'h0), .STACKADDR(32'(RAM_BYTES))
    ) cpu (
        .clk(clk_i), .resetn(!rst_i), .trap(),
        .mem_valid(mem_valid), .mem_instr(mem_instr), .mem_ready(mem_ready),
        .mem_addr(mem_addr), .mem_wdata(mem_wdata), .mem_wstrb(mem_wstrb),
        .mem_rdata(mem_rdata),
        .mem_la_read(), .mem_la_write(), .mem_la_addr(), .mem_la_wdata(), .mem_la_wstrb(),
        .pcpi_valid(), .pcpi_insn(), .pcpi_rs1(), .pcpi_rs2(),
        .pcpi_wr(1'b0), .pcpi_rd(32'd0), .pcpi_wait(1'b0), .pcpi_ready(1'b0),
        .irq(32'd0), .eoi(), .trace_valid(), .trace_data()
    );
    /* verilator lint_on PINCONNECTEMPTY */

    wire [3:0] region = mem_addr[31:28];
    wire       write  = |mem_wstrb;

    // ---------------------------------------------------------------------
    // RAM
    // ---------------------------------------------------------------------
    logic [31:0] ram [RAM_WORDS];
    initial $readmemh(FIRMWARE, ram);
    logic [31:0] ram_rdata;
    wire  [$clog2(RAM_WORDS)-1:0] ram_index = mem_addr[$clog2(RAM_BYTES)-1:2];
    always_ff @(posedge clk_i) begin
        if (mem_valid && !mem_ready && region == 4'h0) begin
            for (int i = 0; i < 4; i++) if (mem_wstrb[i]) ram[ram_index][8*i +: 8] <= mem_wdata[8*i +: 8];
            ram_rdata <= ram[ram_index];
        end
    end

    // ---------------------------------------------------------------------
    // SD engine and its sector buffer
    // ---------------------------------------------------------------------
    logic        sd_valid, sd_ready, sd_write, sd_done, sd_error, sd_failed, sd_sdhc;
    logic        sd_busy, sd_last_error;
    logic [20:0] sd_debug;
    logic [31:0] sd_sector;
    logic        sd_read_byte;
    logic [8:0]  sd_read_index, sd_write_index;
    logic [7:0]  sd_read_data, sd_write_data;

    sd_spi #(.SLOW_DIVIDE(16), .FAST_DIVIDE(1)) sd (
        .clk_i, .rst_i,
        .sd_clk_o, .sd_cs_n_o, .sd_mosi_o, .sd_miso_i,
        .valid_i(sd_valid), .ready_o(sd_ready), .write_i(sd_write), .sector_i(sd_sector),
        .done_o(sd_done), .error_o(sd_error), .failed_o(sd_failed), .sdhc_o(sd_sdhc),
        .debug_o(sd_debug),
        .read_byte_o(sd_read_byte), .read_index_o(sd_read_index), .read_data_o(sd_read_data),
        .write_index_o(sd_write_index), .write_data_i(sd_write_data));

    // 512-byte buffer as a true dual-port RAM of 128 words. Port A serves the
    // SD engine a byte at a time; port B serves the processor's word accesses.
    // Each port is written from its own process so it maps to block RAM.
    logic [31:0] buffer [128];
    logic [31:0] buffer_rdata, sd_word;
    logic [1:0]  sd_lane;
    wire  [8:0]  sd_index = sd_read_byte ? sd_read_index : sd_write_index;
    wire  [3:0]  sd_we = sd_read_byte ? 4'b0001 << sd_read_index[1:0] : 4'b0000;
    wire  [3:0]  cpu_we = (mem_valid && !mem_ready && region == 4'h1 && mem_addr[9]) ? mem_wstrb : 4'b0000;

    always_ff @(posedge clk_i) begin
        for (int i = 0; i < 4; i++)
            if (sd_we[i]) buffer[sd_index[8:2]][8*i +: 8] <= sd_read_data;
        sd_word <= buffer[sd_index[8:2]];
        sd_lane <= sd_index[1:0];
    end
    always_ff @(posedge clk_i) begin
        for (int i = 0; i < 4; i++)
            if (cpu_we[i]) buffer[mem_addr[8:2]][8*i +: 8] <= mem_wdata[8*i +: 8];
        buffer_rdata <= buffer[mem_addr[8:2]];
    end
    // The SD engine samples write data well after presenting its index.
    assign sd_write_data = sd_word[8*sd_lane +: 8];

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            sd_valid <= 1'b0;
            sd_busy <= 1'b0;
            sd_last_error <= 1'b0;
        end else begin
            if (sd_valid && sd_ready) sd_valid <= 1'b0;
            if (sd_done) begin sd_busy <= 1'b0; sd_last_error <= sd_error; end
            if (mem_valid && !mem_ready && write && region == 4'h1 && !mem_addr[9]) begin
                if (mem_addr[7:0] == 8'h04) sd_sector <= mem_wdata;
                if (mem_addr[7:0] == 8'h08 && !sd_busy && (mem_wdata == 1 || mem_wdata == 2)) begin
                    sd_write <= mem_wdata == 2;
                    sd_valid <= 1'b1;
                    sd_busy <= 1'b1;
                end
            end
        end
    end
    // Bits 31:11 carry the controller's diagnostics: state 31:27, last R1
    // 26:19, setup attempts 18:11.
    wire [31:0] sd_status = {sd_debug, 6'd0, sd_last_error, sd_sdhc, sd_failed, sd_busy,
                             sd_ready && !sd_busy};

    // ---------------------------------------------------------------------
    // Store mailbox
    // ---------------------------------------------------------------------
    logic        request_waiting;
    logic [3:0]  request_op;
    logic [39:0] request_ref, request_class, request_data;
    logic [23:0] request_size, request_offset;
    logic        request_cond;

    assign store_ready_o = !request_waiting && !store_rsp_valid_o;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            request_waiting <= 1'b0;
            store_rsp_valid_o <= 1'b0;
        end else begin
            if (store_valid_i && store_ready_o) begin
                request_waiting <= 1'b1;
                request_op <= store_op_i;
                request_ref <= store_ref_i;
                request_class <= store_class_i;
                request_size <= store_size_i;
                request_cond <= store_cond_i;
                request_offset <= store_offset_i;
                request_data <= store_data_i;
            end
            if (store_rsp_valid_o && store_rsp_ready_i) begin
                store_rsp_valid_o <= 1'b0;
                request_waiting <= 1'b0;
            end
            if (mem_valid && !mem_ready && write && region == 4'h2) begin
                unique case (mem_addr[7:0])
                    8'h40: store_rsp_status_o <= mem_wdata[3:0];
                    8'h44: store_rsp_ref_o[31:0] <= mem_wdata;
                    8'h48: store_rsp_ref_o[39:32] <= mem_wdata[7:0];
                    8'h4c: store_rsp_class_o[31:0] <= mem_wdata;
                    8'h50: store_rsp_class_o[39:32] <= mem_wdata[7:0];
                    8'h54: store_rsp_size_o <= mem_wdata[23:0];
                    8'h58: store_rsp_cond_o <= mem_wdata[0];
                    8'h5c: store_rsp_data_o[31:0] <= mem_wdata;
                    8'h60: store_rsp_data_o[39:32] <= mem_wdata[7:0];
                    8'h64: if (request_waiting) store_rsp_valid_o <= 1'b1;
                    default: begin end
                endcase
            end
        end
    end

    logic [31:0] mailbox_rdata;
    always_comb begin
        unique case (mem_addr[7:0])
            8'h00: mailbox_rdata = {31'd0, request_waiting && !store_rsp_valid_o};
            8'h04: mailbox_rdata = {28'd0, request_op};
            8'h08: mailbox_rdata = request_ref[31:0];
            8'h0c: mailbox_rdata = {24'd0, request_ref[39:32]};
            8'h10: mailbox_rdata = request_class[31:0];
            8'h14: mailbox_rdata = {24'd0, request_class[39:32]};
            8'h18: mailbox_rdata = {8'd0, request_size};
            8'h1c: mailbox_rdata = {31'd0, request_cond};
            8'h20: mailbox_rdata = {8'd0, request_offset};
            8'h24: mailbox_rdata = request_data[31:0];
            8'h28: mailbox_rdata = {24'd0, request_data[39:32]};
            default: mailbox_rdata = 32'd0;
        endcase
    end

    // ---------------------------------------------------------------------
    // Machine boot port
    // ---------------------------------------------------------------------
    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            boot_valid_o <= 1'b0;
            start_o <= 1'b0;
            boot_record_o <= '0;
        end else begin
            if (boot_valid_o && boot_ready_i) boot_valid_o <= 1'b0;
            if (mem_valid && !mem_ready && write && region == 4'h4) begin
                if (mem_addr[7:0] == 8'h04) boot_record_o[31:0] <= mem_wdata;
                if (mem_addr[7:0] == 8'h08) begin
                    boot_record_o[63:32] <= mem_wdata;
                    if (mem_wdata[31:30] == 2'd1 || mem_wdata[31:30] == 2'd3) boot_valid_o <= 1'b1;
                    if (mem_wdata[31:30] == 2'd2) start_o <= 1'b1;
                end
            end
        end
    end
    wire [31:0] boot_status = {27'd0, boot_valid_o, machine_status_i};

    // ---------------------------------------------------------------------
    // Display rows
    // ---------------------------------------------------------------------
    always_ff @(posedge clk_i) begin
        if (rst_i) for (int i = 0; i < 8; i++) display_o[i] <= '0;
        else if (mem_valid && !mem_ready && write && region == 4'h3)
            display_o[mem_addr[4:2]] <= mem_wdata;
    end

    // ---------------------------------------------------------------------
    // Bus: every access completes on the cycle after it is presented.
    // ---------------------------------------------------------------------
    logic [3:0]  region_q;
    logic        sd_buffer_q;
    logic [31:0] register_q;
    always_ff @(posedge clk_i) begin
        if (rst_i) mem_ready <= 1'b0;
        else mem_ready <= mem_valid && !mem_ready;
        region_q <= region;
        sd_buffer_q <= mem_addr[9];
        register_q <= region == 4'h1 ? sd_status : region == 4'h4 ? boot_status : mailbox_rdata;
    end
    assign mem_rdata = region_q == 4'h0 ? ram_rdata :
                       region_q == 4'h1 && sd_buffer_q ? buffer_rdata : register_q;

    wire unused = &{1'b0, mem_instr};
endmodule

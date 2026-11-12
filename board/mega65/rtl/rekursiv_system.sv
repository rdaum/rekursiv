// The machine for the first MEGA65 bring-up: LOGIK and OBJEKT with board
// implementations of their external channels.
//
// - Object memory is block RAM.
// - The disk processor serves the backing store from the SD card.
// - The device channel serves only the debug display (0xf00-0xf1f).
// - A boot sequencer replays a stream from BOOT_FILE through LOGIK's halted
//   programming port, then starts the processor. The start record says
//   whether collection is enabled and where the collector begins.
module rekursiv_system #(
    parameter integer PAGER_BITS = 10,
    parameter integer MEMORY_WORDS = 16384,
    parameter integer CODE_WORDS = 8192,
    parameter integer BOOT_RECORDS = 1024,
    parameter BOOT_FILE = "boot.mem",
    parameter FIRMWARE = "firmware.mem"
)(
    input  logic        clk_i,
    input  logic        rst_i,
    output logic        sd_clk_o,
    output logic        sd_cs_n_o,
    output logic        sd_mosi_o,
    input  logic        sd_miso_i,
    output logic [31:0] disk_display_o [8],
    output logic [31:0] debug_o [8],
    output logic        booted_o,
    output logic        halted_o,
    output logic        service_o,
    output logic [3:0]  fault_o,
    output logic [15:0] pc_o
);
    // ---------------------------------------------------------------------
    // Boot sequencer
    // ---------------------------------------------------------------------
    logic [63:0] boot_rom [BOOT_RECORDS];
    initial $readmemh(BOOT_FILE, boot_rom);

    logic [$clog2(BOOT_RECORDS)-1:0] boot_index;
    logic        boot_done;
    wire  [63:0] record = boot_rom[boot_index];
    wire  [1:0]  kind = record[63:62];
    logic        boot_ready, start;
    logic        gc_enable;
    logic [15:0] gc_entry;

    wire boot_valid = !boot_done && kind == 2'd1;
    assign start = !boot_done && kind == 2'd2 && halted_o;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            boot_index <= '0;
            boot_done  <= 1'b0;
            gc_enable  <= 1'b0;
            gc_entry   <= '0;
        end else if (!boot_done) begin
            if (start) begin
                gc_enable <= record[16];
                gc_entry  <= record[47:32];
            end
            if (boot_valid && boot_ready) boot_index <= boot_index + 1'b1;
            if (start || kind == 2'd0 || kind == 2'd3) boot_done <= 1'b1;
        end
    end
    assign booted_o = boot_done;

    // ---------------------------------------------------------------------
    // LOGIK <-> OBJEKT
    // ---------------------------------------------------------------------
    logic gc_valid, gc_ready, gc_response, gc_response_ready, gc_committed, gc_explicit, gc_active;
    logic [3:0] gc_operation, gc_status;
    logic [39:0] gc_data, gc_root, gc_result;

    logic cmd_valid, cmd_ready, rsp_valid, rsp_ready;
    logic [3:0] pager, index, read, rsp_status;
    logic [2:0] register, vr;
    logic [1:0] memory;
    logic prepare, prepared, load_vr, alloc_scan, check_type;
    logic [23:0] alloc_size;
    logic [39:0] data, expected_type, rsp_data;

    logic io_valid, io_ready, io_write, io_response, io_response_ready, io_error;
    logic [31:0] io_address, io_data, io_result;

    logic mem_valid, mem_ready, mem_write, mem_rsp_valid, mem_rsp_ready, mem_rsp_error;
    logic [23:0] mem_addr;
    logic [39:0] mem_data, mem_rsp_data;

    logic store_valid, store_ready, store_rsp_valid, store_rsp_ready, store_cond, store_rsp_cond;
    logic [3:0]  store_op, store_rsp_status;
    logic [39:0] store_ref, store_class, store_data;
    logic [39:0] store_rsp_ref, store_rsp_class, store_rsp_data;
    logic [23:0] store_size, store_offset, store_rsp_size;

    /* verilator lint_off PINCONNECTEMPTY */
    logik #(.CODE_WORDS(CODE_WORDS)) processor (
        .clk_i, .rst_i,
        .boot_valid_i(boot_valid), .boot_ready_o(boot_ready),
        .boot_space_i(record[57:56]), .boot_addr_i(record[47:32]),
        .boot_lane_i(record[50:48]), .boot_data_i(record[31:0]),
        .gc_enable_i(gc_enable), .gc_entry_i(gc_entry),
        .gc_active_o(gc_active), .gc_explicit_o(gc_explicit),
        .gc_valid_o(gc_valid), .gc_ready_i(gc_ready),
        .gc_operation_o(gc_operation), .gc_data_o(gc_data), .gc_root_o(gc_root),
        .gc_response_i(gc_response), .gc_response_ready_o(gc_response_ready),
        .gc_status_i(gc_status), .gc_result_i(gc_result),
        .gc_committed_i(gc_committed), .last_status_o(),
        .start_i(start), .entry_i(record[15:0]), .resume_i(1'b0), .irq_i(1'b0),
        .halted_o, .service_o, .service_code_o(), .fault_o, .retire_o(),
        .cmd_valid_o(cmd_valid), .cmd_ready_i(cmd_ready),
        .pager_o(pager), .index_o(index), .register_o(register), .memory_o(memory),
        .prepare_o(prepare), .prepared_o(prepared),
        .read_o(read), .load_vr_o(load_vr), .vr_o(vr),
        .alloc_size_o(alloc_size), .alloc_scan_o(alloc_scan),
        .data_o(data), .check_type_o(check_type), .expected_type_o(expected_type),
        .rsp_valid_i(rsp_valid), .rsp_ready_o(rsp_ready),
        .rsp_status_i(rsp_status), .rsp_data_i(rsp_data),
        .io_valid_o(io_valid), .io_ready_i(io_ready),
        .io_write_o(io_write), .io_address_o(io_address), .io_data_o(io_data),
        .io_response_i(io_response), .io_response_ready_o(io_response_ready),
        .io_error_i(io_error), .io_result_i(io_result),
        .device_result_o(),
        .pc_o, .upcor_o(), .mark_o(), .sp_o(), .esp_o(), .csp_o(), .ap_o(), .apc_o(),
        .estkr_o(), .cstkr_o(), .symbol_o(), .object_o(),
        .q_o(), .product_o(), .flags_o(), .fp_flags_o(), .lastcc_o(),
        .ucar_o(), .namarg_o(),
        .dbg_addr_i(16'd0), .dbg_rf_o(), .dbg_estk_o(), .dbg_cstk_o(),
        .dbg_code_valid_o(), .dbg_code_data_o(), .dbg_code_type_o(), .dbg_root_o()
    );

    objekt #(.PAGER_BITS(PAGER_BITS), .MEMORY_WORDS(MEMORY_WORDS)) core (
        .clk_i, .rst_i, .run_i(1'b1),
        .gc_enable_i(gc_enable), .gc_active_i(gc_active), .gc_explicit_i(gc_explicit),
        .gc_valid_i(gc_valid), .gc_ready_o(gc_ready), .gc_operation_i(gc_operation),
        .gc_data_i(gc_data), .gc_root_i(gc_root),
        .gc_response_o(gc_response), .gc_response_ready_i(gc_response_ready),
        .gc_status_o(gc_status), .gc_result_o(gc_result), .gc_committed_o(gc_committed),
        .cmd_valid_i(cmd_valid), .cmd_ready_o(cmd_ready),
        .pager_i(pager), .index_i(index), .register_i(register), .memory_i(memory),
        .prepare_i(prepare), .prepared_i(prepared),
        .read_i(read), .load_vr_i(load_vr), .vr_i(vr),
        .alloc_size_i(alloc_size), .alloc_scan_i(alloc_scan),
        .data_i(data), .check_type_i(check_type), .expected_type_i(expected_type),
        .svc_valid_i(1'b0), .svc_ready_o(), .svc_op_i(4'd0),
        .svc_ref_i('0), .svc_class_i('0), .svc_base_i('0), .svc_size_i('0),
        .svc_repr_i('0), .svc_flags_i('0), .svc_code_i('0),
        .rsp_valid_o(rsp_valid), .rsp_ready_i(rsp_ready),
        .rsp_status_o(rsp_status), .rsp_data_o(rsp_data),
        .mem_valid_o(mem_valid), .mem_ready_i(mem_ready),
        .mem_write_o(mem_write), .mem_addr_o(mem_addr), .mem_data_o(mem_data),
        .mem_rsp_valid_i(mem_rsp_valid), .mem_rsp_ready_o(mem_rsp_ready),
        .mem_rsp_data_i(mem_rsp_data), .mem_rsp_error_i(mem_rsp_error),
        .store_valid_o(store_valid), .store_ready_i(store_ready), .store_op_o(store_op),
        .store_ref_o(store_ref), .store_class_o(store_class), .store_size_o(store_size),
        .store_cond_o(store_cond), .store_offset_o(store_offset), .store_data_o(store_data),
        .store_rsp_valid_i(store_rsp_valid), .store_rsp_ready_o(store_rsp_ready),
        .store_rsp_status_i(store_rsp_status),
        .store_rsp_ref_i(store_rsp_ref), .store_rsp_class_i(store_rsp_class),
        .store_rsp_size_i(store_rsp_size), .store_rsp_cond_i(store_rsp_cond),
        .store_rsp_data_i(store_rsp_data),
        .dbg_next_identity_o(), .dbg_body_cursor_o(), .dbg_maintenance_o(),
        .dbg_class_code_i(2'd0), .dbg_class_valid_o(), .dbg_compact_class_o(),
        .dbg_vr_i(3'd0), .dbg_vr_o(), .dbg_idx_o(), .dbg_reg_o(),
        .dbg_prepared_ref_o(), .dbg_prepared_index_o(), .dbg_prepared_address_o(),
        .dbg_prepared_status_o(), .dbg_selected_o(), .dbg_ref_o(),
        .dbg_class_o(), .dbg_size_o(), .dbg_base_o(), .dbg_repr_o(), .dbg_flags_o(),
        .dbg_slot_i('0), .dbg_valid_o(),
        .dbg_entry_ref_o(), .dbg_entry_class_o(), .dbg_entry_size_o(), .dbg_entry_base_o(),
        .dbg_entry_repr_o(), .dbg_entry_flags_o()
    );
    /* verilator lint_on PINCONNECTEMPTY */

    // ---------------------------------------------------------------------
    // Object memory: one request at a time, completed on the next edge.
    // ---------------------------------------------------------------------
    logic [39:0] object_memory [MEMORY_WORDS];
    logic        mem_pending;
    wire         mem_accept = mem_valid && mem_ready;
    wire         mem_in_range = {8'd0, mem_addr} < 32'(MEMORY_WORDS);

    assign mem_ready     = !mem_pending;
    assign mem_rsp_valid = mem_pending;

    always_ff @(posedge clk_i) begin
        if (mem_accept && mem_in_range) begin
            if (mem_write) object_memory[mem_addr[$clog2(MEMORY_WORDS)-1:0]] <= mem_data;
            mem_rsp_data <= object_memory[mem_addr[$clog2(MEMORY_WORDS)-1:0]];
        end
    end
    always_ff @(posedge clk_i) begin
        if (rst_i) mem_pending <= 1'b0;
        else if (mem_accept) begin
            mem_pending   <= 1'b1;
            mem_rsp_error <= !mem_in_range;
        end else if (mem_rsp_valid && mem_rsp_ready) mem_pending <= 1'b0;
    end

    // ---------------------------------------------------------------------
    // Backing store: the disk processor.
    // ---------------------------------------------------------------------
    disk_processor #(.FIRMWARE(FIRMWARE)) disk (
        .clk_i, .rst_i,
        .store_valid_i(store_valid), .store_ready_o(store_ready), .store_op_i(store_op),
        .store_ref_i(store_ref), .store_class_i(store_class), .store_size_i(store_size),
        .store_cond_i(store_cond), .store_offset_i(store_offset), .store_data_i(store_data),
        .store_rsp_valid_o(store_rsp_valid), .store_rsp_ready_i(store_rsp_ready),
        .store_rsp_status_o(store_rsp_status), .store_rsp_ref_o(store_rsp_ref),
        .store_rsp_class_o(store_rsp_class), .store_rsp_size_o(store_rsp_size),
        .store_rsp_cond_o(store_rsp_cond), .store_rsp_data_o(store_rsp_data),
        .sd_clk_o, .sd_cs_n_o, .sd_mosi_o, .sd_miso_i,
        .display_o(disk_display_o));

    // ---------------------------------------------------------------------
    // Devices: the debug display only.
    // ---------------------------------------------------------------------
    logic io_pending;
    wire  io_debug = io_address[31:5] == 27'h78;   // 0xf00-0xf1f
    assign io_ready    = !io_pending;
    assign io_response = io_pending;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            io_pending <= 1'b0;
            for (int i = 0; i < 8; i++) debug_o[i] <= '0;
        end else if (io_valid && io_ready) begin
            io_pending <= 1'b1;
            io_error   <= !io_debug;
            io_result  <= io_write ? 32'd0 : debug_o[io_address[4:2]];
            if (io_write && io_debug) debug_o[io_address[4:2]] <= io_data;
        end else if (io_response && io_response_ready) io_pending <= 1'b0;
    end
endmodule

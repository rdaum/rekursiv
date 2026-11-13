// The machine for the first MEGA65 bring-up: LOGIK and OBJEKT with board
// implementations of their external channels.
//
// - Object memory is the board's SDRAM: 2^23 words.
// - The disk processor serves the backing store from the SD card.
// - The device channel serves identification (0x000-0x008), events, clocks,
//   the pointer and the input FIFO (0x100-0x40f), the cursor bitmap bank
//   (0x410-0x447), the display bitmap bank (0x510-0x547) and the debug
//   display (0xf00-0xf1f). Input comes from the MEGA65's keyboard and an
//   Atari ST mouse on either joystick port.
// - The disk processor loads the microcode: it replays a boot stream from a
//   file on the card through LOGIK's halted programming port, then starts
//   the processor. The start record says whether collection is enabled and
//   where the collector begins.
module rekursiv_system #(
    parameter integer PAGER_BITS = 15,
    parameter integer MEMORY_WORDS = 8388608,
    parameter integer CODE_WORDS = 8192,
    parameter FIRMWARE = "loader.mem"
)(
    input  logic        clk_i,
    input  logic        rst_i,
    output logic        sd_clk_o,
    output logic        sd_cs_n_o,
    output logic        sd_mosi_o,
    input  logic        sd_miso_i,
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
    input  logic [15:0] sdram_dq_i,
    output logic        memory_tested_o,
    output logic        memory_failed_o,
    output logic [22:0] memory_failed_address_o,
    output logic [31:0] disk_display_o [8],
    output logic [31:0] debug_o [8],
    output logic [31:0] perf_o [8],        // see perf_counters
    output logic [79:0] keys_held_o,       // keyboard state by key bit
    // The monitor: pause_i holds LOGIK at its next instruction boundary.
    // Page 0 shows R0-R15 on the left; VR0-VR7, Symbol, Object, PC,
    // {SP, ESP, CSP}, ESTKR, CSTKR, OBJEKT's index and selected object on the
    // right. Page 1 shows the evaluation stack from SP down on the left and
    // the control stack from CSP down on the right.
    input  logic        pause_i,
    input  logic        monitor_page_i,
    output logic [39:0] monitor_left_o [16],
    output logic [39:0] monitor_right_o [16],
    // The display, scanned out on the video clock (synchronous to clk_i).
    input  logic        video_clk_i,
    input  logic [9:0]  video_x_i,
    input  logic [9:0]  video_y_i,
    output logic        video_cursor_o,    // the cursor covers this position
    // Keyboard and mouse.
    output logic        kb_clock_o,
    output logic        kb_data_o,
    input  logic        kb_data_i,
    input  logic [4:0]  joy_a_n_i,         // {fire, right, left, down, up}
    input  logic [4:0]  joy_b_n_i,
    output logic        video_visible_o,
    output logic        video_inside_o,
    output logic [23:0] video_rgb_o,
    // The table and control store sizes, for the status display.
    output logic [31:0] build_o,
    // The last device request that was refused: its address, and bit 31 for
    // a write. A refused request faults the processor.
    output logic [31:0] io_refused_o,
    output logic        booted_o,
    output logic        halted_o,
    output logic        service_o,
    output logic [3:0]  fault_o,
    output logic [15:0] pc_o
);
    // ---------------------------------------------------------------------
    // Boot: the disk processor programs LOGIK from a file on the card, then
    // starts it. The record layout is boot_stream's.
    // ---------------------------------------------------------------------
    logic [63:0] record;
    logic        boot_valid, boot_ready, start_request, start;
    // A service record runs one halted OBJEKT service operation and is
    // acknowledged by that operation's response.
    logic        logik_boot_ready, svc_ready, svc_sent, rsp_valid;
    wire         record_service = record[63:62] == 2'd3;
    wire         svc_valid = boot_valid && record_service && !svc_sent;
    assign boot_ready = record_service ? svc_sent && rsp_valid : logik_boot_ready;
    logic        gc_enable;
    logic [15:0] gc_entry;

    assign start = start_request && halted_o && !booted_o;
    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            booted_o  <= 1'b0;
            gc_enable <= 1'b0;
            gc_entry  <= '0;
        end else if (start) begin
            gc_enable <= record[16];
            gc_entry  <= record[47:32];
        end else if (start_request && !halted_o) booted_o <= 1'b1;
    end
    always_ff @(posedge clk_i) begin
        if (rst_i) svc_sent <= 1'b0;
        else if (svc_valid && svc_ready) svc_sent <= 1'b1;
        else if (svc_sent && rsp_valid) svc_sent <= 1'b0;
    end

    // ---------------------------------------------------------------------
    // LOGIK <-> OBJEKT
    // ---------------------------------------------------------------------
    logic gc_valid, gc_ready, gc_response, gc_response_ready, gc_committed, gc_explicit, gc_active;
    logic [3:0] gc_operation, gc_status;
    logic [39:0] gc_data, gc_root, gc_result;

    logic cmd_valid, cmd_ready, rsp_ready, retired, dispatched;
    logic [23:0] sp, esp, csp, cstkr, scan_control;
    logic [39:0] estkr, symbol, object, scan_evaluation, scan_vr, objekt_index, objekt_selected;
    logic [31:0] scan_register;
    logic [15:0] scan_address;
    logic [3:0] pager, index, read, rsp_status;
    logic [2:0] register, vr;
    logic [1:0] memory;
    logic prepare, prepared, load_vr, alloc_scan, check_type;
    logic [23:0] alloc_size;
    logic [39:0] data, expected_type, rsp_data;

    logic io_valid, io_ready, io_write, io_response, io_response_ready, io_error, irq, low_space;
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
        .boot_valid_i(boot_valid && !record_service), .boot_ready_o(logik_boot_ready),
        .boot_space_i(record[57:56]), .boot_addr_i(record[47:32]),
        .boot_lane_i(record[50:48]), .boot_data_i(record[31:0]),
        .gc_enable_i(gc_enable), .gc_entry_i(gc_entry),
        .gc_active_o(gc_active), .gc_explicit_o(gc_explicit),
        .gc_valid_o(gc_valid), .gc_ready_i(gc_ready),
        .gc_operation_o(gc_operation), .gc_data_o(gc_data), .gc_root_o(gc_root),
        .gc_response_i(gc_response), .gc_response_ready_o(gc_response_ready),
        .gc_status_i(gc_status), .gc_result_i(gc_result),
        .gc_committed_i(gc_committed), .last_status_o(),
        .start_i(start), .entry_i(record[15:0]), .resume_i(1'b0), .irq_i(irq),
        .low_space_i(low_space), .pending_o(),
        .halted_o, .service_o, .service_code_o(), .fault_o, .retire_o(retired),
        .dispatch_o(dispatched), .pause_i,
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
        .pc_o, .upcor_o(), .mark_o(), .sp_o(sp), .esp_o(esp), .csp_o(csp), .ap_o(), .apc_o(),
        .estkr_o(estkr), .cstkr_o(cstkr), .symbol_o(symbol), .object_o(object),
        .q_o(), .product_o(), .flags_o(), .fp_flags_o(), .lastcc_o(),
        .ucar_o(), .namarg_o(),
        .dbg_addr_i(scan_address), .dbg_rf_o(scan_register), .dbg_estk_o(scan_evaluation),
        .dbg_cstk_o(scan_control),
        .dbg_code_valid_o(), .dbg_code_data_o(), .dbg_code_type_o(), .dbg_root_o()
    );

    objekt #(.PAGER_BITS(PAGER_BITS), .MEMORY_WORDS(MEMORY_WORDS), .DEBUG_PORT(0)) core (
        .clk_i, .rst_i, .run_i(!halted_o), .low_space_o(low_space),
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
        .svc_valid_i(svc_valid), .svc_ready_o(svc_ready), .svc_op_i(record[61:58]),
        .svc_ref_i('0), .svc_class_i(record[39:0]), .svc_base_i('0), .svc_size_i('0),
        .svc_repr_i(record[39:0]), .svc_flags_i('0), .svc_code_i(record[57:52]),
        .rsp_valid_o(rsp_valid), .rsp_ready_i(rsp_ready || halted_o),
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
        .dbg_vr_i(scan_slot[2:0]), .dbg_vr_o(scan_vr), .dbg_idx_o(objekt_index), .dbg_reg_o(),
        .dbg_prepared_ref_o(), .dbg_prepared_index_o(), .dbg_prepared_address_o(),
        .dbg_prepared_status_o(), .dbg_selected_o(), .dbg_ref_o(objekt_selected),
        .dbg_class_o(), .dbg_size_o(), .dbg_base_o(), .dbg_repr_o(), .dbg_flags_o(),
        .dbg_slot_i('0), .dbg_valid_o(),
        .dbg_entry_ref_o(), .dbg_entry_class_o(), .dbg_entry_size_o(), .dbg_entry_base_o(),
        .dbg_entry_repr_o(), .dbg_entry_flags_o()
    );
    /* verilator lint_on PINCONNECTEMPTY */

    // ---------------------------------------------------------------------
    // Object memory: SDRAM, checked at power-up before the machine boots.
    // ---------------------------------------------------------------------
    object_memory #(.MEMORY_WORDS(MEMORY_WORDS)) object_ram (
        .clk_i, .rst_i,
        .mem_valid_i(mem_valid), .mem_ready_o(mem_ready), .mem_write_i(mem_write),
        .mem_addr_i(mem_addr), .mem_data_i(mem_data),
        .mem_rsp_valid_o(mem_rsp_valid), .mem_rsp_ready_i(mem_rsp_ready),
        .mem_rsp_data_o(mem_rsp_data), .mem_rsp_error_o(mem_rsp_error),
        .test_done_o(memory_tested_o), .test_failed_o(memory_failed_o),
        .test_address_o(memory_failed_address_o),
        .sdram_cke_o, .sdram_cs_n_o, .sdram_ras_n_o, .sdram_cas_n_o, .sdram_we_n_o,
        .sdram_ba_o, .sdram_a_o, .sdram_dqml_o, .sdram_dqmh_o,
        .sdram_dq_o, .sdram_dq_oe_o, .sdram_dq_i);

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
        .boot_record_o(record), .boot_valid_o(boot_valid), .boot_ready_i(boot_ready),
        .start_o(start_request),
        .machine_status_i({booted_o, memory_failed_o, memory_tested_o, halted_o}),
        .display_o(disk_display_o));

    // ---------------------------------------------------------------------
    // Devices
    // ---------------------------------------------------------------------
    assign build_o = {8'(PAGER_BITS), 24'(CODE_WORDS)};

    logic io_pending;
    wire  io_debug = io_address[31:5] == 27'h78;   // 0xf00-0xf1f
    wire  io_identity = io_address < 32'h00c && !io_write;
    wire [31:0] io_bank = io_address - 32'h510;
    wire  io_display = io_address >= 32'h510 && io_address < 32'h548;
    wire [31:0] io_cursor_bank = io_address - 32'h410;
    wire  io_cursor = io_address >= 32'h410 && io_address < 32'h448;
    wire  io_station = io_address >= 32'h100 && io_address < 32'h410;
    logic [31:0] identity, display_result, cursor_result, station_result;
    logic display_error, cursor_error, station_error;

    // The cursor bitmap is scanned out at the cursor's position; its set
    // pixels (palette entry one, black) mark where the cursor covers the frame.
    logic [10:0] cursor_x, cursor_y;
    logic        cursor_visible, cursor_inside;
    logic [23:0] cursor_rgb;
    display #(.MAX_WIDTH(16), .MAX_HEIGHT(16)) cursor (
        .clk_i, .rst_i, .valid_i(io_valid && io_ready && io_cursor), .write_i(io_write),
        .offset_i(io_cursor_bank[5:0]), .data_i(io_data), .result_o(cursor_result),
        .error_o(cursor_error),
        .video_clk_i, .x_i(video_x_i - cursor_x[9:0]), .y_i(video_y_i - cursor_y[9:0]),
        .visible_o(cursor_visible), .inside_o(cursor_inside), .rgb_o(cursor_rgb));
    assign video_cursor_o = cursor_visible && cursor_inside && cursor_rgb == 24'h000000;

    // Either joystick port takes the mouse.
    logic key_valid, key_down, button_a, button_b;
    logic [7:0] key_code;
    logic [1:0] x_step, x_negative, y_step, y_negative;
    keyboard keys (.clk_i, .rst_i, .kb_clock_o, .kb_data_o, .kb_data_i, .held_o(keys_held_o),
                   .key_valid_o(key_valid), .key_code_o(key_code), .key_down_o(key_down));
    mouse mouse_a (.clk_i, .rst_i, .up_n_i(joy_a_n_i[0]), .down_n_i(joy_a_n_i[1]),
                   .left_n_i(joy_a_n_i[2]), .right_n_i(joy_a_n_i[3]), .fire_n_i(joy_a_n_i[4]),
                   .x_step_o(x_step[0]), .x_negative_o(x_negative[0]),
                   .y_step_o(y_step[0]), .y_negative_o(y_negative[0]), .button_o(button_a));
    mouse mouse_b (.clk_i, .rst_i, .up_n_i(joy_b_n_i[0]), .down_n_i(joy_b_n_i[1]),
                   .left_n_i(joy_b_n_i[2]), .right_n_i(joy_b_n_i[3]), .fire_n_i(joy_b_n_i[4]),
                   .x_step_o(x_step[1]), .x_negative_o(x_negative[1]),
                   .y_step_o(y_step[1]), .y_negative_o(y_negative[1]), .button_o(button_b));
    workstation station (
        .clk_i, .rst_i, .valid_i(io_valid && io_ready && io_station), .write_i(io_write),
        .address_i(io_address[11:0]), .data_i(io_data), .result_o(station_result),
        .error_o(station_error), .irq_o(irq),
        .x_step_i(|x_step), .x_negative_i(x_step[0] ? x_negative[0] : x_negative[1]),
        .y_step_i(|y_step), .y_negative_i(y_step[0] ? y_negative[0] : y_negative[1]),
        .key_valid_i(key_valid), .key_code_i(key_code), .key_down_i(key_down),
        .button_i(button_a || button_b), .cursor_x_o(cursor_x), .cursor_y_o(cursor_y));
    always_comb unique case (io_address[3:2])
        2'd0: identity = 32'h524b494f;       // "RKIO"
        2'd1: identity = 32'h00010001;
        default: identity = 32'h0000007f;    // everything but block storage
    endcase
    display screen (
        .clk_i, .rst_i, .valid_i(io_valid && io_ready && io_display), .write_i(io_write),
        .offset_i(io_bank[5:0]), .data_i(io_data), .result_o(display_result),
        .error_o(display_error),
        .video_clk_i, .x_i(video_x_i), .y_i(video_y_i), .visible_o(video_visible_o),
        .inside_o(video_inside_o), .rgb_o(video_rgb_o));
    assign io_ready    = !io_pending;
    assign io_response = io_pending;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            io_pending <= 1'b0;
            io_refused_o <= '0;
            for (int i = 0; i < 8; i++) debug_o[i] <= '0;
        end else if (io_valid && io_ready) begin
            if (io_display ? display_error : io_cursor ? cursor_error :
                io_station ? station_error : !(io_debug || io_identity))
                io_refused_o <= {io_write, io_address[30:0]};
            io_pending <= 1'b1;
            io_error   <= io_display ? display_error : io_cursor ? cursor_error :
                          io_station ? station_error : !(io_debug || io_identity);
            io_result  <= io_display ? display_result : io_cursor ? cursor_result :
                          io_station ? station_result : io_identity ? identity :
                          io_write ? 32'd0 : debug_o[io_address[4:2]];
            if (io_write && io_debug) debug_o[io_address[4:2]] <= io_data;
        end else if (io_response && io_response_ready) io_pending <= 1'b0;
    end

    perf_counters counters (
        .clk_i, .rst_i, .retire_i(retired && !gc_active), .dispatch_i(dispatched),
        .command_i(cmd_valid && cmd_ready), .collecting_i(gc_active),
        .store_i(store_valid && store_ready), .rows_o(perf_o));

    // The monitor's snapshot. A scan visits each register, VR and the top 16
    // entries of each stack in turn, holding each address for two cycles so
    // that a read with one cycle of latency is also captured.
    logic [5:0]  scan_slot;
    logic        scan_hold;
    logic [31:0] snap_register [16];
    logic [39:0] snap_evaluation [16], snap_vr [8];
    logic [23:0] snap_control [16];
    always_comb begin
        unique case (scan_slot[5:4])
            2'd0: scan_address = {12'd0, scan_slot[3:0]};
            2'd1: scan_address = 16'(sp - 24'(scan_slot[3:0]));
            2'd2: scan_address = 16'(csp - 24'(scan_slot[3:0]));
            default: scan_address = '0;
        endcase
    end
    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            scan_slot <= '0;
            scan_hold <= 1'b0;
        end else begin
            scan_hold <= !scan_hold;
            if (scan_hold) begin
                unique case (scan_slot[5:4])
                    2'd0: snap_register[scan_slot[3:0]] <= scan_register;
                    2'd1: snap_evaluation[scan_slot[3:0]] <= scan_evaluation;
                    2'd2: snap_control[scan_slot[3:0]] <= scan_control;
                    default: if (scan_slot[3] == 1'b0) snap_vr[scan_slot[2:0]] <= scan_vr;
                endcase
                scan_slot <= scan_slot == 6'd55 ? '0 : scan_slot + 1'b1;
            end
        end
    end
    always_comb begin
        for (int i = 0; i < 16; i++) begin
            monitor_left_o[i] = monitor_page_i ? snap_evaluation[i] : {8'd0, snap_register[i]};
            monitor_right_o[i] = monitor_page_i ? {16'd0, snap_control[i]} : '0;
        end
        if (!monitor_page_i) begin
            for (int i = 0; i < 8; i++) monitor_right_o[i] = snap_vr[i];
            monitor_right_o[8]  = symbol;
            monitor_right_o[9]  = object;
            monitor_right_o[10] = {24'd0, pc_o};
            monitor_right_o[11] = {4'd0, sp[11:0], esp[11:0], csp[11:0]};
            monitor_right_o[12] = estkr;
            monitor_right_o[13] = {16'd0, cstkr};
            monitor_right_o[14] = objekt_index;
            monitor_right_o[15] = objekt_selected;
        end
    end
endmodule

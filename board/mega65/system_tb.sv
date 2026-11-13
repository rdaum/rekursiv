// Runs rekursiv_system from build/sim/boot.mem and prints its debug display.
// Built and run by sim.sh.
module system_tb #(parameter integer PAGER_BITS = 10);
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
    rekursiv_system #(.PAGER_BITS(PAGER_BITS), .FIRMWARE("build/sim/loader.mem")) machine (
        .clk_i(clk), .rst_i(rst),
        .sd_clk_o(sd_clk), .sd_cs_n_o(sd_cs_n), .sd_mosi_o(sd_mosi), .sd_miso_i(sd_miso),
        .disk_display_o(disk), .debug_o(debug), .booted_o(booted),
        .video_clk_i(clk), .video_x_i(10'd0), .video_y_i(10'd0),
        .video_visible_o(), .video_inside_o(), .video_rgb_o(), .build_o(), .video_cursor_o(), .io_refused_o(),
        .kb_clock_o(), .kb_data_o(), .kb_data_i(1'b1), .joy_a_n_i(joy_a), .joy_b_n_i(5'b11111),
        .sdram_cke_o(m_cke), .sdram_cs_n_o(m_cs_n), .sdram_ras_n_o(m_ras_n), .sdram_cas_n_o(m_cas_n),
        .sdram_we_n_o(m_we_n), .sdram_ba_o(m_ba), .sdram_a_o(m_a), .sdram_dqml_o(), .sdram_dqmh_o(),
        .sdram_dq_o(m_dq_out), .sdram_dq_oe_o(m_dq_oe), .sdram_dq_i(m_dq_in),
        .memory_tested_o(memory_tested), .memory_failed_o(memory_failed), .memory_failed_address_o(),
        .halted_o(halted), .service_o(service), .fault_o(fault), .pc_o(pc),
        .pause_i(pause), .monitor_page_i(1'b0), .monitor_left_o(monitor_left),
        .monitor_right_o(monitor_right));
    sd_card_model #(.IMAGE("build/sim/card.img"), .SECTORS(81920)) card (
        .sd_clk_i(sd_clk), .sd_cs_n_i(sd_cs_n), .sd_mosi_i(sd_mosi), .sd_miso_o(sd_miso));
    // An Atari ST mouse on port A, {fire, right, left, down, up}. +mouse=N
    // moves it N steps along both axes from cycle 60 million, then presses
    // and releases the button.
    logic [4:0] joy_a = 5'b11111;
    integer mouse_steps = 0;
    initial begin
        void'($value$plusargs("mouse=%d", mouse_steps));
        if (mouse_steps != 0) begin
            repeat (60000000) @(posedge clk);
            for (int i = 0; i < mouse_steps; i++) begin
                repeat (20000) @(posedge clk);
                unique case (i % 4)
                    0: begin joy_a[1] = 0; joy_a[2] = 0; end
                    1: begin joy_a[0] = 0; joy_a[3] = 0; end
                    2: begin joy_a[1] = 1; joy_a[2] = 1; end
                    3: begin joy_a[0] = 1; joy_a[3] = 1; end
                endcase
            end
            repeat (1000000) @(posedge clk); joy_a[4] = 0;
            repeat (2000000) @(posedge clk); joy_a[4] = 1;
        end
    end
    // +pause=CYCLE pauses LOGIK at that cycle for 100,000 cycles. Nothing may
    // retire while it is held after the first 1,000 cycles, which allow an
    // instruction already past EXECUTE to finish; the monitor's PC row must
    // match the processor's.
    logic pause = 0;
    logic [39:0] monitor_left [16], monitor_right [16];
    longint pause_at = 0;
    initial begin
        if ($value$plusargs("pause=%d", pause_at)) begin
            logic [15:0] held_pc;
            repeat (pause_at) @(posedge clk);
            pause = 1;
            repeat (1000) @(posedge clk);
            held_pc = pc;
            for (int i = 0; i < 99000; i++) begin
                @(posedge clk);
                if (machine.retired || pc != held_pc) begin
                    $display("FAIL: retired while paused at pc %0d", held_pc);
                    $fatal(1);
                end
            end
            if (monitor_right[10] != {24'd0, pc}) begin
                $display("FAIL: monitor PC %h, processor %h", monitor_right[10], pc);
                $fatal(1);
            end
            $display("paused at pc %0d for 99,000 cycles; R0=%h VR0=%h", held_pc,
                     monitor_left[0], monitor_right[0]);
            pause = 0;
        end
    end
    // Progress, for long runs.
    longint cycle = 0;
    logic booted_q = 0, visible_q = 0;
    always @(posedge clk) begin
        cycle <= cycle + 1;
        booted_q <= booted;
        visible_q <= machine.screen.visible_o;
        if (booted && !booted_q) $display("cycle %0d: booted", cycle);
        if (machine.screen.visible_o && !visible_q)
            $display("cycle %0d: first frame published; %0d store requests", cycle, disk[6]);
        if (cycle % 10000000 == 0 && cycle != 0)
            $display("cycle %0d: pc=%0d fault=%0d store requests=%0d last=%h", cycle, pc, fault, disk[6], disk[7]);
    end
    string frame_file;
    initial begin
        repeat (4) @(posedge clk);
        rst = 0;
        if (!$value$plusargs("cycles=%d", cycles)) cycles = 2000000;
        repeat (cycles) @(posedge clk);
        $display("booted=%0d halted=%0d service=%0d fault=%0d pc=%0d memory tested=%0d failed=%0d sdram violations=%0d",
                 booted, halted, service, fault, pc, memory_tested, memory_failed, ram.errors);
        $display("disk processor trap=%0d pc=%h", machine.disk.cpu.trap, machine.disk.cpu.reg_pc);
        $display("display visible=%0d %0dx%0d stride %0d words[0,319,320,9599]=%08x %08x %08x %08x",
            machine.screen.visible_o, machine.screen.visible_width, machine.screen.visible_height,
            machine.screen.visible_stride, machine.screen.frame[0], machine.screen.frame[319],
            machine.screen.frame[320], machine.screen.frame[9599]);
        $display("mouse=(%0d,%0d) cursor=(%0d,%0d) fifo count=%0d input events=%0d irq=%0d",
            machine.station.mouse_x, machine.station.mouse_y, machine.station.cursor_x_o,
            machine.station.cursor_y_o, machine.station.count, machine.station.input_events,
            machine.station.irq_o);
        if ($value$plusargs("frame=%s", frame_file)) $writememh(frame_file, machine.screen.frame);
        for (int i = 0; i < 8; i++) $display("debug[%0d] = %08x   disk[%0d] = %08x", i, debug[i], i, disk[i]);
        $finish;
    end
endmodule

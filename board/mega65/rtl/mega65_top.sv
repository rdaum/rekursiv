// MEGA65 R6 top level for the first rekursiv bring-up. The machine runs at
// 12.6 MHz; its debug display and status are drawn at 640x480 60 Hz on the
// HDMI connector (plain DVI) and the VGA connector. Both clocks come from one
// MMCM, so their crossing is timed as a synchronous path.
//
// Mainboard LEDs: ULED blinks while configured; green once the boot stream has
// run without a fault; red on a processor fault.
//
// Port names follow the MiSTer2MEGA65 R6 pin constraint file.
module mega65_top (
    input  logic       clk_i,            // 100 MHz

    // HDMI connector
    output logic       tmds_clk_p_o,
    output logic       tmds_clk_n_o,
    output logic [2:0] tmds_data_p_o,
    output logic [2:0] tmds_data_n_o,
    output logic       hdmi_hiz_en_o,
    output logic       hdmi_ls_oe_n_o,

    // VGA connector, through the ADV7125 video DAC
    output logic [7:0] vga_red_o,
    output logic [7:0] vga_green_o,
    output logic [7:0] vga_blue_o,
    output logic       vga_hs_o,
    output logic       vga_vs_o,
    output logic       vdac_clk_o,
    output logic       vdac_sync_n_o,
    output logic       vdac_blank_n_o,
    output logic       vdac_psave_n_o,

    // Mainboard LEDs
    output logic       led_o,
    output logic       led_g_n_o,
    output logic       led_r_n_o,

    // Internal full-size SD card slot, in SPI mode (D3 is chip select)
    output logic       sd2_clk_o,
    output logic       sd2_reset_o,
    output logic       sd2_mosi_o,
    input  logic       sd2_miso_i,
    // SDRAM (IS42S16320F)
    output logic        sdram_clk_o,
    output logic        sdram_cke_o,
    output logic        sdram_cs_n_o,
    output logic        sdram_ras_n_o,
    output logic        sdram_cas_n_o,
    output logic        sdram_we_n_o,
    output logic [1:0]  sdram_ba_o,
    output logic [12:0] sdram_a_o,
    output logic        sdram_dqml_o,
    output logic        sdram_dqmh_o,
    inout  wire  [15:0] sdram_dq_io,
    // Rear microSD slot, held deselected
    output logic       sd_clk_o,
    output logic       sd_reset_o,
    output logic       sd_mosi_o,

    // Everything below is held inactive so the board's peripherals stay idle.
    output logic       f_density_o,
    output logic       f_motora_o,
    output logic       f_motorb_o,
    output logic       f_selecta_o,
    output logic       f_selectb_o,
    output logic       f_side1_o,
    output logic       f_stepdir_o,
    output logic       f_step_o,
    output logic       f_wdata_o,
    output logic       f_wgate_o,

    output logic       cart_en_o,
    output logic       cart_ctrl_en_o,
    output logic       cart_ctrl_dir_o,
    output logic       cart_data_en_o,
    output logic       cart_data_dir_o,
    output logic       cart_addr_en_o,
    output logic       cart_haddr_dir_o,
    output logic       cart_laddr_dir_o,
    output logic       cart_reset_oe_n_o,
    output logic       cart_game_oe_n_o,
    output logic       cart_exrom_oe_n_o,
    output logic       cart_nmi_oe_n_o,
    output logic       cart_irq_oe_n_o,
    output logic       cart_roml_oe_n_o,
    output logic       cart_romh_oe_n_o,

    output logic       iec_reset_n_o,
    output logic       iec_atn_n_o,
    output logic       iec_clk_en_n_o,
    output logic       iec_clk_n_o,
    output logic       iec_data_en_n_o,
    output logic       iec_data_n_o,
    output logic       iec_srq_en_n_o,
    output logic       iec_srq_n_o,

    output logic       eth_reset_o,
    output logic       eth_txen_o,
    output logic       hr_cs0_o,
    output logic       hr_reset_o,
    output logic       qspicsn_o,
    output logic       audio_pdn_n_o,
    output logic       joystick_5v_disable_o,
    output logic       pmod1_en_o,
    output logic       pmod2_en_o
);
    // ---------------------------------------------------------------------
    // Idle levels, matching the MiSTer2MEGA65 R6 top level.
    // ---------------------------------------------------------------------
    assign {f_density_o, f_motora_o, f_motorb_o, f_selecta_o, f_selectb_o,
            f_side1_o, f_stepdir_o, f_step_o, f_wdata_o, f_wgate_o} = '1;

    assign cart_en_o = 1'b0;
    assign {cart_ctrl_en_o, cart_data_en_o, cart_addr_en_o} = '1;
    assign {cart_ctrl_dir_o, cart_data_dir_o, cart_haddr_dir_o, cart_laddr_dir_o} = '0;
    assign {cart_reset_oe_n_o, cart_game_oe_n_o, cart_exrom_oe_n_o, cart_nmi_oe_n_o,
            cart_irq_oe_n_o, cart_roml_oe_n_o, cart_romh_oe_n_o} = '1;

    assign {iec_reset_n_o, iec_atn_n_o, iec_clk_n_o, iec_data_n_o, iec_srq_n_o} = '1;
    assign {iec_clk_en_n_o, iec_data_en_n_o, iec_srq_en_n_o} = '1;

    assign eth_reset_o           = 1'b1;
    assign eth_txen_o            = 1'b0;
    assign hr_cs0_o              = 1'b1;
    assign hr_reset_o            = 1'b1;
    assign qspicsn_o             = 1'b1;
    assign audio_pdn_n_o         = 1'b0;
    assign joystick_5v_disable_o = 1'b0;
    assign pmod1_en_o            = 1'b0;
    assign pmod2_en_o            = 1'b0;

    // ---------------------------------------------------------------------
    // Clocks: VCO = 100 / 5 * 31.5 = 630 MHz.
    //   CLKOUT0 126 MHz  TMDS serialiser (5x pixel)
    //   CLKOUT1 25.2 MHz pixel clock
    //   CLKOUT2 12.6 MHz machine clock
    // ---------------------------------------------------------------------
    logic clk_pix, clk_x5, clk_cpu, clk_pix_raw, clk_x5_raw, clk_cpu_raw, clk_fb, locked;

    MMCME2_BASE #(
        .CLKIN1_PERIOD   (10.0),
        .DIVCLK_DIVIDE   (5),
        .CLKFBOUT_MULT_F (31.5),
        .CLKOUT0_DIVIDE_F(5.0),
        .CLKOUT1_DIVIDE  (25),
        .CLKOUT2_DIVIDE  (50)
    ) mmcm (
        .CLKIN1(clk_i), .CLKFBIN(clk_fb), .CLKFBOUT(clk_fb), .CLKFBOUTB(),
        .CLKOUT0(clk_x5_raw), .CLKOUT0B(), .CLKOUT1(clk_pix_raw), .CLKOUT1B(),
        .CLKOUT2(clk_cpu_raw), .CLKOUT2B(), .CLKOUT3(), .CLKOUT3B(),
        .CLKOUT4(), .CLKOUT5(), .CLKOUT6(),
        .LOCKED(locked), .PWRDWN(1'b0), .RST(1'b0)
    );

    BUFG bufg_x5  (.I(clk_x5_raw),  .O(clk_x5));
    BUFG bufg_pix (.I(clk_pix_raw), .O(clk_pix));
    BUFG bufg_cpu (.I(clk_cpu_raw), .O(clk_cpu));

    logic [7:0] rst_pix_shift = '1, rst_cpu_shift = '1;
    always_ff @(posedge clk_pix or negedge locked)
        if (!locked) rst_pix_shift <= '1;
        else         rst_pix_shift <= {rst_pix_shift[6:0], 1'b0};
    always_ff @(posedge clk_cpu or negedge locked)
        if (!locked) rst_cpu_shift <= '1;
        else         rst_cpu_shift <= {rst_cpu_shift[6:0], 1'b0};
    wire rst = rst_pix_shift[7];
    wire rst_cpu = rst_cpu_shift[7];

    // ---------------------------------------------------------------------
    // The machine
    // ---------------------------------------------------------------------
    logic [31:0] debug [8];
    logic        booted, halted, service;
    logic [3:0]  fault;
    logic [15:0] pc;

    logic [31:0] disk_rows [8];
    logic [15:0] sdram_dq_out;
    logic        sdram_dq_oe, memory_tested, memory_failed;
    logic [22:0] memory_failed_address;
    assign sdram_dq_io = sdram_dq_oe ? sdram_dq_out : 16'bz;
    // The chip samples mid-cycle: send it the inverted machine clock.
    ODDR #(.DDR_CLK_EDGE("SAME_EDGE")) sdram_clk_fwd (
        .C(clk_cpu), .CE(1'b1), .D1(1'b0), .D2(1'b1), .R(1'b0), .S(1'b0), .Q(sdram_clk_o)
    );
    rekursiv_system #(.FIRMWARE("loader.mem")) machine (
        .clk_i(clk_cpu), .rst_i(rst_cpu),
        .sd_clk_o(sd2_clk_o), .sd_cs_n_o(sd2_reset_o), .sd_mosi_o(sd2_mosi_o),
        .sd_miso_i(sd2_miso_i), .disk_display_o(disk_rows),
        .sdram_cke_o, .sdram_cs_n_o, .sdram_ras_n_o, .sdram_cas_n_o, .sdram_we_n_o,
        .sdram_ba_o, .sdram_a_o, .sdram_dqml_o, .sdram_dqmh_o,
        .sdram_dq_o(sdram_dq_out), .sdram_dq_oe_o(sdram_dq_oe), .sdram_dq_i(sdram_dq_io),
        .memory_tested_o(memory_tested), .memory_failed_o(memory_failed),
        .memory_failed_address_o(memory_failed_address),
        .debug_o(debug), .booted_o(booted), .halted_o(halted), .service_o(service),
        .fault_o(fault), .pc_o(pc)
    );

    logic [26:0] blink = '0;
    always_ff @(posedge clk_i) blink <= blink + 1'b1;
    assign led_o     = blink[26];
    assign led_g_n_o = !(booted && fault == 0);
    assign led_r_n_o = fault == 0 && !memory_failed;

    // ---------------------------------------------------------------------
    // 640x480 at 60 Hz: 800 x 525 total, both syncs active low.
    // ---------------------------------------------------------------------
    localparam int H_ACTIVE = 640, H_FRONT = 16, H_SYNC = 96, H_TOTAL = 800;
    localparam int V_ACTIVE = 480, V_FRONT = 10, V_SYNC = 2,  V_TOTAL = 525;

    logic [9:0] x = '0, y = '0;
    always_ff @(posedge clk_pix) begin
        if (x == H_TOTAL - 1) begin
            x <= '0;
            y <= (y == V_TOTAL - 1) ? '0 : y + 1'b1;
        end else begin
            x <= x + 1'b1;
        end
    end

    // Rows 0-7: the debug display. Row 8: PC, fault, memory check, halted, booted.
    logic [31:0] rows [9];
    always_comb begin
        for (int i = 0; i < 8; i++) rows[i] = debug[i];
        // Memory check: 1 passed, e failed (rows 6 shows the failing address).
        rows[8] = {pc, fault, memory_failed ? 4'he : {3'b0, memory_tested}, 3'b0, halted, 3'b0, booted};
        if (memory_failed) rows[6] = {9'd0, memory_failed_address};
    end

    // The disk processor's display, in a second panel on the right.
    assign sd_clk_o   = 1'b0;
    assign sd_reset_o = 1'b1;
    assign sd_mosi_o  = 1'b1;

    // Sample both panels' values once per frame, during vertical blanking,
    // so a value that changes mid-frame is not drawn half old, half new.
    logic [31:0] rows_shown [9], disk_shown [8];
    always_ff @(posedge clk_pix) if (x == 0 && y == V_ACTIVE) begin
        rows_shown <= rows;
        disk_shown <= disk_rows;
    end

    logic ink, panel, sd_ink, sd_panel;
    hex_display #(.ROWS(9), .LEFT(0)) text (.clk_i(clk_pix), .x_i(x), .y_i(y), .values_i(rows_shown),
                                            .ink_o(ink), .panel_o(panel));
    hex_display #(.ROWS(8), .LEFT(320)) sd_text (.clk_i(clk_pix), .x_i(x), .y_i(y),
                                                 .values_i(disk_shown), .ink_o(sd_ink),
                                                 .panel_o(sd_panel));

    // Match the display's one-cycle latency for position and blanking.
    logic [9:0] x_d, y_d;
    always_ff @(posedge clk_pix) begin x_d <= x; y_d <= y; end

    logic       active;
    logic [7:0] r, g, b;
    always_comb begin
        active = (x_d < H_ACTIVE) && (y_d < V_ACTIVE);
        if (!active)        {r, g, b} = '0;
        else if (ink || sd_ink) {r, g, b} = 24'hFFFFFF;
        else if (panel)     {r, g, b} = fault != 0 ? 24'h800000 : 24'h102060;
        else if (sd_panel)  {r, g, b} = 24'h104020;
        else                {r, g, b} = 24'h000000;
    end

    logic [7:0] r_q, g_q, b_q;
    logic       hs_q, vs_q, de_q;
    always_ff @(posedge clk_pix) begin
        {r_q, g_q, b_q} <= {r, g, b};
        de_q <= active;
        hs_q <= ~((x_d >= H_ACTIVE + H_FRONT) && (x_d < H_ACTIVE + H_FRONT + H_SYNC));
        vs_q <= ~((y_d >= V_ACTIVE + V_FRONT) && (y_d < V_ACTIVE + V_FRONT + V_SYNC));
    end

    // ---------------------------------------------------------------------
    // VGA through the ADV7125: inverted pixel clock puts its sampling edge
    // mid-pixel.
    // ---------------------------------------------------------------------
    assign vga_red_o      = r_q;
    assign vga_green_o    = g_q;
    assign vga_blue_o     = b_q;
    assign vga_hs_o       = hs_q;
    assign vga_vs_o       = vs_q;
    assign vdac_sync_n_o  = 1'b0;
    assign vdac_blank_n_o = 1'b1;
    assign vdac_psave_n_o = 1'b1;

    ODDR #(.DDR_CLK_EDGE("SAME_EDGE")) vdac_clk_fwd (
        .C(clk_pix), .CE(1'b1), .D1(1'b0), .D2(1'b1), .R(1'b0), .S(1'b0), .Q(vdac_clk_o)
    );

    // ---------------------------------------------------------------------
    // DVI on the HDMI connector. Channel 0 = blue + syncs, 1 = green, 2 = red.
    // ---------------------------------------------------------------------
    assign hdmi_hiz_en_o  = 1'b0;
    assign hdmi_ls_oe_n_o = 1'b0;

    logic [9:0] tmds [3];
    tmds_encoder enc_b (.clk(clk_pix), .d(b_q), .c({vs_q, hs_q}), .de(de_q), .q(tmds[0]));
    tmds_encoder enc_g (.clk(clk_pix), .d(g_q), .c(2'b00),        .de(de_q), .q(tmds[1]));
    tmds_encoder enc_r (.clk(clk_pix), .d(r_q), .c(2'b00),        .de(de_q), .q(tmds[2]));

    for (genvar i = 0; i < 3; i++) begin : g_data
        tmds_serialiser ser (
            .clk_pix, .clk_x5, .rst, .d(tmds[i]),
            .tmds_p(tmds_data_p_o[i]), .tmds_n(tmds_data_n_o[i])
        );
    end
    tmds_serialiser ser_clk (
        .clk_pix, .clk_x5, .rst, .d(10'b0000011111),
        .tmds_p(tmds_clk_p_o), .tmds_n(tmds_clk_n_o)
    );
endmodule

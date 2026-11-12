// 10:1 serialiser for one TMDS pair, LSB first, using a cascaded pair of
// Xilinx 7-series OSERDESE2 cells in DDR mode. clk_x5 must be five times
// clk_pix and phase aligned with it (same MMCM).
module tmds_serialiser (
    input  logic       clk_pix,
    input  logic       clk_x5,
    input  logic       rst,
    input  logic [9:0] d,
    output logic       tmds_p,
    output logic       tmds_n
);
    logic serial, shift1, shift2;

    OSERDESE2 #(
        .DATA_RATE_OQ("DDR"), .DATA_RATE_TQ("SDR"), .DATA_WIDTH(10),
        .SERDES_MODE("MASTER"), .TRISTATE_WIDTH(1)
    ) master (
        .CLK(clk_x5), .CLKDIV(clk_pix), .RST(rst), .OCE(1'b1),
        .D1(d[0]), .D2(d[1]), .D3(d[2]), .D4(d[3]),
        .D5(d[4]), .D6(d[5]), .D7(d[6]), .D8(d[7]),
        .SHIFTIN1(shift1), .SHIFTIN2(shift2), .SHIFTOUT1(), .SHIFTOUT2(),
        .OQ(serial), .OFB(), .TQ(), .TFB(), .TBYTEOUT(),
        .T1(1'b0), .T2(1'b0), .T3(1'b0), .T4(1'b0), .TCE(1'b0),
        .TBYTEIN(1'b0)
    );

    OSERDESE2 #(
        .DATA_RATE_OQ("DDR"), .DATA_RATE_TQ("SDR"), .DATA_WIDTH(10),
        .SERDES_MODE("SLAVE"), .TRISTATE_WIDTH(1)
    ) slave (
        .CLK(clk_x5), .CLKDIV(clk_pix), .RST(rst), .OCE(1'b1),
        .D1(1'b0), .D2(1'b0), .D3(d[8]), .D4(d[9]),
        .D5(1'b0), .D6(1'b0), .D7(1'b0), .D8(1'b0),
        .SHIFTIN1(1'b0), .SHIFTIN2(1'b0), .SHIFTOUT1(shift1), .SHIFTOUT2(shift2),
        .OQ(), .OFB(), .TQ(), .TFB(), .TBYTEOUT(),
        .T1(1'b0), .T2(1'b0), .T3(1'b0), .T4(1'b0), .TCE(1'b0),
        .TBYTEIN(1'b0)
    );

    OBUFDS #(.IOSTANDARD("TMDS_33")) buffer (.I(serial), .O(tmds_p), .OB(tmds_n));
endmodule

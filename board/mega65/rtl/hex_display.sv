// Draws rows of 32-bit values in hexadecimal on a 640x480 raster: a row
// label digit, a gap, then eight digits. Glyphs are 5x7 in an 8x8 cell,
// scaled four times. One pixel per clock; output is registered once.
module hex_display #(
    parameter integer ROWS = 9
)(
    input  logic        clk_i,
    input  logic [9:0]  x_i,
    input  logic [9:0]  y_i,
    input  logic [31:0] values_i [ROWS],
    output logic        ink_o,
    output logic        panel_o
);
    localparam integer LEFT = 64, TOP = 48, CELL = 32, PITCH = 40;

    function automatic logic [34:0] glyph(input logic [3:0] digit);
        unique case (digit)
            4'h0: glyph = 35'b01110_10001_10011_10101_11001_10001_01110;
            4'h1: glyph = 35'b00100_01100_00100_00100_00100_00100_01110;
            4'h2: glyph = 35'b01110_10001_00001_00010_00100_01000_11111;
            4'h3: glyph = 35'b11111_00010_00100_00010_00001_10001_01110;
            4'h4: glyph = 35'b00010_00110_01010_10010_11111_00010_00010;
            4'h5: glyph = 35'b11111_10000_11110_00001_00001_10001_01110;
            4'h6: glyph = 35'b00110_01000_10000_11110_10001_10001_01110;
            4'h7: glyph = 35'b11111_00001_00010_00100_01000_01000_01000;
            4'h8: glyph = 35'b01110_10001_10001_01110_10001_10001_01110;
            4'h9: glyph = 35'b01110_10001_10001_01111_00001_00010_01100;
            4'ha: glyph = 35'b01110_10001_10001_11111_10001_10001_10001;
            4'hb: glyph = 35'b11110_10001_10001_11110_10001_10001_11110;
            4'hc: glyph = 35'b01110_10001_10000_10000_10000_10001_01110;
            4'hd: glyph = 35'b11100_10010_10001_10001_10001_10010_11100;
            4'he: glyph = 35'b11111_10000_10000_11110_10000_10000_11111;
            4'hf: glyph = 35'b11111_10000_10000_11110_10000_10000_10000;
        endcase
    endfunction

    logic [9:0] rx, ry;
    logic [3:0] column, row;
    logic [2:0] gx, gy;
    logic       in_panel, in_cell;
    logic [3:0] digit;
    logic [34:0] bits;

    always_comb begin
        rx = x_i - 10'(LEFT);
        ry = y_i - 10'(TOP);
        column = 4'(rx / CELL);
        row = 4'(ry / PITCH);
        gx = 3'((rx % CELL) / 4);
        gy = 3'((ry % PITCH) / 4);
        in_panel = x_i >= LEFT && x_i < LEFT + 10 * CELL && y_i >= TOP && y_i < TOP + ROWS * PITCH;
        in_cell = in_panel && (ry % PITCH) < CELL && column != 1 && gx < 5 && gy < 7;
        digit = column == 0 ? row : values_i[row < ROWS ? row : 0][4 * (9 - column) +: 4];
        bits = glyph(digit);
    end

    always_ff @(posedge clk_i) begin
        panel_o <= in_panel;
        ink_o <= in_cell && bits[34 - (5 * gy + gx)];
    end
endmodule

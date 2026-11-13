// The display device: the bitmap bank of docs/devices.md at one bit per
// pixel, with its frame in block RAM and a read port for video scan-out.
//
// A register access completes in the cycle it is presented. Offsets are in
// bytes from the bank base.
//
// Differences from the device contract:
// - Depth is 1 only. Writing another depth is an error.
// - Uploaded words go straight to the visible frame; Publish only makes the
//   new geometry visible. An upload is therefore not atomic on screen, and an
//   abandoned upload leaves its words behind.
// - A full replacement counts its writes instead of tracking each word.
// - Palette entries 0 and 1 are kept; writes to other entries are accepted
//   and dropped. Cursor origin and composition writes are accepted and unused.
//
// The video side runs on its own clock, which must be synchronous to clk_i.
// A raster position gets its pixel one video clock later.
module display #(
    parameter integer MAX_WIDTH = 640,
    parameter integer MAX_HEIGHT = 480
)(
    input  logic        clk_i,
    input  logic        rst_i,
    input  logic        valid_i,
    input  logic        write_i,
    input  logic [5:0]  offset_i,
    input  logic [31:0] data_i,
    output logic [31:0] result_o,
    output logic        error_o,

    input  logic        video_clk_i,
    input  logic [9:0]  x_i,
    input  logic [9:0]  y_i,
    output logic        visible_o,   // a frame has been published
    output logic        inside_o,    // the position is within the frame
    output logic [23:0] rgb_o
);
    localparam integer MAX_STRIDE = (MAX_WIDTH + 31) / 32;
    localparam integer WORDS = MAX_HEIGHT * MAX_STRIDE;
    localparam integer AW = $clog2(WORDS);

    logic [31:0] frame [0:WORDS-1];

    logic [10:0] width, height, visible_width, visible_height, upload_width, upload_height;
    logic [5:0]  stride, visible_stride, upload_stride;
    logic        uploading, patch;
    logic [AW:0] upload_offset, upload_length, upload_written;
    logic [8:0]  palette_index;
    logic [23:0] palette [2];

    wire [5:0]  least_stride = 6'((width + 11'd31) >> 5);
    wire        geometry_ok = width != 0 && width <= 11'(MAX_WIDTH) && height != 0 &&
        height <= 11'(MAX_HEIGHT) && stride >= least_stride && stride <= 6'(MAX_STRIDE);
    wire        same_geometry = visible_o && visible_width == width && visible_height == height &&
        visible_stride == stride;
    wire        complete = uploading && (patch || upload_written == upload_length);
    wire        store = valid_i && write_i && offset_i == 6'h1c && uploading &&
        upload_offset < upload_length;

    always_comb begin
        result_o = '0;
        error_o = 1'b0;
        if (!write_i) begin
            unique case (offset_i)
                6'h00: result_o = 32'(MAX_WIDTH);
                6'h04: result_o = 32'(MAX_HEIGHT);
                6'h08: result_o = visible_o ? 32'(visible_width) : '0;
                6'h0c: result_o = visible_o ? 32'(visible_height) : '0;
                6'h10: result_o = visible_o ? 32'(visible_stride) : '0;
                6'h20: result_o = 32'd1;
                default: error_o = 1'b1;
            endcase
        end else begin
            unique case (offset_i)
                6'h08, 6'h0c, 6'h10, 6'h2c, 6'h30: error_o = 1'b0;
                6'h14: error_o = !(data_i == 1 ? complete :
                    (data_i == 0 || (data_i == 2 && same_geometry)) && geometry_ok);
                6'h18: error_o = !(uploading && data_i < 32'(upload_length));
                6'h1c: error_o = !(uploading && upload_offset < upload_length);
                6'h20: error_o = data_i != 1;
                6'h24: error_o = data_i >= 256;
                6'h28: error_o = palette_index >= 256;
                6'h34: error_o = data_i > 1;
                default: error_o = 1'b1;
            endcase
        end
    end

    always_ff @(posedge clk_i) if (store) frame[upload_offset[AW-1:0]] <= data_i;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            width <= '0; height <= '0; stride <= '0;
            visible_o <= 1'b0; visible_width <= '0; visible_height <= '0; visible_stride <= '0;
            uploading <= 1'b0; patch <= 1'b0;
            upload_width <= '0; upload_height <= '0; upload_stride <= '0;
            upload_offset <= '0; upload_length <= '0; upload_written <= '0;
            palette_index <= '0;
            palette[0] <= 24'hffffff;
            palette[1] <= 24'h000000;
        end else if (valid_i && write_i && !error_o) begin
            unique case (offset_i)
                6'h08: width <= data_i > 2047 ? 11'd2047 : data_i[10:0];
                6'h0c: height <= data_i > 2047 ? 11'd2047 : data_i[10:0];
                6'h10: stride <= data_i > 63 ? 6'd63 : data_i[5:0];
                6'h14: if (data_i == 1) begin
                    visible_o <= 1'b1;
                    visible_width <= upload_width;
                    visible_height <= upload_height;
                    visible_stride <= upload_stride;
                    uploading <= 1'b0;
                end else begin
                    uploading <= 1'b1;
                    patch <= data_i == 2;
                    upload_width <= width; upload_height <= height; upload_stride <= stride;
                    upload_offset <= '0; upload_written <= '0;
                    upload_length <= (AW+1)'(height * stride);
                end
                6'h18: upload_offset <= data_i[AW:0];
                6'h1c: begin
                    upload_offset <= upload_offset + 1'b1;
                    if (upload_written != upload_length) upload_written <= upload_written + 1'b1;
                end
                6'h24: palette_index <= data_i[8:0];
                6'h28: begin
                    if (palette_index < 2) palette[palette_index[0]] <= data_i[23:0];
                    palette_index <= palette_index + 1'b1;
                end
                default: begin end
            endcase
        end
    end

    // Scan-out. Pixels are most significant bit first within a word.
    logic [31:0] word;
    logic [4:0]  bit_q;
    wire  [AW:0] address = (AW+1)'(y_i[8:0] * visible_stride) + (AW+1)'(x_i[9:5]);
    always_ff @(posedge video_clk_i) begin
        word <= frame[address[AW-1:0]];
        bit_q <= x_i[4:0];
        inside_o <= visible_o && {1'b0, x_i} < visible_width && {1'b0, y_i} < visible_height;
    end
    assign rgb_o = palette[word[5'd31 - bit_q]];
endmodule

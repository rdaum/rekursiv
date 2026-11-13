// The MEGA65's keyboard. Its controller sends the state of every key as a
// serial frame of 128 bits, about 2,400 times a second, on three wires: a
// clock and LED data from the FPGA, key data to it. This follows the protocol
// of mega65kbd_to_matrix.vhdl in the MEGA65 core (Paul Gardner-Stephen) as
// carried by MiSTer2MEGA65.
//
// Key bits are low while a key is down. Bits 0-71 are the C65 key matrix in
// column order; 72 is CAPS LOCK, 73 cursor up, 74 cursor left, 75 RESTORE,
// 76 INST/DEL, 77 RETURN, 78 the fast key. Matrix bits 0 and 1 arrive late
// and are taken from 76 and 77 instead.
//
// Every sixteenth frame is sampled, about 7 ms apart. A key changes state
// when two successive samples agree, which removes contact bounce. Each
// change is reported for one cycle as a device key code and a direction. The
// codes are those of the emulator's US physical keyboard profile, which the
// guest's input driver expects: unshifted ASCII, 136/137 shift, 138 control,
// 139 caps lock. Keys of the Commodore layout without a US counterpart take
// the nearest US key. ALT and the MEGA key stand in for the yellow and blue
// mouse buttons (129 and 128), which a two-button mouse lacks.
module keyboard #(
    parameter integer CLK_HZ = 25_200_000
)(
    input  logic        clk_i,
    input  logic        rst_i,
    output logic        kb_clock_o,
    output logic        kb_data_o,
    input  logic        kb_data_i,
    output logic        key_valid_o,
    output logic [7:0]  key_code_o,
    output logic        key_down_o
);
    // The serial clock toggles 312,500 times a second.
    localparam integer DIVIDE = CLK_HZ / 2 / 312_500;

    function automatic logic [7:0] code(input logic [6:0] key);
        unique case (key)
            7'd0: code = 8'd8;     7'd1: code = 8'd13;
            7'd8: code = "3";      7'd9: code = "w";      7'd10: code = "a";    7'd11: code = "4";
            7'd12: code = "z";     7'd13: code = "s";     7'd14: code = "e";    7'd15: code = 8'd136;
            7'd16: code = "5";     7'd17: code = "r";     7'd18: code = "d";    7'd19: code = "6";
            7'd20: code = "c";     7'd21: code = "f";     7'd22: code = "t";    7'd23: code = "x";
            7'd24: code = "7";     7'd25: code = "y";     7'd26: code = "g";    7'd27: code = "8";
            7'd28: code = "b";     7'd29: code = "h";     7'd30: code = "u";    7'd31: code = "v";
            7'd32: code = "9";     7'd33: code = "i";     7'd34: code = "j";    7'd35: code = "0";
            7'd36: code = "m";     7'd37: code = "k";     7'd38: code = "o";    7'd39: code = "n";
            7'd40: code = "=";     7'd41: code = "p";     7'd42: code = "l";    7'd43: code = "-";
            7'd44: code = ".";     7'd45: code = "'";     7'd46: code = "[";    7'd47: code = ",";
            7'd48: code = "`";     7'd49: code = "]";     7'd50: code = ";";    7'd52: code = 8'd137;
            7'd53: code = 8'd92;   7'd55: code = "/";
            7'd56: code = "1";     7'd58: code = 8'd138;  7'd59: code = "2";    7'd60: code = " ";
            7'd61: code = 8'd128;  7'd62: code = "q";
            7'd65: code = 8'd9;    7'd66: code = 8'd129;  7'd71: code = 8'd27;
            7'd72: code = 8'd139;
            default: code = 8'd0;
        endcase
    endfunction

    logic [7:0]  divider;
    logic        clock, sync;
    logic [7:0]  phase;
    logic [3:0]  frame;
    logic [79:0] reported, candidate;
    logic        data_q, data_s;

    // Bits 76 and 77 are the true INST/DEL and RETURN.
    wire  [6:0]  key = phase == 8'd76 ? 7'd0 : phase == 8'd77 ? 7'd1 : phase[6:0];
    wire         sampled = clock && phase < 8'd80 && phase > 8'd1 && frame == 0 &&
                           divider == 8'(DIVIDE);
    wire         down = !data_s;

    always_ff @(posedge clk_i) begin
        data_q <= kb_data_i;
        data_s <= data_q;
        key_valid_o <= 1'b0;
        if (rst_i) begin
            divider <= '0; clock <= 1'b0; sync <= 1'b0; phase <= '0; frame <= '0;
            reported <= '0; candidate <= '0;
            kb_clock_o <= 1'b1; kb_data_o <= 1'b0;
        end else if (divider != 8'(DIVIDE)) divider <= divider + 1'b1;
        else begin
            divider <= '0;
            clock <= !clock;
            kb_clock_o <= clock || sync;
            if (sampled) begin
                candidate[key] <= down;
                if (down != reported[key] && candidate[key] == down) begin
                    reported[key] <= down;
                    key_valid_o <= code(key) != 0;
                    key_code_o <= code(key);
                    key_down_o <= down;
                end
            end
            if (!clock) begin
                phase <= phase == 8'd140 ? '0 : phase + 1'b1;
                if (phase == 8'd127) begin sync <= 1'b1; frame <= frame + 1'b1; end
                if (phase == 8'd140) sync <= 1'b0;
                // No LEDs are lit: every output bit is zero.
                kb_data_o <= 1'b0;
            end
        end
    end
endmodule

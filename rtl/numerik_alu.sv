// Combinational integer ALU. The parent NUMERIK module owns all registers.
//
// F is the unshifted arithmetic result; Y is the destination-shifted result.
// Flags always describe F. This distinction prevents an output shift from
// accidentally changing the condition used by the next instruction.
module numerik_alu (
    input logic [3:0] op_i,
    input logic [31:0] r_i,
    input logic [31:0] s_i,
    input logic carry_i,
    input logic [1:0] shift_i,
    input logic [63:0] product_i,
    output logic [31:0] y_o,
    output logic [4:0] flags_o,
    output logic [63:0] product_o
);
    `include "rekursiv_control.svh"
    logic [32:0] wide;
    logic [31:0] f;
    arithmetic_flags_t flags;

    // The extra bit preserves carry-out before narrowing to F. Subtraction is
    // A + ~B + CIN: CIN=1 means ordinary subtraction (book p. 148), and carry-out
    // means "no borrow". Signed overflow is independent of unsigned carry.
    always_comb begin
        wide = '0;
        flags = '0;
        product_o = product_i;
        unique case (op_i)
            ALU_ADD: begin
                wide = {1'b0, r_i} + {1'b0, s_i} + {32'b0, carry_i};
                flags.overflow = ~(r_i[31] ^ s_i[31]) & (wide[31] ^ r_i[31]);
            end
            ALU_SUB: begin
                wide = {1'b0, r_i} + {1'b0, ~s_i} + {32'b0, carry_i};
                flags.overflow = (r_i[31] ^ s_i[31]) & (wide[31] ^ r_i[31]);
            end
            ALU_SUB_REVERSE: begin
                wide = {1'b0, s_i} + {1'b0, ~r_i} + {32'b0, carry_i};
                flags.overflow = (s_i[31] ^ r_i[31]) & (wide[31] ^ s_i[31]);
            end
            ALU_AND: wide = {1'b0, r_i & s_i};
            ALU_OR: wide = {1'b0, r_i | s_i};
            ALU_XOR: wide = {1'b0, r_i ^ s_i};
            ALU_NOT: wide = {1'b0, ~r_i};
            // A barrel rotation is not a logical shift. Book p. 151 selects a
            // distance modulo 32. A distance of zero must return R unchanged.
            ALU_ROTATE: wide = {1'b0, (r_i << s_i[4:0]) |
                (r_i >> (6'd32 - {1'b0, s_i[4:0]}))};
            // Multiplication retains all 64 bits; its low word also supplies F.
            // The parent latches product_o only on instruction retirement.
            ALU_MULTIPLY_SIGNED: begin
                product_o = $signed(r_i) * $signed(s_i);
                wide = {1'b0, product_o[31:0]};
            end
            ALU_MULTIPLY_UNSIGNED: begin
                product_o = r_i * s_i;
                wide = {1'b0, product_o[31:0]};
            end
            ALU_PRODUCT_HIGH: wide = {1'b0, product_i[63:32]};
            ALU_PRODUCT_LOW: wide = {1'b0, product_i[31:0]};
            default: wide = {1'b0, r_i};
        endcase
        f = wide[31:0];
        flags.zero = f == 0;
        flags.sign = f[31];
        flags.carry = wide[32];
        flags.corrected_sign = f[31] ^ flags.overflow;

        // Project policy: destination shifts do not alter F's flags. The book
        // distinguishes destinations, but does not fully establish flag timing.
        unique case (shift_i)
            SHIFT_LEFT: y_o = f << 1;
            SHIFT_RIGHT: y_o = f >> 1;
            SHIFT_ARITHMETIC_RIGHT: y_o = {f[31], f[31:1]};
            default: y_o = f;
        endcase
    end
    assign flags_o = flags;
endmodule

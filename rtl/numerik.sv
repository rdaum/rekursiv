// NUMERIK integer datapath and architectural arithmetic state.
//
// Book pp. 144-149 establish sixteen 32-bit registers, F/Y/Q separation,
// carry-input selection, and a retained 64-bit product. This module implements
// that useful subset. BCD, division microsteps, rounded multiplication, and
// priority normalization are not silently approximated here.
//
// Normal arithmetic state changes only at retire_i. Collector entry saves a
// separate context; collector exit restores it without retiring the failed
// mutator instruction. In particular, an OBJEKT stall must
// not repeat a register write or replace the product. Condition selection in
// LOGIK sees flags from the previous retired instruction, even when this
// instruction writes flags. The independent Rust model checks that policy.
module numerik (
    input logic clk_i,
    input logic rst_i,
    input logic retire_i, input logic save_i, restore_i,
    input logic [3:0] operation_i,
    input logic [3:0] ra_i,
    input logic [3:0] rb_i,
    input logic [2:0] r_source_i,
    input logic [2:0] s_source_i,
    input logic [1:0] carry_source_i,
    input logic [1:0] shift_i,
    input logic write_register_i,
    input logic load_q_i,
    input logic write_flags_i,
    input logic [31:0] bus_i,
    input logic [31:0] estkr_i,
    input logic [15:0] branch_i,
    output logic [31:0] y_o,
    output logic [31:0] register_a_o,
    output logic [31:0] q_o,
    output logic [63:0] product_o,
    output logic [4:0] flags_o,
    input logic [3:0] debug_register_i,
    output logic [31:0] debug_register_o
);
    `include "rekursiv_control.svh"

    logic [31:0] registers [0:15], saved_registers [0:15];
    logic [31:0] saved_q;
    logic [63:0] saved_product;
    logic [4:0] saved_flags;
    logic [31:0] q;
    logic [63:0] product, calculated_product;
    arithmetic_flags_t flags, calculated_flags;
    logic [31:0] r_operand, s_operand;
    logic carry_input;

    // Both register reads use pre-retirement state. Register B is also the
    // optional write destination, so read/modify/write needs no bypass path.
    always_comb begin
        unique case (r_source_i)
            SOURCE_BUS: r_operand = bus_i;
            SOURCE_ESTK: r_operand = estkr_i;
            SOURCE_Q: r_operand = q;
            SOURCE_BRANCH: r_operand = {{16{branch_i[15]}}, branch_i};
            default: r_operand = registers[ra_i];
        endcase
        unique case (s_source_i)
            SOURCE_BUS: s_operand = bus_i;
            SOURCE_ESTK: s_operand = estkr_i;
            SOURCE_Q: s_operand = q;
            SOURCE_BRANCH: s_operand = {{16{branch_i[15]}}, branch_i};
            default: s_operand = registers[rb_i];
        endcase
        carry_input = (carry_source_i == CARRY_ONE) ||
            ((carry_source_i == CARRY_ZERO_FLAG) && flags.zero);
    end

    numerik_alu alu (
        .op_i(operation_i),
        .r_i(r_operand),
        .s_i(s_operand),
        .carry_i(carry_input),
        .shift_i(shift_i),
        .product_i(product),
        .y_o(y_o),
        .flags_o(calculated_flags),
        .product_o(calculated_product)
    );

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            q <= '0; saved_q <= '0; saved_product <= '0; saved_flags <= '0;
            product <= '0;
            flags <= '0;
            for (int index = 0; index < 16; index++) begin
                registers[index] <= '0; saved_registers[index] <= '0;
            end
        end else if(save_i) begin
            for(int index=0;index<16;index++) saved_registers[index]<=registers[index];
            saved_q<=q; saved_product<=product; saved_flags<=flags;
        end else if(restore_i) begin
            for(int index=0;index<16;index++) registers[index]<=saved_registers[index];
            q<=saved_q; product<=saved_product; flags<=saved_flags;
        end else if (retire_i) begin
            if (write_register_i) registers[rb_i] <= y_o;
            if (load_q_i) q <= y_o;
            if (write_flags_i) flags <= calculated_flags;
            product <= calculated_product;
        end
    end

    assign register_a_o = registers[ra_i];
    assign debug_register_o = registers[debug_register_i];
    assign q_o = q;
    assign product_o = product;
    assign flags_o = flags;
endmodule

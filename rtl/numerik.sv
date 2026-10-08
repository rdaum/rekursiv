// NUMERIK integer and floating-point datapaths and architectural state.
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
    input logic fp_start_i, input logic fp64_i, output logic fp_done_o,
    input logic [3:0] fp_operation_i, input logic [2:0] fp_rounding_i,
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
    output logic [4:0] flags_o, output logic [4:0] fp_flags_o,
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
    logic carry_input, unused_fp_ready;
    logic [31:0] integer_y, fp_result;
    logic [4:0] fp_exceptions, fp32_exceptions, fp64_exceptions, fp_flags, saved_fp_flags;
    logic [31:0] fp32_result;
    logic [63:0] fp64_result;
    logic fp32_done, fp64_done, unused_fp64_ready;
    assign fp_done_o=fp64_i ? fp64_done : fp32_done;
    assign fp_result=fp64_i ? fp64_result[31:0] : fp32_result;
    assign fp_exceptions=fp64_i ? fp64_exceptions : fp32_exceptions;
    numerik_fp64 floating_point_wide (
        .clk_i(clk_i), .rst_i(rst_i), .valid_i(fp_start_i && fp64_i), .ready_o(unused_fp64_ready),
        .operation_i(fp_operation_i), .rounding_i(fp_rounding_i),
        .a_i({registers[ra_i | 4'd1],registers[ra_i]}),
        .b_i({registers[rb_i | 4'd1],registers[rb_i]}),
        .valid_o(fp64_done), .ready_i(retire_i && operation_i==ALU_FLOAT && fp64_i),
        .result_o(fp64_result), .exceptions_o(fp64_exceptions)
    );
    arithmetic_flags_t integer_flags;
    numerik_fp32 floating_point (
        .clk_i(clk_i), .rst_i(rst_i), .valid_i(fp_start_i && !fp64_i), .ready_o(unused_fp_ready),
        .operation_i(fp_operation_i), .rounding_i(fp_rounding_i),
        .a_i(r_operand), .b_i(s_operand),
        .valid_o(fp32_done), .ready_i(retire_i && operation_i==ALU_FLOAT && !fp64_i),
        .result_o(fp32_result), .exceptions_o(fp32_exceptions)
    );
    // Integer condition flags remain independent of FP exceptions. FloatStatus
    // reads the exceptions from the most recently retired Float operation.
    always_comb begin
        y_o=integer_y; calculated_flags=integer_flags;
        if(operation_i==ALU_FLOAT || operation_i==ALU_FLOAT_STATUS) begin
            y_o=operation_i==ALU_FLOAT ? fp_result : {27'b0,fp_flags};
            calculated_flags='0;
            calculated_flags.zero=y_o==0;
            calculated_flags.sign=y_o[31];
            calculated_flags.corrected_sign=y_o[31];
            if(operation_i==ALU_FLOAT_STATUS) begin
                case(shift_i)
                    SHIFT_LEFT: y_o={26'b0,fp_flags,1'b0};
                    SHIFT_RIGHT, SHIFT_ARITHMETIC_RIGHT: y_o={28'b0,fp_flags[4:1]};
                    default: ;
                endcase
            end
        end
    end

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
        .y_o(integer_y),
        .flags_o(integer_flags),
        .product_o(calculated_product)
    );

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            q <= '0; saved_q <= '0; saved_product <= '0; saved_flags <= '0;
            product <= '0;
            flags <= '0; fp_flags<=0; saved_fp_flags<=0;
            for (int index = 0; index < 16; index++) begin
                registers[index] <= '0; saved_registers[index] <= '0;
            end
        end else if(save_i) begin
            for(int index=0;index<16;index++) saved_registers[index]<=registers[index];
            saved_q<=q; saved_product<=product; saved_flags<=flags; saved_fp_flags<=fp_flags;
        end else if(restore_i) begin
            for(int index=0;index<16;index++) registers[index]<=saved_registers[index];
            q<=saved_q; product<=saved_product; flags<=saved_flags; fp_flags<=saved_fp_flags;
        end else if (retire_i) begin
            if (write_register_i) registers[rb_i] <= y_o;
            if (load_q_i) q <= y_o;
            if (write_flags_i) flags <= calculated_flags;
            product <= (operation_i==ALU_FLOAT && fp64_i) ? fp64_result : calculated_product;
            if(operation_i==ALU_FLOAT) fp_flags<=fp_exceptions;
        end
    end

    assign register_a_o = registers[ra_i];
    assign debug_register_o = registers[debug_register_i];
    assign q_o = q;
    assign product_o = product;
    assign flags_o = flags;
    assign fp_flags_o = fp_flags;
endmodule

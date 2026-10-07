// Generic binary32 execution backend. No object tags or language semantics.
//
// One request is accepted in IDLE. Operands, operation and rounding mode are
// latched together; subsequent input changes cannot affect the computation.
// The response stays stable until consumed. Reset cancels work and responses.
// LOGIK must use the handshake, not assume a fixed operation latency. This
// boundary also permits a device-specific implementation with the same bits,
// rounding, exceptions and reset behavior.
//
// HardFloat supplies arithmetic, including gradual underflow and all five IEEE
// rounding directions. Its significand multiplication is a Verilog multiply,
// available for FPGA DSP inference. Division/sqrt share an iterative engine.
// This wrapper does not claim a board clock rate or a DSP count.
module numerik_fp32 (
    input logic clk_i, input logic rst_i,
    input logic valid_i, output logic ready_o,
    input logic [3:0] operation_i,
    input logic [2:0] rounding_i,
    input logic [31:0] a_i, input logic [31:0] b_i,
    output logic valid_o, input logic ready_i,
    output logic [31:0] result_o,
    output logic [4:0] exceptions_o
);
    typedef enum logic [1:0] { IDLE, EXECUTE, DIVIDE, RESPONSE } state_t;
    state_t state;
    logic [3:0] operation;
    logic [2:0] rounding;
    logic [31:0] a, b;
    logic [32:0] ar, br, add_rec, mul_rec, from_rec, div_rec;
    logic [31:0] add_value, mul_value, from_value, div_value, to_value;
    logic [4:0] add_exc, mul_exc, from_exc, div_exc, compare_exc;
    logic [2:0] to_exc;
    logic less, equal, greater, unordered, div_ready, div_valid, unused_sqrt;
    logic [31:0] simple_value;
    logic [4:0] simple_exc;
    assign ready_o = state == IDLE && !rst_i;
    assign valid_o = state == RESPONSE && !rst_i;

    fNToRecFN #(8,24) decode_a(a, ar);
    fNToRecFN #(8,24) decode_b(b, br);
    // control=1 detects tininess after rounding. The vendor specialization
    // returns one canonical quiet NaN (0x7fc00000), not a payload-dependent NaN.
    addRecFN #(8,24) add_unit(1'b1, operation==1, ar, br, rounding, add_rec, add_exc);
    mulRecFN #(8,24) multiply_unit(1'b1, ar, br, rounding, mul_rec, mul_exc);
    iNToRecFN #(32,8,24) from_integer(1'b1, operation==6, a, rounding, from_rec, from_exc);
    recFNToIN #(8,24,32) to_integer(1'b1, ar, rounding, operation==7, to_value, to_exc);
    compareRecFN #(8,24) compare_unit(ar, br, 1'b0, less, equal, greater, unordered, compare_exc);
    divSqrtRecFN_small #(8,24,0) divide_unit(
        .nReset(!rst_i), .clock(clk_i), .control(1'b1), .inReady(div_ready),
        .inValid(state==EXECUTE && (operation==3 || operation==4) && rounding<=4),
        .sqrtOp(operation==4), .a(ar), .b(br), .roundingMode(rounding),
        .outValid(div_valid), .sqrtOpOut(unused_sqrt), .out(div_rec), .exceptionFlags(div_exc)
    );
    recFNToFN #(8,24) encode_add(add_rec, add_value);
    recFNToFN #(8,24) encode_mul(mul_rec, mul_value);
    recFNToFN #(8,24) encode_from(from_rec, from_value);
    recFNToFN #(8,24) encode_div(div_rec, div_value);

    always_comb begin
        simple_value=32'h7fc00000; simple_exc=5'b10000;
        case(operation)
            0,1: begin simple_value=add_value; simple_exc=add_exc; end
            2: begin simple_value=mul_value; simple_exc=mul_exc; end
            // Quiet compare: signaling NaNs raise invalid; all NaNs are unordered.
            5: begin simple_value={28'b0,unordered,greater,equal,less}; simple_exc=compare_exc; end
            6,8: begin simple_value=from_value; simple_exc=from_exc; end
            // Float-to-integer overflow is IEEE invalid, not FP overflow.
            7,9: begin simple_value=to_value; simple_exc={|to_exc[2:1],3'b0,to_exc[0]}; end
            default: ;
        endcase
        if(rounding>4) begin simple_value=32'h7fc00000; simple_exc=5'b10000; end
    end
    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            state<=IDLE; operation<=0; rounding<=0; a<=0; b<=0;
            result_o<=0; exceptions_o<=0;
        end else case(state)
            IDLE: if(valid_i) begin
                operation<=operation_i; rounding<=rounding_i; a<=a_i; b<=b_i;
                state<=EXECUTE;
            end
            EXECUTE: if((operation==3 || operation==4) && rounding<=4) begin
                if(div_ready) state<=DIVIDE;
            end else begin
                result_o<=simple_value; exceptions_o<=simple_exc; state<=RESPONSE;
            end
            DIVIDE: if(div_valid) begin
                result_o<=div_value; exceptions_o<=div_exc; state<=RESPONSE;
            end
            RESPONSE: if(ready_i) state<=IDLE;
            default: state<=IDLE;
        endcase
    end
endmodule

// One outstanding device transaction. LOGIK validates the instruction before
// start_i and freezes its architectural state until done_o. This module owns
// the captured wire request and the previous successful device result.
//
// The result becomes architecturally visible only at finish_i. A failed reply
// leaves that result unchanged. Request acceptance and reply consumption occur
// in separate states, so backpressure cannot duplicate a device side effect.
module logik_io (
    input logic clk_i, rst_i, start_i, finish_i,
    input logic write_i,
    input logic [31:0] address_i, data_i,
    output logic done_o, error_o,
    output logic [31:0] result_o,
    output logic valid_o, input logic ready_i,
    output logic write_o,
    output logic [31:0] address_o, data_o,
    input logic response_i, output logic response_ready_o,
    input logic response_error_i,
    input logic [31:0] response_data_i
);
    typedef enum logic [1:0] { IDLE, REQUEST, RESPONSE, COMPLETE } state_t;
    state_t state;
    logic [31:0] pending_result;
    assign valid_o = state == REQUEST && !rst_i;
    assign response_ready_o = state == RESPONSE && !rst_i;
    assign done_o = state == COMPLETE && !rst_i;
    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            state <= IDLE;
            write_o <= 0;
            address_o <= 0;
            data_o <= 0;
            pending_result <= 0;
            result_o <= 0;
            error_o <= 0;
        end else begin
            case (state)
                IDLE: if (start_i) begin
                    write_o <= write_i;
                    address_o <= address_i;
                    data_o <= write_i ? data_i : 32'b0;
                    error_o <= 0;
                    state <= REQUEST;
                end
                REQUEST: if (ready_i) state <= RESPONSE;
                RESPONSE: if (response_i) begin
                    pending_result <= response_data_i;
                    error_o <= response_error_i;
                    state <= COMPLETE;
                end
                COMPLETE: if (finish_i) begin
                    if (!error_o) result_o <= pending_result;
                    state <= IDLE;
                end
                default: state <= IDLE;
            endcase
        end
    end
endmodule

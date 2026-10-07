// Read-only identity lookup and ordered enumeration over resident and backing
// records. The parent supplies the best resident candidate while commands are
// locked. Backing storage supplies a key lookup, never a class or language rule.
//
// A resident object can be newer than its committed backing record. On equal
// identities the resident reference and class therefore win. NextObject still
// queries storage when a resident candidate exists: storage may hold a smaller
// qualifying identity. Exact lookup can finish immediately on a resident hit.
//
// Only the parent publishes the destination VR and response, after done_o with
// OK status. Neither request failures nor malformed metadata can partially
// change architectural state. This engine has no body-memory port.
module objekt_directory (
    input  logic clk_i, rst_i, start_i, next_i,
    input  logic [36:0] identity_i,
    input  logic [39:0] resident_ref_i, resident_class_i,
    output logic done_o,
    output logic [3:0] status_o,
    output logic [39:0] reference_o, class_o,
    output logic store_valid_o,
    input  logic store_ready_i,
    output logic [3:0] store_op_o,
    output logic [39:0] store_ref_o,
    input  logic store_response_i,
    output logic store_response_ready_o,
    input  logic [3:0] store_status_i,
    input  logic [39:0] store_ref_i, store_class_i
);
    localparam logic [39:0] NIL = 40'hc000000000;
    localparam logic [3:0] OK = 4'd0, SERVICE_ERROR = 4'd10;
    localparam logic [3:0] NEXT_RECORD = 4'd8, FIND_RECORD = 4'd9;
    typedef enum logic [1:0] { IDLE, REQUEST, WAIT_REPLY, DONE } state_t;
    state_t state;
    logic next_key;
    logic [36:0] identity;

    // Registers retain the operand through both handshakes. Reset cancels an
    // in-flight request; the system resets/drains its external channel too.
    assign done_o = state == DONE && !rst_i;
    assign store_valid_o = state == REQUEST && !rst_i;
    assign store_response_ready_o = state == WAIT_REPLY && !rst_i;
    assign store_op_o = next_key ? NEXT_RECORD : FIND_RECORD;
    assign store_ref_o = {3'b0, identity};

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            state <= IDLE;
            next_key <= 0;
            identity <= 0;
            status_o <= OK;
            reference_o <= NIL;
            class_o <= NIL;
        end else begin
            case (state)
                IDLE: if (start_i) begin
                    next_key <= next_i;
                    identity <= identity_i;
                    status_o <= OK;
                    reference_o <= resident_ref_i;
                    class_o <= resident_class_i;
                    if ((!next_i && resident_ref_i != NIL) ||
                        (next_i && (&identity_i)))
                        state <= DONE;
                    else
                        state <= REQUEST;
                end
                REQUEST: if (store_ready_i) state <= WAIT_REPLY;
                WAIT_REPLY: if (store_response_i) begin
                    state <= DONE;
                    if (store_status_i != OK) begin
                        status_o <= SERVICE_ERROR;
                    end else if (store_ref_i != NIL) begin
                        // A reply must be a canonical nonzero stored identity
                        // with a canonical class and the requested key relation.
                        if (store_ref_i[39:38] != 2'b10 || store_ref_i[36:0] == 0 ||
                            store_class_i[39:38] != 2'b10 || store_class_i[36:0] == 0 ||
                            (next_key ? store_ref_i[36:0] <= identity : store_ref_i[36:0] != identity))
                            status_o <= SERVICE_ERROR;
                        else if (reference_o == NIL || store_ref_i[36:0] < reference_o[36:0]) begin
                            reference_o <= store_ref_i;
                            class_o <= store_class_i;
                        end
                    end
                end
                DONE: state <= IDLE;
                default: state <= IDLE;
            endcase
        end
    end
endmodule

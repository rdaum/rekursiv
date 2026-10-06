// Allocation and streaming object transfer; OBJEKT owns pager publication.
//
// A transfer reserves fresh physical space, preserves a dirty/new collision
// victim, and fills the incoming body. Only DONE+OK permits the parent to publish
// entry_o. The old pager entry remains valid throughout, including on failure.
//
// Request and completion states are deliberately separate. Each channel allows
// one outstanding transaction, and output fields remain stable until accepted.
// A committed victim save may survive a later failure; reservations and completed
// writes into unpublished space also survive. Machine collection reclaims them,
// not rolled back here. See docs/interface.md for that failure contract.
module objekt_transfer #(
    parameter integer MEMORY_WORDS=512
)(
    input logic clk_i, input logic rst_i,
    input logic start_i, input logic allocate_i,
    input logic [39:0] target_i, input logic [23:0] size_i, input logic scan_i,
    input logic victim_valid_i, input logic [170:0] victim_i,
    input logic seed_i, input logic [39:0] seed_ref_i,
    input logic [23:0] seed_base_i, input logic [23:0] seed_size_i,
    input logic cursor_set_i, input logic [24:0] cursor_i,
    input logic [24:0] allocation_limit_i,
    output logic [23:0] required_size_o, output logic [39:0] required_class_o, output logic saved_victim_o,
    output logic [37:0] next_identity_o, output logic [24:0] body_cursor_o,
    output logic done_o, output logic [3:0] status_o, output logic [170:0] entry_o,
    output logic mem_valid_o, input logic mem_ready_i,
    output logic mem_write_o, output logic [23:0] mem_addr_o, output logic [39:0] mem_data_o,
    input logic mem_rsp_valid_i, output logic mem_rsp_ready_o,
    input logic [39:0] mem_rsp_data_i, input logic mem_rsp_error_i,
    output logic store_valid_o, input logic store_ready_i, output logic [2:0] store_op_o,
    output logic [39:0] store_ref_o, output logic [39:0] store_class_o,
    output logic [23:0] store_size_o, output logic store_cond_o,
    output logic [23:0] store_offset_o, output logic [39:0] store_data_o,
    input logic store_rsp_valid_i, output logic store_rsp_ready_o,
    input logic [3:0] store_rsp_status_i,
    input logic [39:0] store_rsp_ref_i, input logic [39:0] store_rsp_class_i,
    input logic [23:0] store_rsp_size_i, input logic store_rsp_cond_i, input logic [39:0] store_rsp_data_i
);
    `include "objekt_entry.svh"
    typedef enum logic [4:0] {
        IDLE, META, META_WAIT, RESERVE, SAVE, SAVE_WAIT,
        READ, READ_WAIT, PUT, PUT_WAIT, COMMIT, COMMIT_WAIT,
        FILL, GET, GET_WAIT, WRITE, WRITE_WAIT, DONE
    } transfer_state_t;
    transfer_state_t phase;
    localparam logic [3:0] OK=0, INVALID_REFERENCE=3, BOUNDS=6,
        MEMORY_ERROR=9, SERVICE_ERROR=10, OUT_OF_SPACE=11, IDENTITY_EXHAUSTED=12;
    localparam logic [39:0] NIL = 40'hc000000000;
    logic allocating, victim_valid;
    pager_entry_t victim, incoming;
    logic [23:0] offset;
    logic [39:0] buffer;
    logic [25:0] end_address;
    logic [24:0] seed_end, victim_end;
    logic store_wait, store_fail;

    // Widen before addition. The cursor can equal memory capacity; identity can
    // reach its exhausted sentinel. Neither counter wraps to reusable storage.
    assign end_address = {1'b0, body_cursor_o} + {2'b0, incoming.size};
    assign seed_end = {1'b0, seed_base_i} + {1'b0, seed_size_i};
    assign victim_end = {1'b0, victim.base} + {1'b0, victim.size};
    assign store_wait = phase == META_WAIT || phase == SAVE_WAIT || phase == PUT_WAIT ||
        phase == COMMIT_WAIT || phase == GET_WAIT;
    assign store_fail = store_wait && store_rsp_valid_i && store_rsp_status_i != OK;
    assign entry_o = incoming;
    assign required_size_o = incoming.size;
    assign required_class_o = incoming.class_reference;
    assign saved_victim_o = phase == COMMIT_WAIT && store_rsp_valid_i && store_rsp_status_i == OK;
    assign done_o = phase == DONE && !rst_i;

    // The transfer state selects the current channel owner and request payload.
    // Metadata/read requests name the incoming object; save requests name the
    // victim. Zero unused fields so transport traces have a canonical form.
    assign store_valid_o = (phase == META || phase == SAVE || phase == PUT ||
        phase == COMMIT || phase == GET) && !rst_i;
    assign store_rsp_ready_o = store_wait && !rst_i;
    assign store_op_o = phase == META ? 3'd0 : phase == GET ? 3'd1 :
        phase == SAVE ? 3'd2 : phase == PUT ? 3'd3 : 3'd4;
    assign store_ref_o = (phase == META || phase == GET) ? incoming.reference : victim.reference;
    assign store_class_o = phase == SAVE ? victim.class_reference : 40'b0;
    assign store_size_o = phase == SAVE ? victim.size : 24'b0;
    assign store_cond_o = phase == SAVE && victim.cond;
    assign store_offset_o = (phase == GET || phase == PUT) ? offset : 24'b0;
    assign store_data_o = phase == PUT ? buffer : 40'b0;
    assign mem_valid_o = (phase == READ || phase == WRITE) && !rst_i;
    assign mem_rsp_ready_o = (phase == READ_WAIT || phase == WRITE_WAIT) && !rst_i;
    assign mem_write_o = phase == WRITE;
    assign mem_addr_o = (phase == READ ? victim.base : incoming.base) + offset;
    assign mem_data_o = phase == WRITE ? buffer : 40'b0;

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            phase <= IDLE;
            next_identity_o <= 38'd1;
            body_cursor_o <= '0;
            allocating <= 1'b0;
            victim_valid <= 1'b0;
            victim <= '0;
            offset <= '0;
            buffer <= '0;
            incoming <= '0;
            status_o <= OK;
        end else begin
            // The parent admits these privileged operations only while idle.
            // Seeding raises high-water marks; recovery can lower only the body
            // cursor after checking every surviving body's end address.
            if (cursor_set_i) body_cursor_o <= cursor_i;
            if (seed_i) begin
                if (next_identity_o <= {1'b0, seed_ref_i[36:0]})
                    next_identity_o <= {1'b0, seed_ref_i[36:0]} + 38'd1;
                if (body_cursor_o < seed_end) body_cursor_o <= seed_end;
            end
            if (store_fail) begin
                status_o <= (phase == META_WAIT && store_rsp_status_i == INVALID_REFERENCE) ?
                    INVALID_REFERENCE : SERVICE_ERROR;
                phase <= DONE;
            end else begin
                unique case (phase)
                    IDLE: if (start_i) begin
                        allocating <= allocate_i;
                        victim <= victim_i;
                        victim_valid <= victim_valid_i;
                        offset <= '0;
                        status_o <= OK;
                        if (allocate_i) begin
                            incoming <= {3'b001, NIL, 24'b0, target_i, size_i,
                                2'b10, scan_i, next_identity_o[36:0]};
                            if (next_identity_o[37]) begin
                                status_o <= IDENTITY_EXHAUSTED;
                                phase <= DONE;
                            end else phase <= RESERVE;
                        end else begin
                            incoming <= {131'b0, target_i};
                            phase <= META;
                        end
                    end
                    META: if (store_ready_i) phase <= META_WAIT;
                    META_WAIT: if (store_rsp_valid_i) begin
                        // Do not trust backing metadata to redefine an identity
                        // or to smuggle an invalid class through refill.
                        if (store_rsp_ref_i != incoming.reference ||
                            store_rsp_class_i[39:38] != 2'b10 || store_rsp_class_i[36:0] == 0) begin
                            status_o <= INVALID_REFERENCE;
                            phase <= DONE;
                        end else begin
                            incoming <= {store_rsp_cond_i, 2'b00, NIL, 24'b0,
                                store_rsp_class_i, store_rsp_size_i, incoming.reference};
                            phase <= RESERVE;
                        end
                    end
                    RESERVE: begin
                        if (end_address > {1'b0, allocation_limit_i}) begin
                            status_o <= OUT_OF_SPACE;
                            phase <= DONE;
                        end else begin
                            incoming.base <= body_cursor_o[23:0];
                            body_cursor_o <= end_address[24:0];
                            if (allocating) next_identity_o <= next_identity_o + 38'd1;
                            if (victim_valid && (victim.is_new || victim.modified)) begin
                                if (victim_end > 25'(MEMORY_WORDS)) begin
                                    status_o <= BOUNDS;
                                    phase <= DONE;
                                end else phase <= SAVE;
                            end else phase <= FILL;
                        end
                    end
                    // BeginSave -> each resident word -> CommitSave. The backing
                    // adapter publishes the record only after CommitSave succeeds.
                    SAVE: if (store_ready_i) phase <= SAVE_WAIT;
                    SAVE_WAIT: if (store_rsp_valid_i) begin
                        offset <= '0;
                        if (victim.size == 0) phase <= COMMIT;
                        else phase <= READ;
                    end
                    READ: if (mem_ready_i) phase <= READ_WAIT;
                    READ_WAIT: if (mem_rsp_valid_i) begin
                        if (mem_rsp_error_i) begin
                            status_o <= MEMORY_ERROR;
                            phase <= DONE;
                        end else begin
                            buffer <= mem_rsp_data_i;
                            phase <= PUT;
                        end
                    end
                    PUT: if (store_ready_i) phase <= PUT_WAIT;
                    PUT_WAIT: if (store_rsp_valid_i) begin
                        if (offset + 24'd1 == victim.size) phase <= COMMIT;
                        else begin
                            offset <= offset + 24'd1;
                            phase <= READ;
                        end
                    end
                    COMMIT: if (store_ready_i) phase <= COMMIT_WAIT;
                    COMMIT_WAIT: if (store_rsp_valid_i) begin
                        offset <= '0;
                        phase <= FILL;
                    end
                    // Allocation writes nil into scanned bodies and zero into
                    // opaque bodies. Refill streams exact 40-bit stored words.
                    FILL: begin
                        if (incoming.size == 0) phase <= DONE;
                        else if (allocating) begin
                            buffer <= incoming.reference[37] ? NIL : 40'b0;
                            phase <= WRITE;
                        end else phase <= GET;
                    end
                    GET: if (store_ready_i) phase <= GET_WAIT;
                    GET_WAIT: if (store_rsp_valid_i) begin
                        buffer <= store_rsp_data_i;
                        phase <= WRITE;
                    end
                    WRITE: if (mem_ready_i) phase <= WRITE_WAIT;
                    WRITE_WAIT: if (mem_rsp_valid_i) begin
                        if (mem_rsp_error_i) begin
                            status_o <= MEMORY_ERROR;
                            phase <= DONE;
                        end else begin
                            // Representation is the successfully written first
                            // field, never a speculative or failed write value.
                            if (offset == 0) incoming.representation <= buffer;
                            if (offset + 24'd1 == incoming.size) phase <= DONE;
                            else begin
                                offset <= offset + 24'd1;
                                phase <= FILL;
                            end
                        end
                    end
                    DONE: phase <= IDLE;
                    default: phase <= IDLE;
                endcase
            end
        end
    end
    // Tag/scan bits do not contribute to the identity high-water mark. The old
    // cached first field is unnecessary because save reads the complete body.
    wire unused_fields = &{1'b0, seed_ref_i[39:37], victim.representation};
endmodule

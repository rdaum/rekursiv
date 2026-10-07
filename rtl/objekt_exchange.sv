// Exchange two identity bindings without materializing both bodies in RAM.
//
// Each source is read from its resident snapshot or committed backing record.
// The engine streams its class, size, flags and every word into the other key.
// Both records remain private in a generic storage batch until CommitBatch.
// Reads during a batch see committed records, so pager collisions and bodies
// larger than the resident heap do not require a temporary object or buffer.
//
// The parent locks command acceptance and owns pager invalidation. A successful
// batch is the only publication point. Errors abort unpublished storage writes;
// they leave pager metadata, selection, allocation cursors and RAM unchanged.
module objekt_exchange #(
    parameter integer MEMORY_WORDS=512
)(
    input logic clk_i, rst_i, start_i,
    input logic [39:0] first_i, second_i,
    input logic first_resident_i, second_resident_i,
    input logic [170:0] first_entry_i, second_entry_i,
    output logic done_o, output logic [3:0] status_o,
    output logic [39:0] first_o, second_o,
    output logic mem_valid_o, input logic mem_ready_i,
    output logic [23:0] mem_address_o,
    input logic mem_response_i, output logic mem_response_ready_o,
    input logic [39:0] mem_data_i, input logic mem_error_i,
    output logic store_valid_o, input logic store_ready_i,
    output logic [3:0] store_op_o,
    output logic [39:0] store_ref_o, store_class_o,
    output logic [23:0] store_size_o, output logic store_cond_o,
    output logic [23:0] store_offset_o, output logic [39:0] store_data_o,
    input logic store_response_i, output logic store_response_ready_o,
    input logic [3:0] store_status_i,
    input logic [39:0] store_ref_i, store_class_i,
    input logic [23:0] store_size_i, input logic store_cond_i,
    input logic [39:0] store_data_i
);
    `include "objekt_entry.svh"
    typedef enum logic [4:0] {
        IDLE, SELECT_META, META, META_WAIT, BEGIN_BATCH, BEGIN_WAIT,
        SAVE, SAVE_WAIT, MEMORY_READ, MEMORY_WAIT, READ, READ_WAIT,
        WRITE, WRITE_WAIT, COMMIT, COMMIT_WAIT, PUBLISH, PUBLISH_WAIT,
        ABORT_BATCH, ABORT_WAIT, DONE
    } phase_t;
    phase_t phase;
    pager_entry_t first, second, source_entry, destination_entry;
    logic first_resident, second_resident, which, resident, begun, store_wait;
    logic [23:0] offset;
    logic [39:0] buffer;
    localparam logic [3:0] OK=0, INVALID_REFERENCE=3, BOUNDS=6,
        MEMORY_ERROR=9, SERVICE_ERROR=10;

    assign source_entry=which ? second : first;
    assign destination_entry=which ? first : second;
    assign resident=which ? second_resident : first_resident;
    assign first_o=first.reference;
    assign second_o=second.reference;
    assign done_o=phase==DONE && !rst_i;
    assign mem_valid_o=phase==MEMORY_READ && !rst_i;
    assign mem_response_ready_o=phase==MEMORY_WAIT && !rst_i;
    assign mem_address_o=source_entry.base+offset;
    assign store_wait=phase==META_WAIT || phase==BEGIN_WAIT || phase==SAVE_WAIT ||
        phase==READ_WAIT || phase==WRITE_WAIT || phase==COMMIT_WAIT ||
        phase==PUBLISH_WAIT || phase==ABORT_WAIT;
    assign store_response_ready_o=store_wait && !rst_i;
    assign store_valid_o=(phase==META || phase==BEGIN_BATCH || phase==SAVE ||
        phase==READ || phase==WRITE || phase==COMMIT || phase==PUBLISH ||
        phase==ABORT_BATCH) && !rst_i;
    always_comb begin
        store_op_o=0; store_ref_o=0; store_class_o=0; store_size_o=0;
        store_cond_o=0; store_offset_o=0; store_data_o=0;
        case(phase)
            META: store_ref_o=source_entry.reference;
            BEGIN_BATCH: store_op_o=5;
            SAVE: begin
                store_op_o=2; store_ref_o=destination_entry.reference;
                store_class_o=source_entry.class_reference;
                store_size_o=source_entry.size; store_cond_o=source_entry.cond;
            end
            READ: begin
                store_op_o=1; store_ref_o=source_entry.reference;
                store_offset_o=offset;
            end
            WRITE: begin
                store_op_o=3; store_ref_o=destination_entry.reference;
                store_offset_o=offset; store_data_o=buffer;
            end
            COMMIT: begin store_op_o=4; store_ref_o=destination_entry.reference; end
            PUBLISH: store_op_o=6;
            ABORT_BATCH: store_op_o=7;
            default: ;
        endcase
    end

    // The metadata phase validates both keys before opening a batch. Equal keys
    // are a validated no-op. Scanning compatibility is checked by the parent.
    task automatic metadata_done;
        if(first.reference==second.reference) phase<=DONE;
        else if(!which) begin which<=1; phase<=SELECT_META; end
        else begin which<=0; phase<=BEGIN_BATCH; end
    endtask
    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            phase<=IDLE; first<='0; second<='0;
            first_resident<=0; second_resident<=0;
            which<=0; begun<=0; offset<=0; buffer<=0; status_o<=OK;
        end else if(store_wait && store_response_i && store_status_i!=OK) begin
            if(phase==ABORT_WAIT) phase<=DONE;
            else begin
                status_o<=phase==META_WAIT && store_status_i==INVALID_REFERENCE ?
                    INVALID_REFERENCE : SERVICE_ERROR;
                phase<=begun ? ABORT_BATCH : DONE;
            end
        end else begin
            case(phase)
                IDLE: if(start_i) begin
                    first<=first_resident_i ? first_entry_i : {131'b0,first_i};
                    second<=second_resident_i ? second_entry_i : {131'b0,second_i};
                    first_resident<=first_resident_i;
                    second_resident<=second_resident_i;
                    which<=0; begun<=0; offset<=0; status_o<=OK;
                    phase<=SELECT_META;
                end
                SELECT_META: if(resident) begin
                    if({1'b0,source_entry.base}+{1'b0,source_entry.size}>25'(MEMORY_WORDS)) begin
                        status_o<=BOUNDS; phase<=DONE;
                    end else metadata_done();
                end else phase<=META;
                META: if(store_ready_i) phase<=META_WAIT;
                META_WAIT: if(store_response_i) begin
                    if(store_ref_i!=source_entry.reference || store_class_i[39:38]!=2'b10 || store_class_i[36:0]==0) begin
                        status_o<=INVALID_REFERENCE; phase<=DONE;
                    end else begin
                        if(which) begin
                            second.class_reference<=store_class_i;
                            second.size<=store_size_i; second.cond<=store_cond_i;
                        end else begin
                            first.class_reference<=store_class_i;
                            first.size<=store_size_i; first.cond<=store_cond_i;
                        end
                        metadata_done();
                    end
                end
                BEGIN_BATCH: if(store_ready_i) phase<=BEGIN_WAIT;
                BEGIN_WAIT: if(store_response_i) begin begun<=1; phase<=SAVE; end
                SAVE: if(store_ready_i) phase<=SAVE_WAIT;
                SAVE_WAIT: if(store_response_i) begin
                    offset<=0;
                    if(source_entry.size==0) phase<=COMMIT;
                    else phase<=resident ? MEMORY_READ : READ;
                end
                MEMORY_READ: if(mem_ready_i) phase<=MEMORY_WAIT;
                MEMORY_WAIT: if(mem_response_i) begin
                    if(mem_error_i) begin status_o<=MEMORY_ERROR; phase<=ABORT_BATCH; end
                    else begin buffer<=mem_data_i; phase<=WRITE; end
                end
                READ: if(store_ready_i) phase<=READ_WAIT;
                READ_WAIT: if(store_response_i) begin buffer<=store_data_i; phase<=WRITE; end
                WRITE: if(store_ready_i) phase<=WRITE_WAIT;
                WRITE_WAIT: if(store_response_i) begin
                    if(offset+24'd1==source_entry.size) phase<=COMMIT;
                    else begin offset<=offset+24'd1; phase<=resident ? MEMORY_READ : READ; end
                end
                COMMIT: if(store_ready_i) phase<=COMMIT_WAIT;
                COMMIT_WAIT: if(store_response_i) begin
                    if(which) phase<=PUBLISH;
                    else begin which<=1; phase<=SAVE; end
                end
                PUBLISH: if(store_ready_i) phase<=PUBLISH_WAIT;
                PUBLISH_WAIT: if(store_response_i) begin begun<=0; phase<=DONE; end
                ABORT_BATCH: if(store_ready_i) phase<=ABORT_WAIT;
                ABORT_WAIT: if(store_response_i) begin begun<=0; phase<=DONE; end
                DONE: phase<=IDLE;
                default: phase<=IDLE;
            endcase
        end
    end
    // Only payload and residency matter. The full packed entries keep the
    // shared pager layout explicit, while cached/dirty fields are not published.
    wire unused_fields=&{1'b0, source_entry.representation, source_entry.is_new,
        source_entry.modified, destination_entry[170:40]};
endmodule

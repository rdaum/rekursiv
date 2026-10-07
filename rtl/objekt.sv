// OBJEKT resident operations, service access, and transfer publication.
//
// The command evaluator computes a candidate state from PRE-COMMAND values.
// Ordinary memory access uses the old index. PREPARE explicitly forwards the
// new index into an address/check register; PREPARED accesses the old contents
// of that register while selection and the next address can change independently.
// See docs/interface.md for precedence and mapping invalidation.
//
// Resident operations retire either on acceptance (no external memory) or on
// successful memory completion. The pending_* registers keep candidate state
// private during a stall. An error discards it. The response state holds the
// result until consumed and never reexecutes the accepted command.
//
// objekt_transfer owns reservations and streaming save/refill. This parent owns
// the pager and is the only publisher of a completed transfer entry. Recovery
// uses either the machine collector port or the halted test service port;
// both exclude mutator commands while they own maintenance.
module objekt #(
    parameter integer PAGER_BITS = 4,
    parameter integer MEMORY_WORDS = 512
)(
    input logic clk_i, input logic rst_i, input logic run_i,
    input logic gc_enable_i, gc_active_i, gc_explicit_i,
    input logic gc_valid_i, output logic gc_ready_o, input logic [3:0] gc_operation_i,
    input logic [39:0] gc_data_i, gc_root_i,
    output logic gc_response_o, input logic gc_response_ready_i,
    output logic [3:0] gc_status_o, output logic [39:0] gc_result_o,
    output logic gc_committed_o,
    input logic cmd_valid_i, output logic cmd_ready_o,
    input logic [3:0] pager_i, input logic [3:0] index_i,
    input logic [2:0] register_i, input logic [1:0] memory_i,
    input logic prepare_i, prepared_i,
    input logic [3:0] read_i, input logic load_vr_i, input logic [2:0] vr_i,
    input logic [23:0] alloc_size_i, input logic alloc_scan_i,
    input logic [39:0] data_i, input logic check_type_i, input logic [39:0] expected_type_i,
    input logic svc_valid_i, output logic svc_ready_o, input logic [3:0] svc_op_i,
    input logic [39:0] svc_ref_i, input logic [39:0] svc_class_i,
    input logic [23:0] svc_base_i, input logic [23:0] svc_size_i,
    input logic [39:0] svc_repr_i, input logic [2:0] svc_flags_i,
    input logic [5:0] svc_code_i,
    output logic rsp_valid_o, input logic rsp_ready_i,
    output logic [3:0] rsp_status_o, output logic [39:0] rsp_data_o,
    output logic mem_valid_o, input logic mem_ready_i,
    output logic mem_write_o, output logic [23:0] mem_addr_o, output logic [39:0] mem_data_o,
    input logic mem_rsp_valid_i, output logic mem_rsp_ready_o,
    input logic [39:0] mem_rsp_data_i, input logic mem_rsp_error_i,
    output logic store_valid_o, input logic store_ready_i, output logic [3:0] store_op_o,
    output logic [39:0] store_ref_o, output logic [39:0] store_class_o,
    output logic [23:0] store_size_o, output logic store_cond_o,
    output logic [23:0] store_offset_o, output logic [39:0] store_data_o,
    input logic store_rsp_valid_i, output logic store_rsp_ready_o,
    input logic [3:0] store_rsp_status_i,
    input logic [39:0] store_rsp_ref_i, input logic [39:0] store_rsp_class_i,
    input logic [23:0] store_rsp_size_i, input logic store_rsp_cond_i, input logic [39:0] store_rsp_data_i,
    output logic [37:0] dbg_next_identity_o, output logic [24:0] dbg_body_cursor_o,
    output logic dbg_maintenance_o, input logic [1:0] dbg_class_code_i,
    output logic dbg_class_valid_o, output logic [39:0] dbg_compact_class_o,
    input logic [2:0] dbg_vr_i, output logic [39:0] dbg_vr_o,
    output logic [39:0] dbg_idx_o, output logic [39:0] dbg_reg_o,
    output logic [39:0] dbg_prepared_ref_o, dbg_prepared_index_o,
    output logic [23:0] dbg_prepared_address_o, output logic [3:0] dbg_prepared_status_o,
    output logic dbg_selected_o, output logic [39:0] dbg_ref_o,
    output logic [39:0] dbg_class_o, output logic [23:0] dbg_size_o,
    output logic [23:0] dbg_base_o, output logic [39:0] dbg_repr_o, output logic [2:0] dbg_flags_o,
    input logic [PAGER_BITS-1:0] dbg_slot_i, output logic dbg_valid_o,
    output logic [39:0] dbg_entry_ref_o, output logic [39:0] dbg_entry_class_o,
    output logic [23:0] dbg_entry_size_o, output logic [23:0] dbg_entry_base_o,
    output logic [39:0] dbg_entry_repr_o, output logic [2:0] dbg_entry_flags_o
);
    localparam integer ENTRIES = 1 << PAGER_BITS;
    localparam [39:0] NIL = 40'hc000000000;
    localparam [3:0] OK=0, BAD_COMMAND=1, BAD_VALUE=2, INVALID_REFERENCE=3,
        NOT_RESIDENT=4, NO_SELECTION=5, BOUNDS=6, INDEX_OVERFLOW=7, TYPE_ERROR=8, MEMORY_ERROR=9;
    `include "objekt_address.svh"
    prepared_access_t prepared_access, next_prepared, pending_prepared;
    typedef enum logic [2:0] {IDLE, REQUEST, WAIT_MEMORY, RESPONSE, TRANSFER, EXCHANGE, DIRECTORY} resident_state_t;
    resident_state_t phase;
    // Entry payload: {cond, modified, new, representation, base, class, size, reference}.
    logic [170:0] entries [0:ENTRIES-1];
    logic [ENTRIES-1:0] valid, persistent;
    logic [39:0] classes [0:3];
    logic [3:0] class_valid;
    logic [39:0] vr [0:7];
    logic signed [39:0] idx, idxreg;
    logic selected, maintenance;
    logic [170:0] snapshot;
    // Capacity reads use the same cursor/limit as allocation. They require no
    // selected object or memory transfer and report pre-command values. The
    // inactive semispace is collector reserve, not currently allocatable RAM.
    wire [24:0] allocation_limit = gc_enable_i
        ? (gc_space ? 25'(MEMORY_WORDS) : 25'(MEMORY_WORDS/2)) : 25'(MEMORY_WORDS);
    wire [24:0] free_words = dbg_body_cursor_o < allocation_limit
        ? allocation_limit - dbg_body_cursor_o : 25'd0;
    wire [37:0] free_identities = 38'h2000000000 - dbg_next_identity_o;

    // Private retirement record for one resident memory operation. Inputs may
    // change immediately after acceptance; completion uses only these fields.
    logic signed [39:0] pending_idx, pending_reg;
    logic pending_selected, pending_load_vr, pending_mutator, pending_write;
    logic [2:0] pending_vr;
    logic [39:0] pending_data;
    logic [170:0] pending_snapshot;
    logic [170:0] pending_memory_entry;
    logic [PAGER_BITS-1:0] pending_slot;

    logic resident_mem_write_o;
    logic [23:0] resident_mem_addr_o;
    logic [39:0] resident_mem_data_o;
    wire transfer_mem_valid, transfer_mem_write, transfer_mem_rsp_ready, transfer_done;
    wire [23:0] transfer_mem_addr;
    wire [39:0] transfer_mem_data;
    wire [170:0] transfer_entry;
    wire [3:0] transfer_status;
    wire [PAGER_BITS-1:0] transfer_slot=(pager_i==6) ? dbg_next_identity_o[PAGER_BITS-1:0] : data_i[PAGER_BITS-1:0];
    wire start_transfer=cmd_valid_i && cmd_ready_o && ((pager_i==5 && error==NOT_RESIDENT) || (pager_i==6 && error==OK));
    wire seed_transfer=svc_valid_i && svc_ready_o && svc_op_i==0 && is_ref(svc_ref_i) && is_ref(svc_class_i) && (svc_size_i!=0 || svc_repr_i==NIL);
    wire transfer_store_valid;
    wire [3:0] transfer_store_op;
    wire [39:0] transfer_store_ref;
    wire [39:0] transfer_store_class;
    wire [23:0] transfer_store_size;
    wire transfer_store_cond;
    wire [23:0] transfer_store_offset;
    wire [39:0] transfer_store_data;
    wire transfer_store_ready;
    wire exchange_store_valid;
    wire [3:0] exchange_store_op;
    wire [39:0] exchange_store_ref;
    wire [39:0] exchange_store_class;
    wire [23:0] exchange_store_size;
    wire exchange_store_cond;
    wire [23:0] exchange_store_offset;
    wire [39:0] exchange_store_data;
    wire exchange_store_ready;
    wire exchange_done, exchange_mem_valid, exchange_mem_ready;
    wire [3:0] exchange_status;
    wire [23:0] exchange_mem_address;
    wire [39:0] exchange_first, exchange_second;
    logic [ENTRIES-1:0] exchange_edges;
    wire start_exchange=cmd_valid_i && cmd_ready_o && pager_i==7 && error==OK;
    logic [39:0] directory_best_ref, directory_best_class;
    wire directory_valid, directory_ready, directory_done;
    wire [3:0] directory_op, directory_status;
    wire [39:0] directory_request, directory_ref, directory_class;
    logic [2:0] directory_vr;
    wire start_directory=cmd_valid_i && cmd_ready_o && (pager_i==8 || pager_i==9) && error==OK;
    // Search only resident metadata here. The backend merges this candidate
    // with the ordered backing index without fetching any object bodies.
    always_comb begin
        directory_best_ref=NIL; directory_best_class=NIL;
        for(integer k=0;k<ENTRIES;k=k+1) begin
            if(valid[k] && (pager_i==8 ? entries[k][36:0]>data_i[36:0] : entries[k][36:0]==data_i[36:0]) &&
                (directory_best_ref==NIL || entries[k][36:0]<directory_best_ref[36:0])) begin
                directory_best_ref=entries[k][39:0]; directory_best_class=entries[k][103:64];
            end
        end
    end
    objekt_directory directory (
        .clk_i(clk_i),.rst_i(rst_i),.start_i(start_directory),.next_i(pager_i==8),.identity_i(data_i[36:0]),
        .resident_ref_i(directory_best_ref),.resident_class_i(directory_best_class),
        .done_o(directory_done),.status_o(directory_status),.reference_o(directory_ref),.class_o(directory_class),
        .store_valid_o(directory_valid),.store_ready_i(store_ready_i),.store_op_o(directory_op),.store_ref_o(directory_request),
        .store_response_i(store_rsp_valid_i),.store_response_ready_o(directory_ready),
        .store_status_i(store_rsp_status_i),.store_ref_i(store_rsp_ref_i),.store_class_i(store_rsp_class_i)
    );
    // Transfer requests are admitted only for an explicit Fetch miss or a valid
    // Allocate. Probe remains resident-only. While TRANSFER owns the memory bus,
    // no resident command or halted service operation can enter the core.
    objekt_transfer #(.MEMORY_WORDS(MEMORY_WORDS)) transfer (
        .clk_i(clk_i),.rst_i(rst_i),.start_i(start_transfer),.allocate_i(pager_i==6),
        .target_i(data_i),.size_i(alloc_size_i),.scan_i(alloc_scan_i),
        .victim_valid_i(valid[transfer_slot]),.victim_i(entries[transfer_slot]),
        .seed_i(seed_transfer),.seed_ref_i(svc_ref_i),.seed_base_i(svc_base_i),.seed_size_i(svc_size_i),
        .cursor_set_i(gc_commit || (svc_valid_i && svc_ready_o && svc_op_i==6 && maintenance && cursor_safe)),
        .cursor_i(gc_commit ? gc_cursor : svc_repr_i[24:0]),
        .identity_floor_valid_i(svc_valid_i && svc_ready_o && svc_op_i==8 &&
            svc_repr_i!=0 && svc_repr_i<=40'h2000000000),
        .identity_floor_i(svc_repr_i[37:0]),
        .allocation_limit_i(allocation_limit),
        .required_size_o(gc_needed),.required_class_o(gc_needed_class),.saved_victim_o(saved_victim),
        .next_identity_o(dbg_next_identity_o),.body_cursor_o(dbg_body_cursor_o),
        .done_o(transfer_done),.status_o(transfer_status),.entry_o(transfer_entry),
        .mem_valid_o(transfer_mem_valid),.mem_ready_i(mem_ready_i),.mem_write_o(transfer_mem_write),
        .mem_addr_o(transfer_mem_addr),.mem_data_o(transfer_mem_data),
        .mem_rsp_valid_i(mem_rsp_valid_i),.mem_rsp_ready_o(transfer_mem_rsp_ready),
        .mem_rsp_data_i(mem_rsp_data_i),.mem_rsp_error_i(mem_rsp_error_i),
        .store_valid_o(transfer_store_valid),.store_ready_i(store_ready_i),.store_op_o(transfer_store_op),
        .store_ref_o(transfer_store_ref),.store_class_o(transfer_store_class),.store_size_o(transfer_store_size),
        .store_cond_o(transfer_store_cond),.store_offset_o(transfer_store_offset),.store_data_o(transfer_store_data),
        .store_rsp_valid_i(store_rsp_valid_i),.store_rsp_ready_o(transfer_store_ready),
        .store_rsp_status_i(store_rsp_status_i),.store_rsp_ref_i(store_rsp_ref_i),.store_rsp_class_i(store_rsp_class_i),
        .store_rsp_size_i(store_rsp_size_i),.store_rsp_cond_i(store_rsp_cond_i),.store_rsp_data_i(store_rsp_data_i)
    );
    objekt_exchange #(.MEMORY_WORDS(MEMORY_WORDS)) exchange (
        .clk_i(clk_i),.rst_i(rst_i),.start_i(start_exchange),
        .first_i(data_i),.second_i(vr[vr_i]),
        .first_resident_i(valid[data_i[PAGER_BITS-1:0]] && entries[data_i[PAGER_BITS-1:0]][39:0]==data_i),
        .second_resident_i(valid[vr[vr_i][PAGER_BITS-1:0]] && entries[vr[vr_i][PAGER_BITS-1:0]][39:0]==vr[vr_i]),
        .first_entry_i(entries[data_i[PAGER_BITS-1:0]]),.second_entry_i(entries[vr[vr_i][PAGER_BITS-1:0]]),
        .done_o(exchange_done),.status_o(exchange_status),.first_o(exchange_first),.second_o(exchange_second),
        .mem_valid_o(exchange_mem_valid),.mem_ready_i(mem_ready_i),.mem_address_o(exchange_mem_address),
        .mem_response_i(mem_rsp_valid_i),.mem_response_ready_o(exchange_mem_ready),
        .mem_data_i(mem_rsp_data_i),.mem_error_i(mem_rsp_error_i),
        .store_valid_o(exchange_store_valid),.store_ready_i(store_ready_i),.store_op_o(exchange_store_op),
        .store_ref_o(exchange_store_ref),.store_class_o(exchange_store_class),.store_size_o(exchange_store_size),
        .store_cond_o(exchange_store_cond),.store_offset_o(exchange_store_offset),.store_data_o(exchange_store_data),
        .store_response_i(store_rsp_valid_i),.store_response_ready_o(exchange_store_ready),
        .store_status_i(store_rsp_status_i),.store_ref_i(store_rsp_ref_i),.store_class_i(store_rsp_class_i),
        .store_size_i(store_rsp_size_i),.store_cond_i(store_rsp_cond_i),.store_data_i(store_rsp_data_i)
    );
    assign store_valid_o=phase==DIRECTORY ? directory_valid : phase==EXCHANGE ? exchange_store_valid : transfer_store_valid;
    assign store_op_o=phase==DIRECTORY ? directory_op : phase==EXCHANGE ? exchange_store_op : transfer_store_op;
    assign store_ref_o=phase==DIRECTORY ? directory_request : phase==EXCHANGE ? exchange_store_ref : transfer_store_ref;
    assign store_class_o=phase==DIRECTORY ? '0 : phase==EXCHANGE ? exchange_store_class : transfer_store_class;
    assign store_size_o=phase==DIRECTORY ? '0 : phase==EXCHANGE ? exchange_store_size : transfer_store_size;
    assign store_cond_o=phase==DIRECTORY ? '0 : phase==EXCHANGE ? exchange_store_cond : transfer_store_cond;
    assign store_offset_o=phase==DIRECTORY ? '0 : phase==EXCHANGE ? exchange_store_offset : transfer_store_offset;
    assign store_data_o=phase==DIRECTORY ? '0 : phase==EXCHANGE ? exchange_store_data : transfer_store_data;
    assign store_rsp_ready_o=phase==DIRECTORY ? directory_ready : phase==EXCHANGE ? exchange_store_ready : transfer_store_ready;
    // Recovery has its own command port. It can run only after the failed
    // mutator response is consumed. Host maintenance and new mutator commands
    // cannot enter while LOGIK owns collection. All physical traffic still
    // uses the one ordinary external RAM channel.
    logic gc_commit, gc_space, gc_layout_valid, saved_victim, gc_ready;
    logic [PAGER_BITS-1:0] saved_victim_slot;
    assign gc_ready_o=gc_ready && phase==IDLE && !maintenance;
    logic [PAGER_BITS-1:0] gc_slot;
    logic [ENTRIES-1:0] gc_retained;
    logic [ENTRIES*24-1:0] gc_bases;
    logic [24:0] gc_cursor;
    logic [23:0] gc_needed;
    logic [39:0] gc_needed_class;
    logic [ENTRIES-1:0] saved_edges;
    logic saved_scan;
    logic [39:0] gc_root;
    logic gc_mem_valid, gc_mem_ready, gc_mem_write;
    logic [23:0] gc_mem_address;
    logic [39:0] gc_mem_data;
    always_comb begin
        // Explicit collection has no failed transfer. Ignore its stale size
        // and class registers throughout this collection.
        gc_root=gc_data_i==19 ? (gc_explicit_i ? 40'b0 : gc_needed_class) : gc_root_i;
        if(gc_data_i<8) gc_root=vr[gc_data_i[2:0]];
        else if(gc_data_i==8) gc_root=selected ? snapshot[39:0] : 40'b0;
        else if(gc_data_i==9) gc_root=selected ? snapshot[103:64] : 40'b0;
        else if(gc_data_i<14) gc_root=class_valid[2'(gc_data_i-10)] ? classes[2'(gc_data_i-10)] : 40'b0;
        gc_layout_valid=MEMORY_WORDS>=2 && MEMORY_WORDS%2==0;
        for(integer k=0;k<ENTRIES;k=k+1) begin
            if(valid[k] && (entries[k][127:104] < (gc_space ? 24'(MEMORY_WORDS/2) : 24'b0) ||
                {1'b0,entries[k][127:104]}+{1'b0,entries[k][63:40]} >
                (gc_space ? 25'(MEMORY_WORDS) : 25'(MEMORY_WORDS/2)))) gc_layout_valid=0;
        end
    end
    objekt_gc #(.PAGER_BITS(PAGER_BITS),.MEMORY_WORDS(MEMORY_WORDS)) collector_datapath (
        .clk_i(clk_i),.rst_i(rst_i),.active_i(gc_active_i),
        .valid_i(gc_valid_i && phase==IDLE && !maintenance),.ready_o(gc_ready),
        .operation_i(gc_operation_i),.data_i(gc_data_i),.root_i(gc_root),.needed_i(gc_explicit_i ? 24'b0 : gc_needed),
        .response_o(gc_response_o),.response_ready_i(gc_response_ready_i),
        .status_o(gc_status_o),.result_o(gc_result_o),.committed_o(gc_committed_o),
        .slot_o(gc_slot),.entry_valid_i(valid[gc_slot]),.entry_i(entries[gc_slot]),
        .entry_persistent_i(persistent[gc_slot]),
        .entry_prepared_i(prepared_access.status==OK && entries[gc_slot][39:0]==prepared_access.reference),
        .layout_valid_i(gc_layout_valid),
        .commit_o(gc_commit),.retained_o(gc_retained),.bases_o(gc_bases),.cursor_o(gc_cursor),
        .space_o(gc_space),
        .mem_valid_o(gc_mem_valid),.mem_ready_i(mem_ready_i),.mem_write_o(gc_mem_write),
        .mem_addr_o(gc_mem_address),.mem_data_o(gc_mem_data),
        .mem_response_i(mem_rsp_valid_i),.mem_response_ready_o(gc_mem_ready),
        .mem_data_i(mem_rsp_data_i),.mem_error_i(mem_rsp_error_i)
    );
    assign mem_write_o=gc_active_i ? gc_mem_write :(phase==EXCHANGE) ? 1'b0 :(phase==TRANSFER) ? transfer_mem_write : resident_mem_write_o;
    assign mem_addr_o=gc_active_i ? gc_mem_address :(phase==EXCHANGE) ? exchange_mem_address :(phase==TRANSFER) ? transfer_mem_addr : resident_mem_addr_o;
    assign mem_data_o=gc_active_i ? gc_mem_data :(phase==EXCHANGE) ? 40'b0 :(phase==TRANSFER) ? transfer_mem_data : resident_mem_data_o;
    assign cmd_ready_o = phase==IDLE && run_i && !maintenance && !gc_active_i && !rst_i;
    assign svc_ready_o = phase==IDLE && !run_i && !gc_active_i && !rst_i;
    assign rsp_valid_o = phase==RESPONSE && !rst_i;
    assign mem_valid_o = gc_active_i ? gc_mem_valid : ((phase==REQUEST) || (phase==TRANSFER && transfer_mem_valid) || (phase==EXCHANGE && exchange_mem_valid)) && !rst_i;
    assign mem_rsp_ready_o = gc_active_i ? gc_mem_ready : ((phase==WAIT_MEMORY) || (phase==TRANSFER && transfer_mem_rsp_ready) || (phase==EXCHANGE && exchange_mem_ready)) && !rst_i;
    assign dbg_maintenance_o=maintenance;
    assign dbg_class_valid_o=class_valid[dbg_class_code_i];
    assign dbg_compact_class_o=class_valid[dbg_class_code_i] ? classes[dbg_class_code_i] : 40'b0;
    assign dbg_vr_o=vr[dbg_vr_i];
    assign dbg_idx_o=idx; assign dbg_reg_o=idxreg;
    assign dbg_prepared_ref_o=prepared_access.reference;
    assign dbg_prepared_index_o=prepared_access.index;
    assign dbg_prepared_address_o=prepared_access.address;
    assign dbg_prepared_status_o=prepared_access.status;
    assign dbg_selected_o=selected;
    assign dbg_ref_o=snapshot[39:0]; assign dbg_size_o=snapshot[63:40];
    assign dbg_class_o=snapshot[103:64]; assign dbg_base_o=snapshot[127:104];
    assign dbg_repr_o=snapshot[167:128]; assign dbg_flags_o=snapshot[170:168];
    wire [170:0] debug_entry = valid[dbg_slot_i] ? entries[dbg_slot_i] : 171'b0;
    assign dbg_valid_o=valid[dbg_slot_i];
    assign dbg_entry_ref_o=debug_entry[39:0]; assign dbg_entry_size_o=debug_entry[63:40];
    assign dbg_entry_class_o=debug_entry[103:64]; assign dbg_entry_base_o=debug_entry[127:104];
    assign dbg_entry_repr_o=debug_entry[167:128]; assign dbg_entry_flags_o=debug_entry[170:168];

    // Scanning policy is intentionally irrelevant to reference syntax.
    /* verilator lint_off UNUSEDSIGNAL */
    function automatic is_ref(input [39:0] value);
        is_ref = value[39:38]==2'b10 && value[36:0]!=0;
    endfunction
    /* verilator lint_on UNUSEDSIGNAL */
    // Compact values decode without touching pager memory. Stored references
    // must match every tag/identity bit, not merely the direct-mapped slot index.
    // Reusing this lookup for the selected reference rejects stale snapshots
    // after a service replacement or collision.
    task automatic lookup(input [39:0] key, output [170:0] result, output [3:0] error);
        logic [5:0] code;
        begin
            result=0; error=OK; code=key[37:32];
            if(key[39:38]==2'b11) begin
                if(code>3 || (code==0 && key[31:0]!=0) || (code==1 && key[31:0]>1)) error=BAD_VALUE;
                else if(!class_valid[code[1:0]]) error=BAD_VALUE;
                else result={3'b0,8'b0,key[31:0],24'b0,classes[code[1:0]],24'b0,key};
            end else if(!is_ref(key)) error=INVALID_REFERENCE;
            else if(!valid[key[PAGER_BITS-1:0]]) error=NOT_RESIDENT;
            else if(entries[key[PAGER_BITS-1:0]][39:0]!=key) error=NOT_RESIDENT;
            else result=entries[key[PAGER_BITS-1:0]];
        end
    endtask

    logic [3:0] error, old_error, page_error;
    logic [170:0] old_entry, page_entry, next_snapshot;
    logic [170:0] memory_entry, next_memory_entry;
    logic [3:0] memory_error;
    logic signed [39:0] memory_index;
    logic next_selected, need_memory;
    logic [39:0] result_data, page_key;
    logic signed [40:0] next_idx_wide, next_reg_wide;
    logic [64:0] address_wide;
    logic needs_old;
    // Ordered validation and candidate-state construction. The first failing
    // check wins; later checks must not replace its status. Evaluate old-entry
    // metadata separately from a new pager selection so combined fields keep
    // their old-state semantics. Bounds arithmetic widens before truncation.
    // No side effects occur until every required check passes.
    always_comb begin
        lookup(snapshot[39:0],old_entry,old_error);
        if(!selected) old_error=NO_SELECTION;
        if(prepared_i) lookup(prepared_access.reference,memory_entry,memory_error);
        else begin memory_entry=old_entry; memory_error=old_error; end
        memory_index=prepared_i ? prepared_access.index : idx;
        next_memory_entry=memory_entry;
        next_prepared=prepared_access;
        error=OK; page_error=OK; page_entry=0; page_key=data_i;
        next_snapshot=snapshot; next_selected=selected;
        next_idx_wide={idx[39],idx}; next_reg_wide={idxreg[39],idxreg};
        result_data=NIL; need_memory=0; address_wide=0;
        needs_old=(read_i>=3 && read_i<=6) || read_i==9 || check_type_i || (memory_i!=0 && !prepared_i);
        if(pager_i>9 || index_i>9 || register_i>4 || memory_i>2 || read_i>11 ||
           (pager_i!=0 && memory_i!=0 && !prepared_i) || (pager_i!=0 && index_i==8) || (memory_i!=0 && read_i!=0) || (prepared_i && memory_i==0)) error=BAD_COMMAND;
        else if((pager_i>=5 && pager_i<=9) && (index_i!=0 || register_i!=0 || memory_i!=0 || read_i!=0 || load_vr_i || check_type_i || prepare_i || prepared_i)) error=BAD_COMMAND;
        else if((pager_i==8 || pager_i==9) && !is_ref(data_i) && data_i[39:37]!=0) error=BAD_VALUE;
        else if(check_type_i && !is_ref(expected_type_i)) error=BAD_VALUE;
        else if(pager_i==6 && !is_ref(data_i)) error=INVALID_REFERENCE;
        else if(pager_i==7 && (!is_ref(data_i) || !is_ref(vr[vr_i]))) error=INVALID_REFERENCE;
        else if(pager_i==7 && data_i[37]!=vr[vr_i][37]) error=BAD_VALUE;
        if(error==OK) begin
            case(index_i)
                1: next_idx_wide={data_i[39],data_i};
                2: next_idx_wide=0;
                3: next_idx_wide=1;
                4: next_idx_wide=2;
                5: next_idx_wide=$signed({idx[39],idx})+41'sd1;
                6: next_idx_wide=$signed({idx[39],idx})-41'sd1;
                7: next_idx_wide=$signed({idx[39],idx})+$signed({data_i[39],data_i});
                8: begin
                    if(old_error!=OK) error=old_error;
                    else if(old_entry[63:40]==0 || idx<0 || $unsigned(idx)>{16'b0,old_entry[63:40]}) error=BOUNDS;
                    else begin
                        next_snapshot=old_entry;
                        next_idx_wide=($unsigned(idx)=={16'b0,old_entry[63:40]}) ? 41'sd1 : $signed({idx[39],idx})+41'sd1;
                    end
                end
                9: next_idx_wide={idxreg[39],idxreg};
                default: begin end
            endcase
            if(error==OK && (next_idx_wide>41'sd549755813887 || next_idx_wide < -41'sd549755813888)) error=INDEX_OVERFLOW;
        end
        if(error==OK) begin
            case(register_i)
                1: next_reg_wide={data_i[39],data_i};
                2: next_reg_wide=$signed({idxreg[39],idxreg})+41'sd1;
                3: next_reg_wide=$signed({idxreg[39],idxreg})-41'sd1;
                4: next_reg_wide={idx[39],idx};
                default: begin end
            endcase
            if(next_reg_wide>41'sd549755813887 || next_reg_wide < -41'sd549755813888) error=INDEX_OVERFLOW;
        end
        if(error==OK && pager_i!=0 && pager_i<=5) begin
            case(pager_i)
                2: page_key=vr[vr_i];
                3: begin page_key=old_entry[103:64]; if(old_error!=OK) error=old_error; end
                4: begin page_key=old_entry[167:128]; if(old_error!=OK) error=old_error; end
                default: begin end
            endcase
            lookup(page_key,page_entry,page_error);
            if(error==OK) error=page_error;
            if(error==OK) begin next_snapshot=page_entry; next_selected=1; result_data=page_key; end
        end
        if(error==OK && needs_old) begin
            if(old_error!=OK) error=old_error;
            else begin
                if(pager_i==0) next_snapshot=old_entry;
                if(check_type_i && expected_type_i!=old_entry[103:64]) error=TYPE_ERROR;
            end
        end
        if(error==OK) begin
            case(read_i)
                1: result_data=vr[vr_i];
                2: begin if(!selected) error=NO_SELECTION; else result_data=snapshot[39:0]; end
                3: result_data={16'b0,old_entry[63:40]};
                4: result_data=old_entry[103:64];
                5: result_data={16'b0,old_entry[127:104]};
                6: result_data=old_entry[167:128];
                7: result_data=idx;
                8: result_data=idxreg;
                9: result_data={37'b0,old_entry[170:168]};
                10: result_data={15'b0,free_words};
                11: result_data={2'b0,free_identities};
                default: begin end
            endcase
        end
        if(prepare_i) begin
            next_prepared.reference=selected ? snapshot[39:0] : NIL;
            next_prepared.index=next_idx_wide[39:0];
            {next_prepared.address,next_prepared.status}=prepare_address(next_idx_wide[39:0],old_entry[127:104],old_entry[63:40],old_error);
        end
        if(error==OK && memory_i!=0) begin
            address_wide=prepared_i ? {41'b0,prepared_access.address} : {41'b0,memory_entry[127:104]}+{25'b0,memory_index}-65'd1;
            if(prepared_i && prepared_access.status!=OK) error=prepared_access.status;
            else if(memory_error!=OK) error=memory_error;
            else if(memory_index<=0 || $unsigned(memory_index)>{16'b0,memory_entry[63:40]} || address_wide>=65'd16777216 || address_wide>=65'(MEMORY_WORDS)) error=BOUNDS;
            else if(memory_i==1 && memory_index==1) result_data=memory_entry[167:128];
            else begin
                need_memory=1;
                if(memory_i==2) begin
                    next_memory_entry[169]=1;
                    if(memory_index==1) next_memory_entry[167:128]=data_i;
                    if(next_selected && next_snapshot[39:0]==memory_entry[39:0]) next_snapshot=next_memory_entry;
                end
            end
        end
    end
    // Maintenance checks cover every published body. Raw writes cannot corrupt
    // live objects, and a recovered cursor cannot overlap a surviving body.
    logic svc_overlap, cursor_safe;
    logic [170:0] svc_entry;
    integer j;
    always_comb begin
        svc_entry={svc_flags_i,svc_repr_i,svc_base_i,svc_class_i,svc_size_i,svc_ref_i};
        svc_overlap=0;
        cursor_safe=svc_repr_i<=40'(MEMORY_WORDS);
        for(integer k=0;k<ENTRIES;k=k+1) begin
            if(valid[k] && ({1'b0,entries[k][127:104]}+{1'b0,entries[k][63:40]})>{1'b0,svc_repr_i[24:0]}) cursor_safe=0;
            if(valid[k] && svc_base_i>=entries[k][127:104] &&
               {1'b0,svc_base_i}<({1'b0,entries[k][127:104]}+{1'b0,entries[k][63:40]})) svc_overlap=1;
        end
    end
    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            phase<=IDLE; maintenance<=0; valid<=0; persistent<=0; class_valid<=0; selected<=0; snapshot<=0;
            prepared_access.reference<=NIL; prepared_access.index<=0; prepared_access.address<=0; prepared_access.status<=NO_SELECTION;
            pending_prepared<='0; pending_memory_entry<=0;
            idx<=0; idxreg<=0; rsp_status_o<=OK; rsp_data_o<=NIL;
            resident_mem_write_o<=0;resident_mem_addr_o<=0;resident_mem_data_o<=0;
            pending_idx<=0;pending_reg<=0;pending_selected<=0;pending_load_vr<=0;
            pending_mutator<=0;pending_write<=0;pending_vr<=0; directory_vr<=0;pending_data<=0;pending_snapshot<=0;pending_slot<=0; saved_victim_slot<=0; saved_edges<=0; saved_scan<=0; exchange_edges<='0;
            for(j=0;j<8;j=j+1) vr[j]<=NIL;
            for(j=0;j<4;j=j+1) classes[j]<=0;
        end else begin
            // A committed disk object may name NEW objects that remain only in
            // RAM. Retain those identities even after this victim disappears.
            // Edges are accumulated privately and become roots only on save
            // commit. Opaque payloads contribute no edges; classes always do.
            if(phase==TRANSFER && store_valid_o && store_ready_i && store_op_o==2) begin
                saved_scan<=entries[saved_victim_slot][37];
                for(integer k=0;k<ENTRIES;k=k+1)
                    saved_edges[k]<=valid[k] && entries[k][39:0]==store_class_o;
            end
            if(phase==TRANSFER && store_valid_o && store_ready_i && store_op_o==3 && saved_scan)
                for(integer k=0;k<ENTRIES;k=k+1)
                    if(valid[k] && entries[k][39:0]==store_data_o) saved_edges[k]<=1;
            if(saved_victim) begin
                persistent<=persistent | saved_edges;
                persistent[saved_victim_slot]<=1;
            end
            if(start_exchange) exchange_edges<='0;
            if(phase==EXCHANGE && store_valid_o && store_ready_i) begin
                for(integer k=0;k<ENTRIES;k=k+1) begin
                    if(valid[k] && ((store_op_o==2 && entries[k][39:0]==store_class_o) ||
                        (store_op_o==3 && store_ref_o[37] && entries[k][39:0]==store_data_o)))
                        exchange_edges[k]<=1;
                end
            end
            if(gc_commit) begin
                // A valid prepared reference is a root. Relocate its address
                // with the pager; leave deferred bounds/selection errors intact.
                if(prepared_access.status==OK) begin
                    if(gc_retained[prepared_access.reference[PAGER_BITS-1:0]])
                        prepared_access.address<=gc_bases[prepared_access.reference[PAGER_BITS-1:0]*24 +: 24]+prepared_access.index[23:0]-24'd1;
                    else prepared_access.status<=NOT_RESIDENT;
                end
                valid<=valid & gc_retained;
                for(integer k=0;k<ENTRIES;k=k+1)
                    if(valid[k] && gc_retained[k]) entries[k][127:104]<=gc_bases[k*24 +: 24];
                if(selected && is_ref(snapshot[39:0]) && valid[snapshot[PAGER_BITS-1:0]] &&
                    entries[snapshot[PAGER_BITS-1:0]][39:0]==snapshot[39:0]) begin
                    if(gc_retained[snapshot[PAGER_BITS-1:0]]) snapshot[127:104]<=gc_bases[snapshot[PAGER_BITS-1:0]*24 +: 24];
                    else selected<=0;
                end
            end
            case(phase)
                IDLE: begin
                    if(cmd_valid_i && cmd_ready_o) begin
                        rsp_status_o<=error; rsp_data_o<=(error==OK) ? result_data : NIL;
                        if(start_directory) begin phase<=DIRECTORY; directory_vr<=vr_i; end
                        else if(start_exchange) phase<=EXCHANGE;
                        else if(start_transfer) begin phase<=TRANSFER; saved_victim_slot<=transfer_slot; end
                        else if(error!=OK) phase<=RESPONSE;
                        else if(need_memory) begin
                            pending_idx<=next_idx_wide[39:0];pending_reg<=next_reg_wide[39:0];
                            pending_snapshot<=next_snapshot;pending_selected<=next_selected;
                            pending_prepared<=next_prepared; pending_memory_entry<=next_memory_entry;
                            pending_load_vr<=load_vr_i;pending_vr<=vr_i;pending_data<=data_i;
                            pending_mutator<=1;pending_write<=memory_i==2;
                            pending_slot<=memory_entry[PAGER_BITS-1:0];
                            resident_mem_addr_o<=address_wide[23:0];resident_mem_data_o<=data_i;resident_mem_write_o<=memory_i==2;
                            phase<=REQUEST;
                        end else begin
                            idx<=next_idx_wide[39:0];idxreg<=next_reg_wide[39:0];
                            snapshot<=next_snapshot;selected<=next_selected;
                            prepared_access<=next_prepared;
                            if(load_vr_i) vr[vr_i]<=data_i;
                            phase<=RESPONSE;
                        end
                    end else if(svc_valid_i && svc_ready_o) begin
                        rsp_status_o<=OK;rsp_data_o<=NIL;phase<=RESPONSE;
                        case(svc_op_i)
                            0: begin
                                if(!is_ref(svc_ref_i) || !is_ref(svc_class_i)) rsp_status_o<=INVALID_REFERENCE;
                                else if(svc_size_i==0 && svc_repr_i!=NIL) rsp_status_o<=BAD_VALUE;
                                else begin
                                    entries[svc_ref_i[PAGER_BITS-1:0]]<=svc_entry;
                                    if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==svc_ref_i[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                                    valid[svc_ref_i[PAGER_BITS-1:0]]<=1;
                                    persistent[svc_ref_i[PAGER_BITS-1:0]]<=!svc_flags_i[0];
                                    if(selected && snapshot[39:0]==svc_ref_i) snapshot<=svc_entry;
                                end
                            end
                            1: begin
                                if(!is_ref(svc_ref_i)) rsp_status_o<=INVALID_REFERENCE;
                                else if(!valid[svc_ref_i[PAGER_BITS-1:0]] || entries[svc_ref_i[PAGER_BITS-1:0]][39:0]!=svc_ref_i) rsp_status_o<=NOT_RESIDENT;
                                else begin
                                    valid[svc_ref_i[PAGER_BITS-1:0]]<=0;
                                    if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==svc_ref_i[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                                end
                            end
                            2: begin
                                if(svc_code_i>3 || !is_ref(svc_class_i)) rsp_status_o<=BAD_VALUE;
                                else begin
                                    classes[svc_code_i[1:0]]<=svc_class_i;class_valid[svc_code_i[1:0]]<=1;
                                    if(selected && snapshot[39:38]==2'b11 && snapshot[37:32]==svc_code_i) snapshot[103:64]<=svc_class_i;
                                end
                            end
                            3: begin
                                if({8'b0,svc_base_i}>=MEMORY_WORDS) rsp_status_o<=BOUNDS;
                                else if(svc_overlap) rsp_status_o<=BAD_COMMAND;
                                else begin
                                    pending_mutator<=0;pending_write<=1;
                                    resident_mem_addr_o<=svc_base_i;resident_mem_data_o<=svc_repr_i;resident_mem_write_o<=1;
                                    phase<=REQUEST;
                                end
                            end
                            4: begin
                                if({8'b0,svc_base_i}>=MEMORY_WORDS) rsp_status_o<=BOUNDS;
                                else begin
                                    pending_mutator<=0;pending_write<=0;
                                    resident_mem_addr_o<=svc_base_i;resident_mem_data_o<=0;resident_mem_write_o<=0;
                                    phase<=REQUEST;
                                end
                            end
                            5: begin
                                if(maintenance) rsp_status_o<=BAD_COMMAND;
                                else maintenance<=1;
                            end
                            6: begin
                                if(!maintenance) rsp_status_o<=BAD_COMMAND;
                                else if(!cursor_safe) rsp_status_o<=BOUNDS;
                            end
                            7: begin
                                if(!maintenance) rsp_status_o<=BAD_COMMAND;
                                else maintenance<=0;
                            end
                            8: if(svc_repr_i==0 || svc_repr_i>40'h2000000000)
                                rsp_status_o<=BAD_VALUE;
                            default: rsp_status_o<=BAD_COMMAND;
                        endcase
                    end
                end
                REQUEST: if(mem_ready_i) phase<=WAIT_MEMORY;
                // Commit the private retirement record only after successful
                // memory completion. First-field writes publish both the body
                // value and its cached representation on the same logical commit.
                WAIT_MEMORY: if(mem_rsp_valid_i) begin
                    rsp_status_o<=mem_rsp_error_i ? MEMORY_ERROR : OK;
                    rsp_data_o<=NIL;
                    if(!mem_rsp_error_i && pending_mutator) begin
                        idx<=pending_idx;idxreg<=pending_reg;snapshot<=pending_snapshot;selected<=pending_selected;
                        prepared_access<=pending_prepared;
                        if(pending_load_vr) vr[pending_vr]<=pending_data;
                        if(pending_write) begin
                            entries[pending_slot]<=pending_memory_entry;
                            rsp_data_o<=pending_data;
                        end else rsp_data_o<=mem_rsp_data_i;
                    end
                    if(!mem_rsp_error_i && !pending_mutator && !pending_write) rsp_data_o<=mem_rsp_data_i;
                    phase<=RESPONSE;
                end
                // Publication is one complete metadata write plus valid bit.
                // A failed transfer leaves the old slot and selection untouched;
                // transfer reservations and completed unpublished writes may remain.
                DIRECTORY: if(directory_done) begin
                    rsp_status_o<=directory_status;
                    rsp_data_o<=directory_status==OK ? directory_ref : NIL;
                    if(directory_status==OK) vr[directory_vr]<=directory_class;
                    phase<=RESPONSE;
                end
                TRANSFER: if(transfer_done) begin
                    rsp_status_o<=transfer_status;
                    rsp_data_o<=(transfer_status==OK) ? transfer_entry[39:0] : NIL;
                    if(transfer_status==OK) begin
                        if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==transfer_entry[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                        entries[transfer_entry[PAGER_BITS-1:0]]<=transfer_entry;
                        valid[transfer_entry[PAGER_BITS-1:0]]<=1;
                        persistent[transfer_entry[PAGER_BITS-1:0]]<=!transfer_entry[168];
                        snapshot<=transfer_entry;selected<=1;
                    end
                    phase<=RESPONSE;
                end
                EXCHANGE: if(exchange_done) begin
                    rsp_status_o<=exchange_status;
                    rsp_data_o<=exchange_status==OK ? exchange_first : NIL;
                    if(exchange_status==OK && exchange_first!=exchange_second) begin
                        if(prepared_access.status==OK && (prepared_access.reference==exchange_first || prepared_access.reference==exchange_second)) prepared_access.status<=NOT_RESIDENT;
                        persistent<=persistent | exchange_edges;
                        for(integer k=0;k<ENTRIES;k=k+1)
                            if(valid[k] && (entries[k][39:0]==exchange_first || entries[k][39:0]==exchange_second))
                                valid[k]<=0;
                        if(selected && (snapshot[39:0]==exchange_first || snapshot[39:0]==exchange_second))
                            selected<=0;
                    end
                    phase<=RESPONSE;
                end
                RESPONSE: if(rsp_ready_i) phase<=IDLE;
                default: phase<=IDLE;
            endcase
        end
    end
endmodule

// OBJEKT resident operations, service access, and transfer publication.
//
// The command evaluator computes a candidate state from PRE-COMMAND values.
// Ordinary memory access uses the old index. PREPARE explicitly forwards the
// new index into an address/check register; PREPARED accesses the old contents
// of that register while selection and the next address can change independently.
// See docs/interface.md for precedence and mapping invalidation.
//
// Resident operations retire either on execution (no external memory) or on
// successful memory completion. The pending_* registers keep candidate state
// private during a stall. An error discards it. The response state holds the
// result until consumed and never reexecutes the accepted command.
//
// objekt_transfer owns reservations and streaming save/refill. This parent owns
// the pager and is the only publisher of a completed transfer entry. Recovery
// uses either the machine collector port or the halted test service port;
// both exclude mutator commands while they own maintenance.
//
// The pager table is a two-port RAM with registered reads, so it maps to FPGA
// block RAM. Each word holds an entry with its validity and persistence bits.
// Every stored reference lives in the slot named by its low identity bits, so
// a search for a reference is a single read.
//
// The selected and prepared entries are kept in registers and are
// authoritative: commands on those objects read no table port and keep their
// timing. A command that pages a key presents its slot on acceptance and
// executes one cycle later, in LOOKUP. Operations that inspect the whole table
// walk it one slot per cycle, presenting each slot a cycle ahead.
module objekt #(
    parameter integer PAGER_BITS = 4,
    parameter integer MEMORY_WORDS = 512,
    // The debug slot port reads the table combinationally. Leave it out of a
    // hardware build: an unregistered port keeps the table out of block RAM.
    parameter bit DEBUG_PORT = 1
)(
    input logic clk_i, input logic rst_i, input logic run_i,
    input logic gc_enable_i, gc_active_i, gc_explicit_i,
    input logic gc_valid_i, output logic gc_ready_o, input logic [3:0] gc_operation_i,
    input logic [39:0] gc_data_i, gc_root_i,
    output logic gc_response_o, input logic gc_response_ready_i,
    output logic [3:0] gc_status_o, output logic [39:0] gc_result_o,
    output logic gc_committed_o,
    // High while free words or free identities are below the threshold
    // that a threshold command loaded. Registered: it follows a change in
    // either count or threshold by one cycle.
    output logic low_space_o,
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
    typedef enum logic [3:0] {CLEAR, IDLE, LOOKUP, REQUEST, WAIT_MEMORY, RESPONSE, TRANSFER,
        EXCHANGE, EXCHANGE_FINISH, SERVICE_LOOKUP, SERVICE_SCAN, DIRECTORY_SCAN, DIRECTORY_START,
        DIRECTORY, ALLOCATE_SCAN} resident_state_t;
    resident_state_t phase;
    logic [3:0] response_status;
    logic [39:0] response_data;
    // Forward a resident RAM completion to the response channel on the same
    // edge that commits it. If the consumer stalls, retain exactly that reply
    // in RESPONSE; memory completion and architectural effects happen once.
    wire resident_completion = phase==WAIT_MEMORY && mem_rsp_valid_i;
    assign rsp_status_o = resident_completion ? (mem_rsp_error_i ? MEMORY_ERROR : OK) : response_status;
    assign rsp_data_o = resident_completion
        ? (mem_rsp_error_i ? NIL : (pending_write ? (pending_mutator ? pending_data : NIL) : mem_rsp_data_i))
        : response_data;

    // ---------------------------------------------------------------------
    // The pager table. Word: {valid, persistent, entry}, where the entry is
    // {cond, modified, new, representation, base, class, size, reference}.
    // Port A writes, and reads the slot of a paged key. Port B reads the
    // auxiliary slot. Both reads are registered.
    // ---------------------------------------------------------------------
    localparam integer WORD = 173;
    logic [WORD-1:0] entries [0:ENTRIES-1];
    logic [WORD-1:0] a_q, b_q, a_data;
    logic a_write;
    logic [PAGER_BITS-1:0] a_slot, b_slot;
    always_ff @(posedge clk_i) begin
        if(a_write) entries[a_slot]<=a_data;
        a_q<=entries[a_slot];
    end
    always_ff @(posedge clk_i) b_q<=entries[b_slot];
    wire [170:0] page_stored=a_q[170:0];
    wire page_stored_valid=a_q[172];
    wire [170:0] aux_stored=b_q[170:0];
    wire aux_persistent=b_q[171];
    wire aux_valid=b_q[172];
    wire [24:0] aux_end={1'b0,aux_stored[127:104]}+{1'b0,aux_stored[63:40]};

    logic [39:0] classes [0:3];
    logic [3:0] class_valid;
    logic [39:0] vr [0:7];
    logic signed [39:0] idx, idxreg;
    logic selected, maintenance;
    // The selected and prepared entries are authoritative copies. Every table
    // write that touches their slot updates or invalidates them on the same
    // edge, so commands on those objects read no table port.
    logic [170:0] snapshot, prepared_entry;
    logic selected_live, selected_persistent, prepared_persistent;
    // Capacity reads use the same cursor/limit as allocation. They require no
    // selected object or memory transfer and report pre-command values. The
    // inactive semispace is collector reserve, not currently allocatable RAM.
    wire [24:0] allocation_limit = gc_enable_i
        ? (gc_space ? 25'(MEMORY_WORDS) : 25'(MEMORY_WORDS/2)) : 25'(MEMORY_WORDS);
    wire [24:0] free_words = dbg_body_cursor_o < allocation_limit
        ? allocation_limit - dbg_body_cursor_o : 25'd0;
    wire [37:0] free_identities = 38'h2000000000 - dbg_next_identity_o;
    // Low-space thresholds; zero disables one.
    logic [39:0] word_threshold, identity_threshold;
    always_ff @(posedge clk_i) low_space_o <= !rst_i &&
        ({15'b0,free_words} < word_threshold || {2'b0,free_identities} < identity_threshold);

    // A command that pages a key waits one cycle for the table. Its fields
    // are held here, since the issuer may change them after acceptance.
    logic [3:0] held_pager, held_index, held_read;
    logic [2:0] held_register, held_vr;
    logic [1:0] held_memory;
    logic held_prepare, held_prepared, held_load_vr, held_alloc_scan, held_check_type;
    logic [23:0] held_alloc_size;
    logic [39:0] held_data, held_expected_type;
    wire looking=phase==LOOKUP;
    wire [3:0] c_pager=looking ? held_pager : pager_i;
    wire [3:0] c_index=looking ? held_index : index_i;
    wire [3:0] c_read=looking ? held_read : read_i;
    wire [2:0] c_register=looking ? held_register : register_i;
    wire [2:0] c_vr=looking ? held_vr : vr_i;
    wire [1:0] c_memory=looking ? held_memory : memory_i;
    wire c_prepare=looking ? held_prepare : prepare_i;
    wire c_prepared=looking ? held_prepared : prepared_i;
    wire c_load_vr=looking ? held_load_vr : load_vr_i;
    wire c_alloc_scan=looking ? held_alloc_scan : alloc_scan_i;
    wire c_check_type=looking ? held_check_type : check_type_i;
    wire [23:0] c_alloc_size=looking ? held_alloc_size : alloc_size_i;
    wire [39:0] c_data=looking ? held_data : data_i;
    wire [39:0] c_expected_type=looking ? held_expected_type : expected_type_i;
    wire accept=phase==IDLE && cmd_valid_i && cmd_ready_o;
    // A command without a paged key executes on acceptance; others in LOOKUP.
    wire execute=(accept && pager_i==0) || looking;

    // Private retirement record for one resident memory operation. Inputs may
    // change immediately after acceptance; completion uses only these fields.
    logic signed [39:0] pending_idx, pending_reg;
    logic pending_selected, pending_load_vr, pending_mutator, pending_write;
    logic pending_live, pending_selected_persistent, pending_persistent, pending_prepared_persistent;
    logic [2:0] pending_vr;
    logic [39:0] pending_data;
    logic [170:0] pending_snapshot, pending_memory_entry, pending_prepared_entry;
    logic [PAGER_BITS-1:0] pending_slot;

    logic resident_mem_write_o;
    logic [23:0] resident_mem_addr_o;
    logic [39:0] resident_mem_data_o;
    wire transfer_mem_valid, transfer_mem_write, transfer_mem_rsp_ready, transfer_done;
    wire [23:0] transfer_mem_addr;
    wire [39:0] transfer_mem_data;
    wire [170:0] transfer_entry;
    wire [3:0] transfer_status;
    // Allocation takes the first identity at or after the allocator's next
    // one whose slot is empty, so it displaces a resident object only when
    // every slot is occupied. LOOKUP tests the next identity itself;
    // ALLOCATE_SCAN tests one further identity per cycle, having presented
    // its slot the cycle before. When all ENTRIES candidates are occupied the
    // probe wraps to the next identity, whose occupant is the victim.
    logic [PAGER_BITS-1:0] allocate_probe;
    logic allocate_evict;
    logic [39:0] allocate_class;
    logic [23:0] allocate_size;
    logic allocate_scan_tag;
    wire [37:0] allocate_candidate=dbg_next_identity_o+38'(allocate_probe);
    wire [PAGER_BITS-1:0] allocate_ahead=allocate_probe+1'b1;
    wire [PAGER_BITS-1:0] allocate_following=dbg_next_identity_o[PAGER_BITS-1:0]+allocate_ahead;

    logic [PAGER_BITS-1:0] walk_slot;
    wire walk_last=walk_slot==PAGER_BITS'(ENTRIES-1);
    wire [PAGER_BITS-1:0] walk_ahead=walk_slot+1'b1;
    // A stored record names a class and body words. Either can refer to a
    // resident object; its slot follows from the reference itself. The table
    // answers one cycle after the store accepts the request.
    wire [39:0] edge_key=store_op_o==2 ? store_class_o : store_data_o;
    logic edge_pending;
    logic [39:0] edge_key_q;
    wire edge_write=edge_pending && aux_valid && aux_stored[39:0]==edge_key_q;
    // Halted scans: a raw write must miss every published body, and a new
    // body cursor must lie at or above the end of every published body.
    logic scan_cursor, scan_clear;
    logic [24:0] scan_value;
    wire scan_conflict=aux_valid && (scan_cursor ? aux_end>scan_value :
        (resident_mem_addr_o>=aux_stored[127:104] && {1'b0,resident_mem_addr_o}<aux_end));
    wire scan_cursor_set=phase==SERVICE_SCAN && walk_last && scan_cursor && scan_clear && !scan_conflict;
    // Directory scan. FindObject reads the one slot its identity selects.
    // NextObject probes identity+1, identity+2, ... in slot order and keeps
    // the smallest resident identity above the request. A slot holding
    // exactly the probed identity ends the walk early: every later slot, and
    // every collision seen so far, holds a larger one.
    logic directory_next;
    logic [36:0] directory_identity;
    wire [37:0] directory_probe=directory_next ?
        {1'b0,directory_identity}+38'd1+38'(walk_slot) : {1'b0,directory_identity};
    wire [PAGER_BITS-1:0] directory_following=directory_identity[PAGER_BITS-1:0]+1'b1+walk_ahead;
    wire [PAGER_BITS-1:0] directory_first=c_data[PAGER_BITS-1:0]+PAGER_BITS'(c_pager==8);
    wire directory_exact=aux_valid && {1'b0,aux_stored[36:0]}==directory_probe;
    wire directory_better=aux_valid && aux_stored[36:0]>directory_identity &&
        (directory_best_ref==NIL || aux_stored[36:0]<directory_best_ref[36:0]);
    // Port B address: the slot whose entry the next cycle needs.
    always_comb begin
        if(gc_active_i) b_slot=gc_slot;
        else case(phase)
            TRANSFER, EXCHANGE: b_slot=edge_key[PAGER_BITS-1:0];
            // The first candidate beyond the allocator's next identity.
            LOOKUP: b_slot=c_pager==6 ? dbg_next_identity_o[PAGER_BITS-1:0]+1'b1 : directory_first;
            ALLOCATE_SCAN: b_slot=allocate_following;
            SERVICE_SCAN: b_slot=walk_ahead;
            DIRECTORY_SCAN: b_slot=directory_following;
            default: b_slot=!run_i ? ((svc_op_i==3 || svc_op_i==6) ? '0 : svc_ref_i[PAGER_BITS-1:0]) :
                pager_i==6 ? dbg_next_identity_o[PAGER_BITS-1:0] : vr[vr_i][PAGER_BITS-1:0];
        endcase
    end
    wire start_fetch=looking && c_pager==5 && error==NOT_RESIDENT;
    wire accept_allocate=looking && c_pager==6 && error==OK;
    // An exhausted identity goes straight to the engine, which reports it.
    wire allocate_now=accept_allocate && (!aux_valid || dbg_next_identity_o[37]);
    wire allocate_search=accept_allocate && !allocate_now;
    wire allocate_last=allocate_probe==PAGER_BITS'(ENTRIES-1);
    wire allocate_found=phase==ALLOCATE_SCAN && (allocate_evict || !aux_valid || allocate_candidate[37]);
    wire start_transfer=start_fetch || allocate_now || allocate_found;
    // With a collector, allocation into a full table first reports
    // OUT_OF_SPACE, so collection can drop unreachable entries instead of
    // saving a victim that may be garbage. Once a collection has committed,
    // allocation may evict, until it next finds an empty slot.
    logic eviction_permitted;
    wire allocate_refuse=phase==ALLOCATE_SCAN && allocate_evict && aux_valid &&
        gc_enable_i && !eviction_permitted;
    wire allocate_empty=(accept_allocate && !aux_valid && !dbg_next_identity_o[37]) ||
        (phase==ALLOCATE_SCAN && !allocate_evict && !aux_valid && !allocate_candidate[37]);
    wire [PAGER_BITS-1:0] transfer_slot=phase==ALLOCATE_SCAN ? allocate_candidate[PAGER_BITS-1:0] :
        c_pager==6 ? dbg_next_identity_o[PAGER_BITS-1:0] : c_data[PAGER_BITS-1:0];
    wire [170:0] transfer_victim=start_fetch ? page_stored : aux_stored;
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
    wire start_exchange=looking && c_pager==7 && error==OK;
    wire exchange_first_resident, exchange_second_resident;
    wire exchange_published=exchange_status==OK && exchange_first!=exchange_second;
    logic exchange_drop_second;
    logic [39:0] directory_best_ref, directory_best_class;
    wire directory_valid, directory_ready, directory_done;
    wire [3:0] directory_op, directory_status;
    wire [39:0] directory_request, directory_ref, directory_class;
    logic [2:0] directory_vr;
    wire start_directory=looking && (c_pager==8 || c_pager==9) && error==OK;
    // The scan above searches only resident metadata. The backend merges its
    // candidate with the ordered backing index without fetching object bodies.
    objekt_directory directory (
        .clk_i(clk_i),.rst_i(rst_i),.start_i(phase==DIRECTORY_START),.next_i(directory_next),.identity_i(directory_identity),
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
        .clk_i(clk_i),.rst_i(rst_i),.start_i(start_transfer),
        .allocate_i(phase==ALLOCATE_SCAN || c_pager==6),
        .target_i(phase==ALLOCATE_SCAN ? allocate_class : c_data),
        .size_i(phase==ALLOCATE_SCAN ? allocate_size : c_alloc_size),
        .scan_i(phase==ALLOCATE_SCAN ? allocate_scan_tag : c_alloc_scan),
        .identity_i(phase==ALLOCATE_SCAN ? allocate_candidate : dbg_next_identity_o),
        .victim_valid_i(start_fetch ? page_stored_valid : allocate_evict && aux_valid && !allocate_refuse),
        .victim_i(start_fetch ? page_stored : aux_stored),
        .seed_i(seed_transfer),.seed_ref_i(svc_ref_i),.seed_base_i(svc_base_i),.seed_size_i(svc_size_i),
        .cursor_set_i(gc_commit || scan_cursor_set),
        .cursor_i(gc_commit ? gc_cursor : scan_value),
        .identity_floor_valid_i(svc_valid_i && svc_ready_o && svc_op_i==8 &&
            svc_repr_i!=0 && svc_repr_i<=40'h2000000000),
        .identity_floor_i(svc_repr_i[37:0]),
        .allocation_limit_i(allocation_limit),.refuse_i(allocate_refuse),
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
        .first_i(c_data),.second_i(vr[c_vr]),
        .first_resident_i(page_stored_valid && page_stored[39:0]==c_data),
        .second_resident_i(aux_valid && aux_stored[39:0]==vr[c_vr]),
        .first_entry_i(page_stored),.second_entry_i(aux_stored),
        .done_o(exchange_done),.status_o(exchange_status),.first_o(exchange_first),.second_o(exchange_second),
        .first_resident_o(exchange_first_resident),.second_resident_o(exchange_second_resident),
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
    logic gc_relocate, gc_commit, gc_space, gc_retained, saved_victim, gc_ready;
    logic [PAGER_BITS-1:0] saved_victim_slot;
    logic [170:0] saved_victim_entry;
    assign gc_ready_o=gc_ready && phase==IDLE && !maintenance;
    logic [PAGER_BITS-1:0] gc_slot, gc_relocate_slot;
    logic [23:0] gc_base;
    logic [24:0] gc_cursor;
    logic [23:0] gc_needed;
    logic [39:0] gc_needed_class;
    logic victim_scan;
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
    end
    objekt_gc #(.PAGER_BITS(PAGER_BITS),.MEMORY_WORDS(MEMORY_WORDS)) collector_datapath (
        .clk_i(clk_i),.rst_i(rst_i),.active_i(gc_active_i),
        .valid_i(gc_valid_i && phase==IDLE && !maintenance),.ready_o(gc_ready),
        .operation_i(gc_operation_i),.data_i(gc_data_i),.root_i(gc_root),.needed_i(gc_explicit_i ? 24'b0 : gc_needed),
        .response_o(gc_response_o),.response_ready_i(gc_response_ready_i),
        .status_o(gc_status_o),.result_o(gc_result_o),.committed_o(gc_committed_o),
        .slot_o(gc_slot),.entry_valid_i(aux_valid),.entry_i(aux_stored),
        .entry_persistent_i(aux_persistent),
        .entry_prepared_i(prepared_access.status==OK && aux_stored[39:0]==prepared_access.reference),
        .relocate_o(gc_relocate),.commit_o(gc_commit),.relocate_slot_o(gc_relocate_slot),
        .retained_o(gc_retained),.base_o(gc_base),.cursor_o(gc_cursor),
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
    assign rsp_valid_o = (phase==RESPONSE || resident_completion) && !rst_i;
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
    // The debug slot reads the table combinationally, for simulation only: a
    // third, unregistered port would keep the table out of block RAM.
    /* verilator lint_off UNUSEDSIGNAL */
    logic [WORD-1:0] debug_word;
    /* verilator lint_on UNUSEDSIGNAL */
    if(DEBUG_PORT) begin : debug_port
        assign debug_word=entries[dbg_slot_i];
    end else begin : no_debug_port
        assign debug_word='0;
        wire unused_debug_slot=&{1'b0,dbg_slot_i};
    end
    wire [170:0] debug_entry = debug_word[172] ? debug_word[170:0] : 171'b0;
    assign dbg_valid_o=debug_word[172];
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
    // Reusing this lookup for the selected reference rejects a snapshot whose
    // object has left its slot.
    task automatic lookup(input [39:0] key, input [170:0] stored, input stored_valid,
        output [170:0] result, output [3:0] error);
        logic [5:0] code;
        begin
            result=0; error=OK; code=key[37:32];
            if(key[39:38]==2'b11) begin
                if(code>3 || (code==0 && key[31:0]!=0) || (code==1 && key[31:0]>1)) error=BAD_VALUE;
                else if(!class_valid[code[1:0]]) error=BAD_VALUE;
                else result={3'b0,8'b0,key[31:0],24'b0,classes[code[1:0]],24'b0,key};
            end else if(!is_ref(key)) error=INVALID_REFERENCE;
            else if(!stored_valid) error=NOT_RESIDENT;
            else if(stored[39:0]!=key) error=NOT_RESIDENT;
            else result=stored;
        end
    endtask

    logic [3:0] error, old_error, page_error;
    logic [170:0] old_entry, page_entry, next_snapshot;
    logic [170:0] memory_entry, next_memory_entry, next_prepared_entry;
    logic [3:0] memory_error;
    logic signed [39:0] memory_index;
    logic next_selected, need_memory, next_live, next_selected_persistent, next_prepared_persistent;
    logic memory_persistent;
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
        lookup(snapshot[39:0],snapshot,selected_live,old_entry,old_error);
        if(!selected) old_error=NO_SELECTION;
        if(c_prepared) begin
            lookup(prepared_access.reference,prepared_entry,prepared_access.status==OK,memory_entry,memory_error);
            memory_persistent=prepared_persistent;
        end else begin
            memory_entry=old_entry; memory_error=old_error; memory_persistent=selected_persistent;
        end
        // The paged key selects its slot before any check. Allocation and
        // exchange read the same port for their victim and first operand.
        case(c_pager)
            2: page_key=vr[c_vr];
            3: page_key=old_entry[103:64];
            4: page_key=old_entry[167:128];
            default: page_key=c_data;
        endcase
        memory_index=c_prepared ? prepared_access.index : idx;
        next_memory_entry=memory_entry;
        next_prepared=prepared_access;
        next_prepared_entry=prepared_entry; next_prepared_persistent=prepared_persistent;
        error=OK; page_error=OK; page_entry=0;
        next_snapshot=snapshot; next_selected=selected;
        next_live=selected_live; next_selected_persistent=selected_persistent;
        next_idx_wide={idx[39],idx}; next_reg_wide={idxreg[39],idxreg};
        result_data=NIL; need_memory=0; address_wide=0;
        needs_old=(c_read>=3 && c_read<=6) || c_read==9 || c_check_type || (c_memory!=0 && !c_prepared);
        if(c_pager>9 || c_index>9 || c_register>6 || c_memory>2 || c_read>11 ||
           (c_register>4 && (c_pager!=0 || c_index!=0 || c_memory!=0 || c_read!=0 || c_load_vr || c_check_type || c_prepare || c_prepared)) ||
           (c_pager!=0 && c_memory!=0 && !c_prepared) || (c_pager!=0 && c_index==8) || (c_memory!=0 && c_read!=0) || (c_prepared && c_memory==0)) error=BAD_COMMAND;
        else if((c_pager>=5 && c_pager<=9) && (c_index!=0 || c_register!=0 || c_memory!=0 || c_read!=0 || c_load_vr || c_check_type || c_prepare || c_prepared)) error=BAD_COMMAND;
        else if((c_pager==8 || c_pager==9) && !is_ref(c_data) && c_data[39:37]!=0) error=BAD_VALUE;
        else if(c_check_type && !is_ref(c_expected_type)) error=BAD_VALUE;
        else if(c_pager==6 && !is_ref(c_data)) error=INVALID_REFERENCE;
        else if(c_pager==7 && (!is_ref(c_data) || !is_ref(vr[c_vr]))) error=INVALID_REFERENCE;
        else if(c_pager==7 && c_data[37]!=vr[c_vr][37]) error=BAD_VALUE;
        if(error==OK) begin
            case(c_index)
                1: next_idx_wide={c_data[39],c_data};
                2: next_idx_wide=0;
                3: next_idx_wide=1;
                4: next_idx_wide=2;
                5: next_idx_wide=$signed({idx[39],idx})+41'sd1;
                6: next_idx_wide=$signed({idx[39],idx})-41'sd1;
                7: next_idx_wide=$signed({idx[39],idx})+$signed({c_data[39],c_data});
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
            case(c_register)
                1: next_reg_wide={c_data[39],c_data};
                2: next_reg_wide=$signed({idxreg[39],idxreg})+41'sd1;
                3: next_reg_wide=$signed({idxreg[39],idxreg})-41'sd1;
                4: next_reg_wide={idx[39],idx};
                default: begin end
            endcase
            if(next_reg_wide>41'sd549755813887 || next_reg_wide < -41'sd549755813888) error=INDEX_OVERFLOW;
        end
        if(error==OK && c_pager!=0 && c_pager<=5) begin
            if((c_pager==3 || c_pager==4) && old_error!=OK) error=old_error;
            lookup(page_key,page_stored,page_stored_valid,page_entry,page_error);
            if(error==OK) error=page_error;
            if(error==OK) begin
                next_snapshot=page_entry; next_selected=1; result_data=page_key;
                next_live=1; next_selected_persistent=page_key[39:38]==2'b10 && a_q[171];
            end
        end
        if(error==OK && needs_old) begin
            if(old_error!=OK) error=old_error;
            else begin
                if(c_pager==0) next_snapshot=old_entry;
                if(c_check_type && c_expected_type!=old_entry[103:64]) error=TYPE_ERROR;
            end
        end
        if(error==OK) begin
            case(c_read)
                1: result_data=vr[c_vr];
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
        if(c_prepare) begin
            next_prepared.reference=selected ? snapshot[39:0] : NIL;
            next_prepared.index=next_idx_wide[39:0];
            {next_prepared.address,next_prepared.status}=prepare_address(next_idx_wide[39:0],old_entry[127:104],old_entry[63:40],old_error);
            next_prepared_entry=old_entry; next_prepared_persistent=selected_persistent;
        end
        if(error==OK && c_memory!=0) begin
            address_wide=c_prepared ? {41'b0,prepared_access.address} : {41'b0,memory_entry[127:104]}+{25'b0,memory_index}-65'd1;
            if(c_prepared && prepared_access.status!=OK) error=prepared_access.status;
            else if(memory_error!=OK) error=memory_error;
            else if(memory_index<=0 || $unsigned(memory_index)>{16'b0,memory_entry[63:40]} || address_wide>=65'd16777216 || address_wide>=65'(MEMORY_WORDS)) error=BOUNDS;
            else if(c_memory==1 && memory_index==1) result_data=memory_entry[167:128];
            else begin
                need_memory=1;
                if(c_memory==2) begin
                    next_memory_entry[169]=1;
                    if(memory_index==1) next_memory_entry[167:128]=c_data;
                    if(next_selected && next_snapshot[39:0]==memory_entry[39:0]) next_snapshot=next_memory_entry;
                end
            end
        end
    end
    wire [170:0] svc_entry={svc_flags_i,svc_repr_i,svc_base_i,svc_class_i,svc_size_i,svc_ref_i};
    integer j;
    // The single table write port. Each writer stores a complete word in the
    // slot named by that entry's own reference; the writers are exclusive.
    // Dropping an entry writes a zero word.
    logic [PAGER_BITS-1:0] clear_slot;
    wire memory_commit=phase==WAIT_MEMORY && mem_rsp_valid_i && !mem_rsp_error_i && pending_mutator && pending_write;
    wire publish=phase==TRANSFER && transfer_done && transfer_status==OK;
    wire invalidate=phase==SERVICE_LOOKUP && aux_valid && aux_stored[39:0]==held_data;
    always_comb begin
        a_write=0; a_slot=page_key[PAGER_BITS-1:0]; a_data='0;
        if(phase==CLEAR) begin
            a_write=1; a_slot=clear_slot;
        end else if(gc_relocate) begin
            a_write=1; a_slot=gc_relocate_slot;
            if(aux_valid && gc_retained) a_data={1'b1,aux_persistent,aux_stored[170:128],gc_base,aux_stored[103:0]};
        end else if(edge_write) begin
            a_write=1; a_slot=edge_key_q[PAGER_BITS-1:0]; a_data={2'b11,aux_stored};
        end else if(saved_victim) begin
            a_write=1; a_slot=saved_victim_slot; a_data={2'b11,saved_victim_entry};
        end else if(seed_transfer) begin
            a_write=1; a_slot=svc_ref_i[PAGER_BITS-1:0]; a_data={1'b1,!svc_flags_i[0],svc_entry};
        end else if(invalidate) begin
            a_write=1; a_slot=held_data[PAGER_BITS-1:0];
        end else if(memory_commit) begin
            a_write=1; a_slot=pending_slot; a_data={1'b1,pending_persistent,pending_memory_entry};
        end else if(publish) begin
            a_write=1; a_slot=transfer_entry[PAGER_BITS-1:0]; a_data={1'b1,!transfer_entry[168],transfer_entry};
        end else if(phase==EXCHANGE && exchange_done && exchange_published && exchange_first_resident) begin
            a_write=1; a_slot=exchange_first[PAGER_BITS-1:0];
        end else if(phase==EXCHANGE_FINISH && exchange_drop_second) begin
            a_write=1; a_slot=exchange_second[PAGER_BITS-1:0];
        end
    end
    wire write_selected=a_write && selected && is_ref(snapshot[39:0]) && snapshot[PAGER_BITS-1:0]==a_slot;
    wire write_prepared=a_write && prepared_access.status==OK &&
        prepared_access.reference[PAGER_BITS-1:0]==a_slot && a_data[172] &&
        a_data[39:0]==prepared_access.reference;
    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            phase<=CLEAR; clear_slot<='0; eviction_permitted<=0;
            word_threshold<=0; identity_threshold<=0;
            maintenance<=0; class_valid<=0; selected<=0; snapshot<=0;
            selected_live<=0; selected_persistent<=0; prepared_entry<=0; prepared_persistent<=0;
            prepared_access.reference<=NIL; prepared_access.index<=0; prepared_access.address<=0; prepared_access.status<=NO_SELECTION;
            pending_prepared<='0; pending_memory_entry<=0; pending_prepared_entry<=0;
            pending_live<=0; pending_selected_persistent<=0; pending_persistent<=0; pending_prepared_persistent<=0;
            idx<=0; idxreg<=0; response_status<=OK; response_data<=NIL;
            resident_mem_write_o<=0;resident_mem_addr_o<=0;resident_mem_data_o<=0;
            pending_idx<=0;pending_reg<=0;pending_selected<=0;pending_load_vr<=0;
            pending_mutator<=0;pending_write<=0;pending_vr<=0; directory_vr<=0;pending_data<=0;pending_snapshot<=0;pending_slot<=0;
            saved_victim_slot<=0; saved_victim_entry<=0;
            allocate_probe<='0; allocate_evict<=0; allocate_class<=0; allocate_size<=0; allocate_scan_tag<=0;
            victim_scan<=0; walk_slot<='0; scan_cursor<=0; scan_clear<=0; scan_value<=0;
            directory_next<=0; directory_identity<=0; directory_best_ref<=NIL; directory_best_class<=NIL;
            edge_pending<=0; edge_key_q<=0; exchange_drop_second<=0;
            held_pager<=0; held_index<=0; held_read<=0; held_register<=0; held_vr<=0; held_memory<=0;
            held_prepare<=0; held_prepared<=0; held_load_vr<=0; held_alloc_scan<=0; held_check_type<=0;
            held_alloc_size<=0; held_data<=0; held_expected_type<=0;
            for(j=0;j<8;j=j+1) vr[j]<=NIL;
            for(j=0;j<4;j=j+1) classes[j]<=0;
        end else begin
            // Keep the selected and prepared copies equal to the table. The
            // explicit assignments below take precedence where they differ.
            if(write_selected) begin
                selected_live<=a_data[172] && a_data[39:0]==snapshot[39:0];
                if(a_data[172] && a_data[39:0]==snapshot[39:0]) begin
                    snapshot<=a_data[170:0]; selected_persistent<=a_data[171];
                end
            end
            if(write_prepared) begin
                prepared_entry<=a_data[170:0]; prepared_persistent<=a_data[171];
            end
            // A disk record may name NEW objects that remain only in RAM.
            // Mark each resident object that a streamed record names as a
            // persistence dependency, as its reference is sent. A save that
            // later fails leaves those marks: they can only retain more.
            // Opaque payloads contribute no edges; classes always do.
            edge_pending<=store_valid_o && store_ready_i &&
                ((phase==TRANSFER && (store_op_o==2 || (store_op_o==3 && victim_scan))) ||
                 (phase==EXCHANGE && (store_op_o==2 || (store_op_o==3 && store_ref_o[37]))));
            edge_key_q<=edge_key;
            if(gc_commit) eviction_permitted<=1;
            else if(allocate_empty) eviction_permitted<=0;
            // Collector commit visits one slot per cycle. The write port moves
            // a retained entry to its staged base or drops the entry; the
            // registers that cache that slot follow on the same edge.
            if(gc_relocate) begin
                // A valid prepared reference is a root. Relocate its address
                // with the pager; leave deferred bounds/selection errors intact.
                if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==gc_relocate_slot) begin
                    if(gc_retained) prepared_access.address<=gc_base+prepared_access.index[23:0]-24'd1;
                    else prepared_access.status<=NOT_RESIDENT;
                end
                if(selected && is_ref(snapshot[39:0]) && snapshot[PAGER_BITS-1:0]==gc_relocate_slot &&
                    aux_valid && aux_stored[39:0]==snapshot[39:0] && !gc_retained) selected<=0;
            end
            case(phase)
                // Reset leaves the table's contents; drop every entry.
                CLEAR: begin
                    clear_slot<=clear_slot+1'b1;
                    if(clear_slot==PAGER_BITS'(ENTRIES-1)) phase<=IDLE;
                end
                IDLE, LOOKUP: begin
                    if(execute) begin
                        response_status<=error; response_data<=(error==OK) ? result_data : NIL;
                        if(start_directory) begin
                            phase<=DIRECTORY_SCAN; directory_vr<=c_vr; walk_slot<='0;
                            directory_next<=c_pager==8; directory_identity<=c_data[36:0];
                            directory_best_ref<=NIL; directory_best_class<=NIL;
                        end
                        else if(start_exchange) phase<=EXCHANGE;
                        else if(start_transfer) begin
                            phase<=TRANSFER; saved_victim_slot<=transfer_slot;
                            saved_victim_entry<=transfer_victim;
                            victim_scan<=transfer_victim[37];
                        end else if(allocate_search) begin
                            phase<=ALLOCATE_SCAN; allocate_probe<=PAGER_BITS'(1); allocate_evict<=0;
                            allocate_class<=c_data; allocate_size<=c_alloc_size;
                            allocate_scan_tag<=c_alloc_scan;
                        end
                        else if(error!=OK) phase<=RESPONSE;
                        else if(need_memory) begin
                            pending_idx<=next_idx_wide[39:0];pending_reg<=next_reg_wide[39:0];
                            pending_snapshot<=next_snapshot;pending_selected<=next_selected;
                            pending_live<=next_live; pending_selected_persistent<=next_selected_persistent;
                            pending_prepared<=next_prepared; pending_memory_entry<=next_memory_entry;
                            pending_prepared_entry<=next_prepared_entry;
                            pending_prepared_persistent<=next_prepared_persistent;
                            pending_persistent<=memory_persistent;
                            pending_load_vr<=c_load_vr;pending_vr<=c_vr;pending_data<=c_data;
                            pending_mutator<=1;pending_write<=c_memory==2;
                            pending_slot<=memory_entry[PAGER_BITS-1:0];
                            resident_mem_addr_o<=address_wide[23:0];resident_mem_data_o<=c_data;resident_mem_write_o<=c_memory==2;
                            phase<=REQUEST;
                        end else begin
                            idx<=next_idx_wide[39:0];idxreg<=next_reg_wide[39:0];
                            snapshot<=next_snapshot;selected<=next_selected;
                            selected_live<=next_live; selected_persistent<=next_selected_persistent;
                            prepared_access<=next_prepared;
                            prepared_entry<=next_prepared_entry; prepared_persistent<=next_prepared_persistent;
                            if(c_load_vr) vr[c_vr]<=c_data;
                            if(c_register==5) word_threshold<=c_data;
                            if(c_register==6) identity_threshold<=c_data;
                            phase<=RESPONSE;
                        end
                    end else if(accept) begin
                        held_pager<=pager_i; held_index<=index_i; held_read<=read_i;
                        held_register<=register_i; held_vr<=vr_i; held_memory<=memory_i;
                        held_prepare<=prepare_i; held_prepared<=prepared_i; held_load_vr<=load_vr_i;
                        held_alloc_scan<=alloc_scan_i; held_check_type<=check_type_i;
                        held_alloc_size<=alloc_size_i; held_data<=data_i; held_expected_type<=expected_type_i;
                        phase<=LOOKUP;
                    end else if(phase==IDLE && svc_valid_i && svc_ready_o) begin
                        response_status<=OK;response_data<=NIL;phase<=RESPONSE;
                        case(svc_op_i)
                            0: begin
                                if(!is_ref(svc_ref_i) || !is_ref(svc_class_i)) response_status<=INVALID_REFERENCE;
                                else if(svc_size_i==0 && svc_repr_i!=NIL) response_status<=BAD_VALUE;
                                else if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==svc_ref_i[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                            end
                            1: begin
                                if(!is_ref(svc_ref_i)) response_status<=INVALID_REFERENCE;
                                else begin held_data<=svc_ref_i; phase<=SERVICE_LOOKUP; end
                            end
                            2: begin
                                if(svc_code_i>3 || !is_ref(svc_class_i)) response_status<=BAD_VALUE;
                                else begin
                                    classes[svc_code_i[1:0]]<=svc_class_i;class_valid[svc_code_i[1:0]]<=1;
                                    if(selected && snapshot[39:38]==2'b11 && snapshot[37:32]==svc_code_i) snapshot[103:64]<=svc_class_i;
                                end
                            end
                            3: begin
                                if({8'b0,svc_base_i}>=MEMORY_WORDS) response_status<=BOUNDS;
                                else begin
                                    pending_mutator<=0;pending_write<=1;
                                    resident_mem_addr_o<=svc_base_i;resident_mem_data_o<=svc_repr_i;resident_mem_write_o<=1;
                                    scan_cursor<=0; scan_clear<=1; walk_slot<='0;
                                    phase<=SERVICE_SCAN;
                                end
                            end
                            4: begin
                                if({8'b0,svc_base_i}>=MEMORY_WORDS) response_status<=BOUNDS;
                                else begin
                                    pending_mutator<=0;pending_write<=0;
                                    resident_mem_addr_o<=svc_base_i;resident_mem_data_o<=0;resident_mem_write_o<=0;
                                    phase<=REQUEST;
                                end
                            end
                            5: begin
                                if(maintenance) response_status<=BAD_COMMAND;
                                else maintenance<=1;
                            end
                            6: begin
                                if(!maintenance) response_status<=BAD_COMMAND;
                                else begin
                                    scan_cursor<=1; scan_value<=svc_repr_i[24:0]; walk_slot<='0;
                                    scan_clear<=svc_repr_i<=40'(MEMORY_WORDS);
                                    phase<=SERVICE_SCAN;
                                end
                            end
                            7: begin
                                if(!maintenance) response_status<=BAD_COMMAND;
                                else maintenance<=0;
                            end
                            8: if(svc_repr_i==0 || svc_repr_i>40'h2000000000)
                                response_status<=BAD_VALUE;
                            default: response_status<=BAD_COMMAND;
                        endcase
                    end
                end
                // Invalidation requires a full reference match.
                SERVICE_LOOKUP: begin
                    if(!invalidate) response_status<=NOT_RESIDENT;
                    else if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==held_data[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                    phase<=RESPONSE;
                end
                // A raw write proceeds to memory only after every slot is
                // clear; an accepted cursor takes effect on the final cycle.
                SERVICE_SCAN: begin
                    walk_slot<=walk_ahead;
                    if(scan_conflict) scan_clear<=0;
                    if(walk_last) begin
                        if(!scan_clear || scan_conflict) begin
                            response_status<=scan_cursor ? BOUNDS : BAD_COMMAND; phase<=RESPONSE;
                        end else phase<=scan_cursor ? RESPONSE : REQUEST;
                    end
                end
                DIRECTORY_SCAN: begin
                    walk_slot<=walk_ahead;
                    if(directory_next ? directory_better : directory_exact) begin
                        directory_best_ref<=aux_stored[39:0]; directory_best_class<=aux_stored[103:64];
                    end
                    if(!directory_next || directory_exact || walk_last) phase<=DIRECTORY_START;
                end
                DIRECTORY_START: phase<=DIRECTORY;
                ALLOCATE_SCAN: begin
                    allocate_probe<=allocate_ahead;
                    if(allocate_last) allocate_evict<=1;
                    if(start_transfer) begin
                        phase<=TRANSFER; saved_victim_slot<=transfer_slot;
                        saved_victim_entry<=transfer_victim;
                        victim_scan<=transfer_victim[37];
                    end
                end
                REQUEST: if(mem_ready_i) phase<=WAIT_MEMORY;
                // Commit the private retirement record only after successful
                // memory completion. First-field writes publish both the body
                // value and its cached representation on the same logical commit.
                WAIT_MEMORY: if(mem_rsp_valid_i) begin
                    response_status<=mem_rsp_error_i ? MEMORY_ERROR : OK;
                    response_data<=NIL;
                    if(!mem_rsp_error_i && pending_mutator) begin
                        idx<=pending_idx;idxreg<=pending_reg;snapshot<=pending_snapshot;selected<=pending_selected;
                        selected_live<=pending_live; selected_persistent<=pending_selected_persistent;
                        prepared_access<=pending_prepared;
                        // A write to the prepared object updates its copy too.
                        if(pending_write && pending_prepared.status==OK &&
                            pending_prepared.reference==pending_memory_entry[39:0]) begin
                            prepared_entry<=pending_memory_entry; prepared_persistent<=pending_persistent;
                        end else begin
                            prepared_entry<=pending_prepared_entry;
                            prepared_persistent<=pending_prepared_persistent;
                        end
                        if(pending_load_vr) vr[pending_vr]<=pending_data;
                        if(pending_write) response_data<=pending_data;
                        else response_data<=mem_rsp_data_i;
                    end
                    if(!mem_rsp_error_i && !pending_mutator && !pending_write) response_data<=mem_rsp_data_i;
                    phase<=rsp_ready_i ? IDLE : RESPONSE;
                end
                DIRECTORY: if(directory_done) begin
                    response_status<=directory_status;
                    response_data<=directory_status==OK ? directory_ref : NIL;
                    if(directory_status==OK) vr[directory_vr]<=directory_class;
                    phase<=RESPONSE;
                end
                // Publication is one complete table write.
                // A failed transfer leaves the old slot and selection untouched;
                // transfer reservations and completed unpublished writes may remain.
                TRANSFER: if(transfer_done) begin
                    response_status<=transfer_status;
                    response_data<=(transfer_status==OK) ? transfer_entry[39:0] : NIL;
                    if(transfer_status==OK) begin
                        if(prepared_access.status==OK && prepared_access.reference[PAGER_BITS-1:0]==transfer_entry[PAGER_BITS-1:0]) prepared_access.status<=NOT_RESIDENT;
                        snapshot<=transfer_entry;selected<=1;
                        selected_live<=1; selected_persistent<=!transfer_entry[168];
                    end
                    phase<=RESPONSE;
                end
                // A published exchange drops both resident entries: the first
                // on this edge, the second on the next.
                EXCHANGE: if(exchange_done) begin
                    response_status<=exchange_status;
                    response_data<=exchange_status==OK ? exchange_first : NIL;
                    exchange_drop_second<=exchange_published && exchange_second_resident;
                    if(exchange_published) begin
                        if(prepared_access.status==OK && (prepared_access.reference==exchange_first || prepared_access.reference==exchange_second)) prepared_access.status<=NOT_RESIDENT;
                        if(selected && (snapshot[39:0]==exchange_first || snapshot[39:0]==exchange_second))
                            selected<=0;
                    end
                    phase<=EXCHANGE_FINISH;
                end
                EXCHANGE_FINISH: phase<=RESPONSE;
                RESPONSE: if(rsp_ready_i) phase<=IDLE;
                default: phase<=IDLE;
            endcase
        end
    end
endmodule

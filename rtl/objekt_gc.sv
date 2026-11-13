// Machine-owned maintenance datapath. LOGIK microcode supplies every root and
// issues every mark and copy. This module keeps the work list: the first mark
// of a slot queues it, and NEXT hands the most recently queued slot back to
// microcode. NEXT_MARKED then finds the marked slots in ascending order for
// copying. It has no graph walker and never requests backing records.
//
// Marks and relocation plans are transient, separate from persisted COND.
// Until COMMIT, the original pager and source semispace remain authoritative.
// A read/write error can therefore abandon the entire plan without rollback.
// Destination RAM may contain abandoned writes after an error.
//
// The pager table and this module's plan table are RAMs with registered
// reads. slot_o presents an address; the parent's entry for it, and this
// module's plan word, arrive on the next cycle. An ordinary control therefore
// spends one cycle in EXECUTE. A walk presents the next slot while it handles
// the current one, so it costs one cycle per slot. BEGIN walks every slot to
// validate the source-space layout, reset its plan word, and mark and queue
// the slots that persistence requires in RAM. COMMIT walks every slot to hand
// its staged base to the parent. The mutator is excluded throughout and a walk
// performs no memory transaction, so it cannot fail part way.
//
// BEGIN_MINOR starts a minor collection when enough words remain free and the
// request is not explicit, and otherwise answers zero so microcode can BEGIN a
// full one. An object is
// young if its body starts at or above the boundary: the allocation cursor at
// the last commit. Old objects count as live and are not traced; the walk
// also queues every old object whose remembered bit says a reference was
// stored into it since. COMMIT then keeps every entry in place except the
// unmarked young ones, leaves the cursor and semispace as they are, and makes
// every object old. Both kinds of commit clear the remembered bits.
module objekt_gc #(
    parameter integer PAGER_BITS=4, parameter integer MEMORY_WORDS=512
)(
    input logic clk_i, rst_i, active_i,
    input logic valid_i, output logic ready_o, input logic [3:0] operation_i,
    input logic [39:0] data_i, root_i,
    input logic [23:0] needed_i,
    // The write barrier: a mutator stored a reference into this slot's object,
    // whose body starts at remember_base_i.
    input logic remember_i, input logic [PAGER_BITS-1:0] remember_slot_i,
    input logic [23:0] remember_base_i,
    // The allocator's cursor and free words, for a minor collection.
    input logic [24:0] body_cursor_i, free_words_i,
    // An explicit collection request, which always collects in full.
    input logic explicit_i,
    output logic response_o, input logic response_ready_i,
    output logic [3:0] status_o, output logic [39:0] result_o,
    // The parent returns the entry for slot_o on the following cycle.
    output logic [PAGER_BITS-1:0] slot_o,
    input logic entry_valid_i, input logic [170:0] entry_i,
    input logic entry_persistent_i,
    input logic entry_prepared_i,
    // COMMIT walk: one slot per cycle, with its fate and new base.
    output logic relocate_o, commit_o,
    output logic [PAGER_BITS-1:0] relocate_slot_o,
    output logic retained_o,
    output logic [23:0] base_o,
    output logic [24:0] cursor_o,
    output logic space_o, committed_o,
    output logic mem_valid_o, input logic mem_ready_i,
    output logic mem_write_o, output logic [23:0] mem_addr_o,
    output logic [39:0] mem_data_o,
    input logic mem_response_i, output logic mem_response_ready_o,
    input logic [39:0] mem_data_i, input logic mem_error_i
);
    `include "objekt_entry.svh"
    localparam integer ENTRIES=1<<PAGER_BITS;
    localparam integer HALF=MEMORY_WORDS/2;
    localparam logic [3:0] BEGIN_GC=1, ROOT=2, SLOT=3, INFO=4,
        MARK=5, READ_BODY=6, STAGE=7, WRITE_BODY=8, COMMIT=9, NEXT=12, NEXT_MARKED=13,
        BEGIN_MINOR=14;
    typedef enum logic [2:0] {IDLE, EXECUTE, REQUEST, WAIT_MEMORY, RESPONSE,
        CHECK_LAYOUT, RELOCATE, FIND_MARKED} phase_t;
    phase_t phase;
    /* verilator lint_off UNUSEDSIGNAL */
    pager_entry_t entry;
    /* verilator lint_on UNUSEDSIGNAL */
    assign entry=entry_i;

    // Plan word per slot: {mark, staged, complete, staged base, work-list link}.
    localparam integer PLAN_BITS=27+PAGER_BITS;
    logic [PLAN_BITS-1:0] plan [0:ENTRIES-1];
    logic [PLAN_BITS-1:0] plan_q, plan_data;
    /* verilator lint_off UNUSEDSIGNAL */
    logic [PLAN_BITS-1:0] held_plan;
    /* verilator lint_on UNUSEDSIGNAL */
    logic plan_write;
    logic [PAGER_BITS-1:0] plan_slot;
    always_ff @(posedge clk_i) begin
        if(plan_write) plan[plan_slot]<=plan_data;
        plan_q<=plan[slot_o];
    end
    // Remembered bits: set by the write barrier while the mutator runs,
    // cleared by the commit walk.
    logic remembered [0:ENTRIES-1];
    logic remembered_q;
    logic [24:0] boundary;
    logic minor;

    wire plan_mark=plan_q[PLAN_BITS-1];
    wire plan_staged=plan_q[PLAN_BITS-2];
    wire [23:0] plan_base=plan_q[PLAN_BITS-4 -: 24];
    wire [PAGER_BITS-1:0] plan_link=plan_q[PAGER_BITS-1:0];

    logic [3:0] operation;
    logic [39:0] data, root;
    logic [PAGER_BITS-1:0] selected_slot, walk_slot, held_slot, gray_head;
    logic [PAGER_BITS:0] gray_count, marked_count, done_count;
    // NEXT_MARKED resumes its ascending scan here; ENTRIES means finished.
    logic [PAGER_BITS:0] copy_scan;
    logic [23:0] write_offset;
    logic initialized, copied_word, layout_valid;
    logic [23:0] copied_offset;
    logic [39:0] buffer;
    logic [25:0] end_address;
    logic [24:0] destination_end;

    wire walking=phase==CHECK_LAYOUT || phase==RELOCATE || phase==FIND_MARKED;
    always_ff @(posedge clk_i) begin
        if(phase==RELOCATE) remembered[walk_slot]<=1'b0;
        else if(remember_i && {1'b0,remember_base_i}<boundary) remembered[remember_slot_i]<=1'b1;
        remembered_q<=remembered[slot_o];
    end
    wire walk_last=walk_slot==PAGER_BITS'(ENTRIES-1);
    // The address for the next cycle's entry and plan word.
    always_comb begin
        slot_o=selected_slot;
        if(walking) slot_o=walk_slot+1'b1;
        else if(phase==IDLE) case(operation_i)
            BEGIN_GC, BEGIN_MINOR, COMMIT: slot_o='0;
            NEXT_MARKED: slot_o=copy_scan[PAGER_BITS-1:0];
            MARK, SLOT: slot_o=data_i[PAGER_BITS-1:0];
            NEXT: slot_o=gray_head;
            default: slot_o=selected_slot;
        endcase
    end
    // A resident body must lie inside the semispace that is about to be copied.
    wire entry_in_source=entry.base >= (space_o ? 24'(HALF) : 24'b0) &&
        {1'b0,entry.base}+{1'b0,entry.size} <= (space_o ? 25'(MEMORY_WORDS) : 25'(HALF));
    // Persistence requires this object in RAM: a modified persistent object, a
    // NEW one that a stored record depends on, or a prepared access.
    wire retention=(entry.modified && !entry.is_new) || (entry.is_new && entry_persistent_i) ||
        entry_prepared_i;
    wire [39:0] slot_flags={35'b0,entry.is_new,retention,entry.reference[37],plan_mark,entry_valid_i};
    wire young={1'b0,entry.base}>=boundary;
    // A minor collection treats old objects as live and does not trace them.
    wire mark_hit=data[39:38]==2'b10 && data[36:0]!=0 && entry_valid_i && entry.reference==data &&
        (!minor || young);

    assign ready_o=active_i && phase==IDLE && !rst_i;
    assign response_o=phase==RESPONSE && active_i && !rst_i;
    assign mem_valid_o=phase==REQUEST && active_i && !rst_i;
    assign mem_response_ready_o=phase==WAIT_MEMORY && active_i && !rst_i;
    assign mem_data_o=buffer;
    // A full collection copies into the inactive half; a minor one keeps the
    // active half and its allocator.
    assign destination_end=(space_o ^ minor) ? 25'(HALF) : 25'(MEMORY_WORDS);
    assign end_address={1'b0,cursor_o}+{2'b0,entry.size};
    // COMMIT is admitted only when every marked object is copied and the
    // destination leaves room for the pending request. Publication and the
    // allocator exchange occur on the final relocation edge, never at an
    // earlier per-object copy completion.
    wire commit_room=({1'b0,cursor_o}+{2'b0,needed_i}) <= {1'b0,destination_end};
    assign relocate_o=phase==RELOCATE && active_i && !rst_i;
    assign relocate_slot_o=walk_slot;
    assign retained_o=minor ? (entry_valid_i && (!young || plan_mark)) : plan_mark;
    assign base_o=minor ? entry.base : plan_base;
    assign commit_o=relocate_o && walk_last;

    // The single plan write port.
    wire push_retained=phase==CHECK_LAYOUT && entry_valid_i &&
        (retention || (minor && remembered_q && !young));
    wire push_marked=phase==EXECUTE && operation==MARK && mark_hit && !plan_mark;
    wire stage_ok=phase==EXECUTE && operation==STAGE && entry_valid_i && plan_mark && !plan_staged &&
        end_address<={1'b0,destination_end};
    wire copy_done=phase==WAIT_MEMORY && mem_response_i && !mem_error_i && mem_write_o &&
        write_offset+1'b1==entry.size;
    always_comb begin
        plan_write=0; plan_slot=held_slot; plan_data=plan_q;
        if(phase==CHECK_LAYOUT) begin
            plan_write=1; plan_slot=walk_slot;
            plan_data={push_retained,2'b00,24'b0,gray_head};
        end else if(push_marked) begin
            plan_write=1; plan_data={3'b100,24'b0,gray_head};
        end else if(stage_ok) begin
            plan_write=1; plan_data={2'b11,entry.size==0,cursor_o[23:0],plan_link};
        end else if(copy_done) begin
            plan_write=1; plan_slot=selected_slot;
            plan_data={held_plan[PLAN_BITS-1:PLAN_BITS-2],1'b1,held_plan[PLAN_BITS-4:0]};
        end
    end

    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            phase<=IDLE; write_offset<=0;
            selected_slot<='0; walk_slot<='0; held_slot<='0; layout_valid<=0; initialized<=0; committed_o<=0;
            gray_head<='0; gray_count<='0; marked_count<='0; done_count<='0; copy_scan<='0;
            space_o<=0; cursor_o<=0; status_o<=0; result_o<=0; boundary<=0; minor<=0;
            mem_write_o<=0; mem_addr_o<=0; buffer<=0;
            copied_word<=0; copied_offset<=0; operation<=0; data<=0; root<=0; held_plan<='0;
        end else if(!active_i) begin
            phase<=IDLE; initialized<=0; committed_o<=0; copied_word<=0; minor<=0;
        end else begin
            case(phase)
                IDLE: if(valid_i && ready_o) begin
                    status_o<=0; result_o<=0;
                    operation<=operation_i; data<=data_i; root<=root_i; held_slot<=slot_o;
                    phase<=EXECUTE;
                    if((!initialized && operation_i!=BEGIN_GC && operation_i!=BEGIN_MINOR) ||
                        committed_o) begin
                        status_o<=1; phase<=RESPONSE;
                    end else case(operation_i)
                        BEGIN_GC, BEGIN_MINOR: begin
                            if(initialized) begin status_o<=6; phase<=RESPONSE; end
                            // A minor collection reclaims no words: it leaves an
                            // eighth of a semispace free besides the request.
                            else if(operation_i==BEGIN_MINOR && (explicit_i ||
                                free_words_i < 25'(needed_i) + 25'(HALF/8))) phase<=RESPONSE;
                            else begin
                                minor<=operation_i==BEGIN_MINOR;
                                result_o<={39'b0,operation_i==BEGIN_MINOR};
                                walk_slot<='0; phase<=CHECK_LAYOUT;
                                layout_valid<=MEMORY_WORDS>=2 && MEMORY_WORDS%2==0;
                                gray_count<='0; marked_count<='0; done_count<='0; copy_scan<='0;
                            end
                        end
                        COMMIT: begin
                            if(!minor && marked_count!=done_count) begin status_o<=1; phase<=RESPONSE; end
                            else if(!commit_room) begin status_o<=11; phase<=RESPONSE; end
                            else begin walk_slot<='0; phase<=RELOCATE; end
                        end
                        NEXT_MARKED: begin
                            if(copy_scan[PAGER_BITS]) phase<=RESPONSE;
                            else begin walk_slot<=copy_scan[PAGER_BITS-1:0]; phase<=FIND_MARKED; end
                        end
                        default: begin end
                    endcase
                end
                // The entry and plan word for held_slot are now valid.
                EXECUTE: begin
                    phase<=RESPONSE;
                    case(operation)
                        ROOT: result_o<=root;
                        SLOT: begin
                            if(data>=40'(ENTRIES)) status_o<=6;
                            else begin
                                selected_slot<=held_slot;
                                result_o<=entry_valid_i ? slot_flags : 40'b0;
                                copied_word<=0;
                            end
                        end
                        // An empty list returns zero and leaves the selection.
                        NEXT: if(gray_count!=0) begin
                            selected_slot<=held_slot;
                            result_o<=slot_flags;
                            copied_word<=0;
                            gray_head<=plan_link; gray_count<=gray_count-1'b1;
                        end
                        INFO: if(!entry_valid_i) status_o<=4;
                            else case(data)
                                0: result_o<=entry.reference;
                                1: result_o<=entry.class_reference;
                                2: result_o<={16'b0,entry.size};
                                default: status_o<=1;
                            endcase
                        // MARK is a full-reference, resident-only lookup. The
                        // first mark of a slot pushes it on the work list.
                        MARK: if(mark_hit) begin
                            result_o<={39'b0,!plan_mark};
                            if(!plan_mark) begin
                                gray_head<=held_slot; gray_count<=gray_count+1'b1;
                                marked_count<=marked_count+1'b1;
                            end
                        end
                        READ_BODY, WRITE_BODY: begin
                            if(!entry_valid_i || data>={16'b0,entry.size} ||
                                {1'b0,entry.base}+{1'b0,entry.size}>25'(MEMORY_WORDS)) status_o<=6;
                            else if(operation==WRITE_BODY && (!plan_staged ||
                                !copied_word || copied_offset!=data[23:0] || write_offset!=data[23:0])) status_o<=1;
                            else begin
                                mem_addr_o<=(operation==READ_BODY ? entry.base : plan_base)+data[23:0];
                                mem_write_o<=operation==WRITE_BODY;
                                copied_offset<=data[23:0];
                                held_plan<=plan_q;
                                if(operation==WRITE_BODY) copied_word<=0;
                                phase<=REQUEST;
                            end
                        end
                        STAGE: begin
                            if(!entry_valid_i || !plan_mark || plan_staged) status_o<=1;
                            else if(end_address>{1'b0,destination_end}) status_o<=11;
                            else begin
                                cursor_o<=end_address[24:0];
                                write_offset<=0;
                                if(entry.size==0) done_count<=done_count+1'b1;
                            end
                        end
                        default: status_o<=1;
                    endcase
                end
                CHECK_LAYOUT: begin
                    walk_slot<=walk_slot+1'b1;
                    if(push_retained) begin
                        gray_head<=walk_slot; gray_count<=gray_count+1'b1;
                        marked_count<=marked_count+1'b1;
                    end
                    if(entry_valid_i && !entry_in_source) layout_valid<=0;
                    if(walk_last) begin
                        phase<=RESPONSE;
                        if(!layout_valid || (entry_valid_i && !entry_in_source)) status_o<=6;
                        else begin
                            write_offset<=0; initialized<=1;
                            cursor_o<=minor ? body_cursor_i : (space_o ? 25'b0 : 25'(HALF));
                        end
                    end
                end
                RELOCATE: begin
                    walk_slot<=walk_slot+1'b1;
                    if(walk_last) begin
                        committed_o<=1; phase<=RESPONSE; boundary<=cursor_o;
                        if(!minor) space_o<=!space_o;
                    end
                end
                // One slot per cycle until a marked slot or the end.
                FIND_MARKED: begin
                    walk_slot<=walk_slot+1'b1;
                    copy_scan<={1'b0,walk_slot}+1'b1;
                    if(plan_mark) begin
                        selected_slot<=walk_slot; result_o<=slot_flags; copied_word<=0;
                        phase<=RESPONSE;
                    end else if(walk_last) phase<=RESPONSE;
                end
                REQUEST: if(mem_ready_i) phase<=WAIT_MEMORY;
                WAIT_MEMORY: if(mem_response_i) begin
                    phase<=RESPONSE;
                    status_o<=mem_error_i ? 4'd9 : 4'd0;
                    if(!mem_error_i && mem_write_o) begin
                        write_offset<=write_offset+1'b1;
                        if(copy_done) done_count<=done_count+1'b1;
                    end
                    if(!mem_error_i && !mem_write_o) begin
                        buffer<=mem_data_i; result_o<=mem_data_i; copied_word<=1;
                    end
                end
                RESPONSE: if(response_ready_i) phase<=IDLE;
                default: phase<=IDLE;
            endcase
        end
    end
endmodule

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
// The pager table and the staged bases are RAMs, read one slot per cycle.
// BEGIN therefore walks every slot to validate the source-space layout and to
// mark and queue the slots that persistence requires in RAM, and COMMIT walks
// every slot to hand its staged base to the parent. The work list is a stack
// threaded through a link RAM, so it needs no storage beyond one link per slot. The mutator
// is excluded throughout and a walk performs no memory transaction, so it
// cannot fail part way: commit_o on the final cycle publishes validity and
// exchanges the allocation spaces as one step.
module objekt_gc #(
    parameter integer PAGER_BITS=4, parameter integer MEMORY_WORDS=512
)(
    input logic clk_i, rst_i, active_i,
    input logic valid_i, output logic ready_o, input logic [3:0] operation_i,
    input logic [39:0] data_i, root_i,
    input logic [23:0] needed_i,
    output logic response_o, input logic response_ready_i,
    output logic [3:0] status_o, output logic [39:0] result_o,
    output logic [PAGER_BITS-1:0] slot_o,
    input logic entry_valid_i, input logic [170:0] entry_i,
    input logic entry_persistent_i,
    input logic entry_prepared_i,
    output logic relocate_o, commit_o,
    output logic [(1<<PAGER_BITS)-1:0] retained_o,
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
        MARK=5, READ_BODY=6, STAGE=7, WRITE_BODY=8, COMMIT=9, NEXT=12, NEXT_MARKED=13;
    typedef enum logic [2:0] {IDLE, REQUEST, WAIT_MEMORY, RESPONSE, CHECK_LAYOUT, RELOCATE, FIND_MARKED} phase_t;
    phase_t phase;
    /* verilator lint_off UNUSEDSIGNAL */
    pager_entry_t entry;
    /* verilator lint_on UNUSEDSIGNAL */
    logic [ENTRIES-1:0] marks, staged, complete;
    logic [23:0] bases [0:ENTRIES-1];
    logic [PAGER_BITS-1:0] links [0:ENTRIES-1];
    logic [PAGER_BITS-1:0] gray_head;
    logic [PAGER_BITS:0] gray_count;
    // NEXT_MARKED resumes its ascending scan here; ENTRIES means finished.
    logic [PAGER_BITS:0] copy_scan;
    logic [23:0] write_offset;
    logic [PAGER_BITS-1:0] selected_slot, walk_slot;
    logic initialized, copied_word, layout_valid, entry_in_source, walking, walk_last;
    logic [23:0] copied_offset;
    logic [39:0] buffer;
    logic [25:0] end_address;
    logic [24:0] destination_end;
    assign entry=entry_i;
    // MARK performs a full-reference, resident-only lookup. Slot iteration is
    // separate, so collisions cannot be mistaken for an object's residency.
    assign walking=phase==CHECK_LAYOUT || phase==RELOCATE || phase==FIND_MARKED;
    assign walk_last=walk_slot==PAGER_BITS'(ENTRIES-1);
    assign slot_o=walking ? walk_slot : operation_i==MARK ? data_i[PAGER_BITS-1:0] :
        operation_i==SLOT ? data_i[PAGER_BITS-1:0] : operation_i==NEXT ? gray_head : selected_slot;
    // Persistence requires this object in RAM: a modified persistent object, a
    // NEW one that a committed record depends on, or a prepared access.
    wire retention=(entry.modified && !entry.is_new) || (entry.is_new && entry_persistent_i) ||
        entry_prepared_i;
    wire [39:0] slot_flags={35'b0,entry.is_new,retention,entry.reference[37],marks[slot_o],entry_valid_i};
    wire accepted=valid_i && ready_o && initialized && !committed_o;
    wire mark_hit=data_i[39:38]==2'b10 && data_i[36:0]!=0 && entry_valid_i && entry.reference==data_i;
    // The first mark of a slot pushes it; NEXT pops.
    wire push=active_i && !rst_i && ((phase==CHECK_LAYOUT && entry_valid_i && retention) ||
        (phase==IDLE && accepted && operation_i==MARK && mark_hit && !marks[slot_o]));
    wire pop=active_i && !rst_i && phase==IDLE && accepted && operation_i==NEXT && gray_count!=0;
    always_ff @(posedge clk_i) if(push) links[slot_o]<=gray_head;
    assign base_o=bases[slot_o];
    // A resident body must lie inside the semispace that is about to be copied.
    assign entry_in_source=entry.base >= (space_o ? 24'(HALF) : 24'b0) &&
        {1'b0,entry.base}+{1'b0,entry.size} <= (space_o ? 25'(MEMORY_WORDS) : 25'(HALF));
    assign ready_o=active_i && phase==IDLE && !rst_i;
    assign response_o=phase==RESPONSE && active_i && !rst_i;
    assign mem_valid_o=phase==REQUEST && active_i && !rst_i;
    assign mem_response_ready_o=phase==WAIT_MEMORY && active_i && !rst_i;
    assign mem_data_o=buffer;
    assign retained_o=marks;
    assign destination_end=space_o ? 25'(HALF) : 25'(MEMORY_WORDS);
    assign end_address={1'b0,cursor_o}+{2'b0,entry.size};
    // COMMIT is admitted only when every marked object is copied and the
    // destination leaves room for the pending request. Publication and the
    // allocator exchange occur on the final relocation edge, never at an
    // earlier per-object copy completion.
    wire commit_ready=initialized && !committed_o && (marks & ~complete)=='0 &&
        ({1'b0,cursor_o}+{2'b0,needed_i}) <= {1'b0,destination_end};
    assign relocate_o=phase==RELOCATE && active_i && !rst_i;
    assign commit_o=relocate_o && walk_last;

    always_ff @(posedge clk_i) begin
        if(rst_i) begin
            phase<=IDLE; marks<='0; staged<='0; complete<='0; write_offset<=0;
            selected_slot<='0; walk_slot<='0; layout_valid<=0; initialized<=0; committed_o<=0;
            gray_head<='0; gray_count<='0; copy_scan<='0;
            space_o<=0; cursor_o<=0; status_o<=0; result_o<=0;
            mem_write_o<=0; mem_addr_o<=0; buffer<=0;
            copied_word<=0; copied_offset<=0;
        end else if(!active_i) begin
            phase<=IDLE; initialized<=0; committed_o<=0; copied_word<=0;
        end else begin
            case(phase)
                IDLE: if(valid_i && ready_o) begin
                    status_o<=0; result_o<=0; phase<=RESPONSE;
                    if((!initialized && operation_i!=BEGIN_GC) || committed_o) status_o<=1;
                    else case(operation_i)
                        BEGIN_GC: begin
                            if(initialized) status_o<=6;
                            else begin
                                walk_slot<='0; phase<=CHECK_LAYOUT;
                                layout_valid<=MEMORY_WORDS>=2 && MEMORY_WORDS%2==0;
                                marks<='0; staged<='0; complete<='0; gray_count<='0; copy_scan<='0;
                            end
                        end
                        ROOT: result_o<=root_i;
                        SLOT: begin
                            if(data_i>=40'(ENTRIES)) status_o<=6;
                            else begin
                                selected_slot<=data_i[PAGER_BITS-1:0];
                                result_o<=entry_valid_i ? slot_flags : 40'b0;
                                copied_word<=0;
                            end
                        end
                        // An empty list returns zero and leaves the selection.
                        NEXT: if(gray_count!=0) begin
                            selected_slot<=gray_head;
                            result_o<=slot_flags;
                            copied_word<=0;
                        end
                        INFO: if(!entry_valid_i) status_o<=4;
                            else case(data_i)
                                0: result_o<=entry.reference;
                                1: result_o<=entry.class_reference;
                                2: result_o<={16'b0,entry.size};
                                default: status_o<=1;
                            endcase
                        NEXT_MARKED: if(!copy_scan[PAGER_BITS]) begin
                            walk_slot<=copy_scan[PAGER_BITS-1:0]; phase<=FIND_MARKED;
                        end
                        MARK: if(mark_hit) result_o<={39'b0,!marks[slot_o]};
                        READ_BODY, WRITE_BODY: begin
                            if(!entry_valid_i || data_i>={16'b0,entry.size} ||
                                {1'b0,entry.base}+{1'b0,entry.size}>25'(MEMORY_WORDS)) status_o<=6;
                            else if(operation_i==WRITE_BODY && (!staged[selected_slot] ||
                                !copied_word || copied_offset!=data_i[23:0] || write_offset!=data_i[23:0])) status_o<=1;
                            else begin
                                mem_addr_o<=(operation_i==READ_BODY ? entry.base : base_o)+data_i[23:0];
                                mem_write_o<=operation_i==WRITE_BODY;
                                copied_offset<=data_i[23:0];
                                if(operation_i==WRITE_BODY) copied_word<=0;
                                phase<=REQUEST;
                            end
                        end
                        STAGE: begin
                            if(!entry_valid_i || !marks[selected_slot] || staged[selected_slot]) status_o<=1;
                            else if(end_address>{1'b0,destination_end}) status_o<=11;
                            else begin
                                // The only write to the staged bases, and it
                                // precedes every read of its slot: no reset.
                                bases[selected_slot]<=cursor_o[23:0];
                                cursor_o<=end_address[24:0]; staged[selected_slot]<=1;
                                write_offset<=0; complete[selected_slot]<=entry.size==0;
                            end
                        end
                        COMMIT: begin
                            if((marks & ~complete)!='0) status_o<=1;
                            else if(!commit_ready) status_o<=11;
                            else begin walk_slot<='0; phase<=RELOCATE; end
                        end
                        default: status_o<=1;
                    endcase
                end
                CHECK_LAYOUT: begin
                    walk_slot<=walk_slot+1'b1;
                    if(entry_valid_i && !entry_in_source) layout_valid<=0;
                    if(walk_last) begin
                        phase<=RESPONSE;
                        if(!layout_valid || (entry_valid_i && !entry_in_source)) status_o<=6;
                        else begin
                            write_offset<=0; initialized<=1;
                            cursor_o<=space_o ? 25'b0 : 25'(HALF);
                        end
                    end
                end
                // One slot per cycle until a marked slot or the end.
                FIND_MARKED: begin
                    walk_slot<=walk_slot+1'b1;
                    copy_scan<={1'b0,walk_slot}+1'b1;
                    if(marks[walk_slot]) begin
                        selected_slot<=walk_slot; result_o<=slot_flags; copied_word<=0;
                        phase<=RESPONSE;
                    end else if(walk_last) phase<=RESPONSE;
                end
                RELOCATE: begin
                    walk_slot<=walk_slot+1'b1;
                    if(walk_last) begin committed_o<=1; space_o<=!space_o; phase<=RESPONSE; end
                end
                REQUEST: if(mem_ready_i) phase<=WAIT_MEMORY;
                WAIT_MEMORY: if(mem_response_i) begin
                    phase<=RESPONSE;
                    status_o<=mem_error_i ? 4'd9 : 4'd0;
                    if(!mem_error_i && mem_write_o) begin
                        write_offset<=write_offset+1'b1;
                        if(write_offset+1'b1==entry.size) complete[selected_slot]<=1;
                    end
                    if(!mem_error_i && !mem_write_o) begin
                        buffer<=mem_data_i; result_o<=mem_data_i; copied_word<=1;
                    end
                end
                RESPONSE: if(response_ready_i) phase<=IDLE;
                default: phase<=IDLE;
            endcase
            if(push) begin
                marks[slot_o]<=1; gray_head<=slot_o; gray_count<=gray_count+1'b1;
            end
            if(pop) begin
                gray_head<=links[gray_head]; gray_count<=gray_count-1'b1;
            end
        end
    end
endmodule

// LOGIK integration and instruction retirement controller.
//
// Submodules own architectural state: store/fetch, stacks, sequencer, NUMERIK.
// This module decodes one horizontal control word and grants one shared retire
// pulse. A local validation error cannot issue an OBJEKT command or partially
// update local state. OBJEKT keeps its separately documented transfer semantics.
//
// An object instruction moves through EXECUTE -> ISSUE -> WAIT_RESPONSE.
// EXECUTE validates every local effect and captures the chosen condition.
// ISSUE holds the command stable until acceptance. WAIT_RESPONSE cannot reissue
// it. Only a successful response retires local effects and advances the PC.
// Capturing the condition before ISSUE prevents IRQ changes during backpressure
// from changing the selected target. A synchronous, stable IRQ input is assumed.
//
// The 256-bit project format is described in rekursiv_control.svh. Book-derived
// behavior and explicit project choices are explained beside the owning logic.
// The top-level port interface remains scalar for Marlin and FPGA integration.
module logik #(
    parameter integer CODE_WORDS=256, parameter integer STACK_WORDS=32,
    parameter integer NAM_WORDS=256
)(
    input logic clk_i, input logic rst_i,
    input logic boot_valid_i, output logic boot_ready_o,
    input logic [1:0] boot_space_i, input logic [15:0] boot_addr_i,
    input logic [2:0] boot_lane_i, input logic [31:0] boot_data_i,
    input logic gc_enable_i, input logic [15:0] gc_entry_i,
    output logic gc_active_o, output logic gc_valid_o, input logic gc_ready_i,
    output logic [3:0] gc_operation_o, output logic [39:0] gc_data_o, gc_root_o,
    input logic gc_response_i, output logic gc_response_ready_o,
    input logic [3:0] gc_status_i, input logic [39:0] gc_result_i,
    input logic gc_committed_i, output logic [3:0] last_status_o,
    input logic start_i, input logic [15:0] entry_i, input logic resume_i, input logic irq_i,
    output logic halted_o, output logic service_o, output logic [3:0] service_code_o,
    output logic [3:0] fault_o, output logic retire_o,
    output logic cmd_valid_o, input logic cmd_ready_i,
    output logic [3:0] pager_o, output logic [3:0] index_o,
    output logic [2:0] register_o, output logic [1:0] memory_o,
    output logic [3:0] read_o, output logic load_vr_o, output logic [2:0] vr_o,
    output logic [23:0] alloc_size_o, output logic alloc_scan_o,
    output logic [39:0] data_o, output logic check_type_o, output logic [39:0] expected_type_o,
    input logic rsp_valid_i, output logic rsp_ready_o,
    input logic [3:0] rsp_status_i, input logic [39:0] rsp_data_i,
    output logic [15:0] pc_o, output logic [15:0] upcor_o, output logic [15:0] mark_o,
    output logic [23:0] sp_o, output logic [23:0] esp_o, output logic [23:0] csp_o,
    output logic [23:0] ap_o, output logic [23:0] apc_o,
    output logic [39:0] estkr_o, output logic [23:0] cstkr_o,
    output logic [39:0] symbol_o, output logic [39:0] object_o,
    output logic [31:0] q_o, output logic [63:0] product_o,
    output logic [4:0] flags_o, output logic lastcc_o,
    output logic [15:0] ucar_o, output logic [29:0] namarg_o,
    input logic [15:0] dbg_addr_i, output logic [31:0] dbg_rf_o,
    output logic [39:0] dbg_estk_o, output logic [23:0] dbg_cstk_o,
    output logic dbg_code_valid_o, output logic [39:0] dbg_code_data_o, output logic [39:0] dbg_code_type_o
);
    `include "rekursiv_control.svh"

    typedef enum logic [2:0] {
        HALTED, EXECUTE, WAIT_RESPONSE, SERVICE_BREAK, ISSUE
    } execution_state_t;
    execution_state_t state, next_state;
    microinstruction_t instruction;
    processor_fault_t local_fault;
    logic instruction_valid, boot_fault, target_fault, stack_fault, fetch_fault, compact_fault;
    logic retire, start, condition_value, issued_condition;
    object_word_t bus_value, symbol, object_result;
    logic [3:0] object_status, service_code;
    logic [31:0] alu_result, register_a;
    // Recovery reuses this execution pipeline and NUMERIK, not a second CPU.
    // Entry is possible only after a completed, unreserved space failure. The
    // sequencer and arithmetic modules save their own context on gc_enter;
    // stacks and the fetch pipeline are held by their retirement gates. ROOT
    // reads frozen/saved values, so collector temporaries never become roots.
    // Return retries with the original condition, even if IRQ changed meanwhile.
    logic gc_enter, gc_exit, gc_failed, retried, response_valid;
    logic [3:0] response_status;
    logic [39:0] response_data, saved_symbol, saved_object, saved_bus, saved_type;
    logic [3:0] saved_status;
    logic saved_issued_condition;
    logic [15:0] stack_root_address, store_root_address;
    logic [39:0] stack_root, code_literal, code_type, extra_root;
    localparam integer STACK_ROOT=20, CODE_ROOT=STACK_ROOT+STACK_WORDS;
    localparam integer EXTRA_ROOT=CODE_ROOT+2*CODE_WORDS;
    assign response_valid=gc_active_o ? gc_response_i : rsp_valid_i;
    assign response_status=gc_active_o ? gc_status_i : rsp_status_i;
    assign response_data=gc_active_o ? gc_result_i : rsp_data_i;
    assign gc_enter=state==WAIT_RESPONSE && !gc_active_o && rsp_valid_i &&
        rsp_status_i==11 && gc_enable_i && !retried &&
        (instruction.object_pager==6 || instruction.object_pager==5);
    assign gc_failed=gc_active_o && ((state==EXECUTE && local_fault!=FAULT_NONE) ||
        (state==WAIT_RESPONSE && gc_response_i && gc_status_i!=0));
    assign gc_exit=gc_failed || (gc_active_o && state==EXECUTE &&
        local_fault==FAULT_NONE && instruction.recovery==10);
    assign gc_valid_o=state==ISSUE && gc_active_o && !rst_i;
    assign gc_response_ready_o=state==WAIT_RESPONSE && gc_active_o && !rst_i;
    assign gc_operation_o=instruction.recovery;
    assign gc_data_o=bus_value;
    assign stack_root_address=16'(bus_value-40'(STACK_ROOT));
    assign store_root_address=bus_value>=40'(EXTRA_ROOT) ? 16'(bus_value-40'(EXTRA_ROOT)) : 16'((bus_value-40'(CODE_ROOT))>>1);
    always_comb begin
        gc_root_o=0;
        case(bus_value)
            14: gc_root_o=saved_bus;
            15: gc_root_o=saved_type;
            16: gc_root_o=estkr_o;
            17: gc_root_o=saved_symbol;
            18: gc_root_o=saved_object;
            default: begin
                if(bus_value>=40'(STACK_ROOT) && bus_value<40'(CODE_ROOT)) gc_root_o=stack_root;
                else if(bus_value>=40'(CODE_ROOT) && bus_value<40'(EXTRA_ROOT)) gc_root_o=bus_value[0] ^ (CODE_ROOT%2!=0) ? code_type : code_literal;
                else if(bus_value>=40'(EXTRA_ROOT) && bus_value<40'(EXTRA_ROOT)+32) gc_root_o=extra_root;
            end
        endcase
    end

    assign start = state == HALTED && start_i && !boot_valid_i;
    assign boot_ready_o = state == HALTED && !rst_i && !start_i;
    assign halted_o = state == HALTED;
    assign service_o = state == SERVICE_BREAK;
    assign service_code_o = service_code;
    assign cmd_valid_o = state == ISSUE && !gc_active_o && !rst_i;
    assign rsp_ready_o = state == WAIT_RESPONSE && !gc_active_o && !rst_i;

    logik_store #(.CODE_WORDS(CODE_WORDS), .NAM_WORDS(NAM_WORDS)) instruction_store (
        .clk_i(clk_i), .rst_i(rst_i), .retire_i(retire && !gc_active_o),
        .root_address_i(store_root_address),.root_literal_o(code_literal),.root_type_o(code_type),.root_extra_o(extra_root),
        .boot_write_i(boot_valid_i && boot_ready_o),
        .boot_space_i(boot_space_i), .boot_address_i(boot_addr_i),
        .boot_lane_i(boot_lane_i), .boot_data_i(boot_data_i), .boot_fault_o(boot_fault),
        .pc_i(pc_o), .instruction_o(instruction), .instruction_valid_o(instruction_valid),
        .apc_operation_i(instruction.apc), .bus_i(bus_value),
        .fetch_nam_i(instruction.fetch_nam), .fetch_map_i(instruction.fetch_map),
        .fetch_fault_o(fetch_fault), .apc_o(apc_o), .dispatch_o(ucar_o), .operand_o(namarg_o),
        .debug_address_i(dbg_addr_i), .debug_valid_o(dbg_code_valid_o),
        .debug_literal_o(dbg_code_data_o), .debug_type_o(dbg_code_type_o)
    );

    logik_stacks #(.STACK_WORDS(STACK_WORDS)) stacks (
        .clk_i(clk_i), .rst_i(rst_i), .retire_i(retire && !gc_active_o),
        .root_address_i(stack_root_address),.root_o(stack_root),
        .sp_operation_i(instruction.sp), .esp_operation_i(instruction.esp),
        .csp_operation_i(instruction.csp), .evaluation_operation_i(instruction.estk),
        .control_operation_i(instruction.cstk), .load_ap_i(instruction.load_ap),
        .bus_i(bus_value), .alu_i(alu_result), .compact_code_i(instruction.compact_code),
        .branch_i(instruction.branch), .upcor_i(upcor_o), .apc_i(apc_o),
        .bounds_fault_o(stack_fault), .compact_fault_o(compact_fault),
        .sp_o(sp_o), .esp_o(esp_o), .csp_o(csp_o), .ap_o(ap_o),
        .estkr_o(estkr_o), .cstkr_o(cstkr_o), .debug_address_i(dbg_addr_i),
        .debug_evaluation_o(dbg_estk_o), .debug_control_o(dbg_cstk_o)
    );

    numerik arithmetic (
        .save_i(gc_enter),.restore_i(gc_exit),
        .clk_i(clk_i), .rst_i(rst_i), .retire_i(retire),
        .operation_i(instruction.alu), .ra_i(instruction.ra), .rb_i(instruction.rb),
        .r_source_i(instruction.r_source), .s_source_i(instruction.s_source),
        .carry_source_i(instruction.carry), .shift_i(instruction.shift),
        .write_register_i(instruction.write_register), .load_q_i(instruction.load_q),
        .write_flags_i(instruction.write_flags), .bus_i(bus_value[31:0]),
        .estkr_i(estkr_o[31:0]), .branch_i(instruction.branch),
        .y_o(alu_result), .register_a_o(register_a), .q_o(q_o),
        .product_o(product_o), .flags_o(flags_o),
        .debug_register_i(dbg_addr_i[3:0]), .debug_register_o(dbg_rf_o)
    );

    logik_sequencer #(.CODE_WORDS(CODE_WORDS)) sequencer (
        .clk_i(clk_i), .rst_i(rst_i), .retire_i(retire), .save_i(gc_enter),.restore_i(gc_exit),.handler_i(gc_entry_i), .start_i(start), .entry_i(entry_i),
        .operation_i(instruction.sequence_op), .condition_i(instruction.condition),
        .invert_i(instruction.invert_condition), .halt_i(instruction.halt),
        .load_mark_i(instruction.load_mark), .branch_i(instruction.branch),
        .dispatch_i(ucar_o), .control_top_i(cstkr_o), .bus_i(bus_value), .symbol_i(symbol),
        .flags_i(flags_o), .object_ok_i(object_status == 0), .irq_i(irq_i),
        .use_issued_condition_i(state == ISSUE || state == WAIT_RESPONSE || (!gc_active_o && retried)),
        .issued_condition_i((!gc_active_o && retried) ? saved_issued_condition : issued_condition), .condition_o(condition_value),
        .target_fault_o(target_fault), .pc_o(pc_o), .upcor_o(upcor_o), .mark_o(mark_o),
        .last_condition_o(lastcc_o)
    );

    // The D bus retains full object width for stack, symbol, and OBJEKT paths.
    // Arithmetic/address sources zero-extend their narrower, untagged values.
    // Y is not fed back into D in the same instruction, avoiding a combinational
    // ALU/source loop. Microcode can write Y to a register or stack first.
    always_comb begin
        unique case (instruction.bus_source)
            BUS_ESTK: bus_value = estkr_o;
            BUS_CSTK: bus_value = {16'b0, cstkr_o};
            BUS_OBJECT: bus_value = object_result;
            BUS_REGISTER: bus_value = {8'b0, register_a};
            BUS_APC: bus_value = {16'b0, apc_o};
            BUS_AP: bus_value = {16'b0, ap_o};
            BUS_SP: bus_value = {16'b0, sp_o};
            BUS_NAMARG: bus_value = {10'b0, namarg_o};
            BUS_UPCOR: bus_value = {24'b0, upcor_o};
            BUS_Q: bus_value = {8'b0, q_o};
            BUS_SYMBOL: bus_value = symbol;
            default: bus_value = instruction.immediate;
        endcase
    end

    // Validation order is architectural. Reject invalid local effects before
    // granting ISSUE, even if the accompanying OBJEKT operation would succeed.
    // The decoded struct removes bit offsets from executable control logic.
    always_comb begin
        local_fault = FAULT_NONE;
        if (!instruction_valid) local_fault = FAULT_CODE;
        else if (instruction.reserved_high != 0 || instruction.reserved_113 ||
            instruction.reserved_98 || instruction.reserved_94 ||
            instruction.bus_source > BUS_SYMBOL || instruction.condition > CC_INTERRUPT ||
            instruction.alu > ALU_PRODUCT_LOW || instruction.r_source > SOURCE_BRANCH ||
            instruction.s_source > SOURCE_BRANCH || instruction.carry > CARRY_ZERO_FLAG ||
            instruction.estk > ESTK_COMPACT || instruction.cstk > CSTK_DECREMENT ||
            instruction.compact_code > 3) local_fault = FAULT_ENCODING;
        else if(instruction.recovery>10 ||
            (instruction.recovery!=0 && !gc_active_o) ||
            (instruction.allocation_dynamic && register_a[31:24]!=0) ||
            (gc_active_o && (instruction.object_enable || instruction.halt || instruction.sequence_op==SEQ_SERVICE ||
                instruction.sp!=0 || instruction.esp!=0 || instruction.csp!=0 || instruction.estk!=0 ||
                instruction.cstk!=0 || instruction.load_ap || instruction.apc!=0 || instruction.fetch_nam || instruction.fetch_map)) ||
            (instruction.recovery==10 && !gc_committed_i)) local_fault=FAULT_ENCODING;
        else if (target_fault && instruction.recovery!=10) local_fault = FAULT_CODE;
        else if (stack_fault) local_fault = FAULT_STACK;
        else if (fetch_fault) local_fault = FAULT_FETCH;
        else if (compact_fault) local_fault = FAULT_ENCODING;
    end

    always_comb begin
        next_state = state;
        retire = 1'b0;
        unique case (state)
            HALTED: if (start) next_state = EXECUTE;
            EXECUTE: begin
                if (local_fault != FAULT_NONE) next_state = HALTED;
                // HOLD freezes the whole instruction, unconditionally. Relative
                // zero is different: it retires effects and then revisits itself.
                else if (instruction.sequence_op != SEQ_HOLD) begin
                    if (instruction.object_enable || (instruction.recovery!=0 && instruction.recovery!=10)) next_state = ISSUE;
                    else retire = 1'b1;
                end
            end
            ISSUE: if (gc_active_o ? gc_ready_i : cmd_ready_i) next_state = WAIT_RESPONSE;
            WAIT_RESPONSE: if (response_valid) begin
                if(gc_enter) next_state=EXECUTE;
                else if (response_status != 0) next_state = HALTED;
                else begin
                    next_state = EXECUTE;
                    retire = local_fault == FAULT_NONE;
                end
            end
            SERVICE_BREAK: if (resume_i) next_state = EXECUTE;
            default: next_state = HALTED;
        endcase
        if(gc_exit) begin
            retire=0;
            next_state=gc_failed ? HALTED : EXECUTE;
        end
        if (retire) begin
            if (instruction.halt) next_state = HALTED;
            else if (instruction.sequence_op == SEQ_SERVICE && condition_value)
                next_state = SERVICE_BREAK;
        end
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            state <= HALTED;
            gc_active_o<=0; retried<=0; saved_symbol<=0; saved_object<=0; saved_bus<=0; saved_type<=0;
            saved_status<=0; saved_issued_condition<=0; last_status_o<=0;
            retire_o <= 1'b0;
            fault_o <= FAULT_NONE;
            issued_condition <= 1'b0;
            symbol <= '0;
            object_result <= '0;
            object_status <= '0;
            service_code <= '0;
        end else begin
            state <= next_state;
            retire_o <= retire;
            if (boot_fault) fault_o <= FAULT_ENCODING;
            if (start) begin fault_o <= FAULT_NONE; retried<=0; end
            if (state == EXECUTE) begin
                if (local_fault != FAULT_NONE) fault_o <= local_fault;
                else if (next_state == ISSUE) issued_condition <= (!gc_active_o && retried) ? saved_issued_condition : condition_value;
            end
            if (state == WAIT_RESPONSE && response_valid) begin
                last_status_o<=response_status;
                if(response_status!=0 && !gc_enter) fault_o<=FAULT_OBJECT;
            end
            if(gc_enter) begin
                gc_active_o<=1; retried<=1;
                saved_symbol<=symbol; saved_object<=object_result; saved_status<=object_status;
                saved_bus<=bus_value; saved_type<=instruction.check_type ? instruction.expected_type : 40'b0;
                saved_issued_condition<=issued_condition;
            end
            if(gc_exit) begin
                gc_active_o<=0; symbol<=saved_symbol; object_result<=saved_object; object_status<=saved_status;
            end
            if (retire) begin
                if(!gc_active_o) retried<=0;
                if (instruction.load_symbol) symbol <= bus_value;
                // Response data becomes a source for the NEXT instruction.
                // Other simultaneous destinations still consume the old D bus.
                if (state == WAIT_RESPONSE) begin
                    object_result <= response_data;
                    object_status <= response_status;
                end
                if (!instruction.halt && instruction.sequence_op == SEQ_SERVICE && condition_value)
                    service_code <= instruction.branch[3:0];
            end
        end
    end

    // OBJEKT retains its existing independently encoded command interface.
    assign pager_o = instruction.object_pager;
    assign index_o = instruction.object_index;
    assign register_o = instruction.object_register;
    assign memory_o = instruction.object_memory;
    assign read_o = instruction.object_read;
    assign load_vr_o = instruction.load_value_register;
    assign vr_o = instruction.value_register;
    assign check_type_o = instruction.check_type;
    assign expected_type_o = instruction.expected_type;
    assign alloc_size_o = instruction.allocation_dynamic ? register_a[23:0] : instruction.allocation_size;
    assign alloc_scan_o = instruction.allocation_scan;
    assign data_o = bus_value;
    assign symbol_o = symbol;
    assign object_o = object_result;
endmodule

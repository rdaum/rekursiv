// Resident control and evaluation stacks, including their cached data registers.
//
// Book pp. 69-84 and 152-162 distinguish pointer movement from data access.
// SP denotes the logical evaluation top; ESP selects an evaluation address.
// CSTKR and ESTKR cache the last value READ OR WRITTEN. Moving a pointer does
// not refresh either cache. A return reads old CSTKR and does not pop by itself.
//
// Project choices: fixed resident segments, zero-based addresses, and explicit
// bounds faults before retirement. Slot zero is usable and is the reset top;
// there is no separate "empty" count. Software defines its own frame layout.
// Evaluation values retain all 40 bits. Control entries hold 24-bit addresses
// and counters; they cannot hold object references. Segmented paging is future
// work, not a hidden truncation or wraparound behavior in this implementation.
module logik_stacks #(
    parameter integer STACK_WORDS = 32
) (
    input logic clk_i, rst_i, retire_i,
    input logic [1:0] sp_operation_i, esp_operation_i, csp_operation_i,
    input logic [2:0] evaluation_operation_i,
    input logic [3:0] control_operation_i,
    input logic load_ap_i,
    input logic [39:0] bus_i,
    input logic [31:0] alu_i,
    input logic [5:0] compact_code_i,
    input logic [15:0] branch_i, upcor_i,
    input logic [23:0] apc_i,
    output logic bounds_fault_o, compact_fault_o,
    output logic [23:0] sp_o, esp_o, csp_o, ap_o,
    output logic [39:0] estkr_o,
    output logic [23:0] cstkr_o,
    input logic [15:0] debug_address_i, root_address_i,
    output logic [39:0] root_o,
    output logic [39:0] debug_evaluation_o,
    output logic [23:0] debug_control_o
);
    `include "rekursiv_control.svh"
    localparam integer ADDRESS_BITS = $clog2(STACK_WORDS);

    object_word_t evaluation [0:STACK_WORDS-1];
    stack_address_t control [0:STACK_WORDS-1];
    stack_address_t sp, esp, csp, ap;
    object_word_t estkr, evaluation_write;
    stack_address_t cstkr, control_write;
    logic [24:0] next_sp, next_esp, next_csp;

    // Calculate with an extra bit, then validate before narrowing. Decrementing
    // zero produces a value outside the resident range and must fault.
    // ESP=SP deliberately forwards the newly calculated SP. All memory accesses
    // below still use the OLD address; this separates address and cache timing.
    always_comb begin
        next_sp = {1'b0, sp};
        next_esp = {1'b0, esp};
        next_csp = {1'b0, csp};
        unique case (sp_operation_i)
            POINTER_BUS: next_sp = {1'b0, bus_i[23:0]};
            POINTER_INCREMENT: next_sp = {1'b0, sp} + 1'b1;
            POINTER_DECREMENT: next_sp = {1'b0, sp} - 1'b1;
            default: ;
        endcase
        unique case (csp_operation_i)
            POINTER_BUS: next_csp = {1'b0, bus_i[23:0]};
            POINTER_INCREMENT: next_csp = {1'b0, csp} + 1'b1;
            POINTER_DECREMENT: next_csp = {1'b0, csp} - 1'b1;
            default: ;
        endcase
        unique case (esp_operation_i)
            ADDRESS_BUS: next_esp = {1'b0, bus_i[23:0]};
            ADDRESS_SP: next_esp = next_sp;
            ADDRESS_ARGUMENT: next_esp = {1'b0, ap} + {9'b0, branch_i};
            default: ;
        endcase

        evaluation_write = bus_i;
        if (evaluation_operation_i == ESTK_ALU) evaluation_write = {8'b0, alu_i};
        // Wide construction joins an explicit upper byte with the ALU's low
        // word. It is a raw bit operation; consumers validate tags separately.
        if (evaluation_operation_i == ESTK_WIDE) evaluation_write = {bus_i[7:0],alu_i};
        // The project has a six-bit compact code. The original book's five-bit
        // FLAGX plus unused bit is source evidence, not our serialized format.
        if (evaluation_operation_i == ESTK_COMPACT)
            evaluation_write = {2'b11, compact_code_i, alu_i};

        control_write = bus_i[23:0];
        unique case (control_operation_i)
            CSTK_UPCOR: control_write = {8'b0, upcor_i};
            CSTK_APC: control_write = apc_i;
            CSTK_AP: control_write = ap;
            CSTK_SP: control_write = sp;
            CSTK_INCREMENT: control_write = cstkr + 1'b1;
            CSTK_DECREMENT: control_write = cstkr - 1'b1;
            default: ;
        endcase

        // Do not confuse pointer overflow with overflow of a stored counter.
        // Both are rejected before either memory or cached state can change.
        bounds_fault_o = {7'b0, next_sp} >= STACK_WORDS ||
            {7'b0, next_esp} >= STACK_WORDS || {7'b0, next_csp} >= STACK_WORDS ||
            (((sp_operation_i == POINTER_BUS) || (esp_operation_i == ADDRESS_BUS) ||
              (csp_operation_i == POINTER_BUS) || load_ap_i) && bus_i[39:24] != 0) ||
            (load_ap_i && {8'b0, bus_i[23:0]} >= STACK_WORDS) ||
            (control_operation_i == CSTK_BUS && bus_i[39:24] != 0) ||
            (control_operation_i == CSTK_INCREMENT && cstkr == 24'hffffff) ||
            (control_operation_i == CSTK_DECREMENT && cstkr == 0);
        compact_fault_o = evaluation_operation_i == ESTK_COMPACT &&
            ((compact_code_i == 0 && alu_i != 0) || (compact_code_i == 1 && alu_i > 1));
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            sp <= '0;
            esp <= '0;
            csp <= '0;
            ap <= '0;
            estkr <= '0;
            cstkr <= '0;
            for (int index = 0; index < STACK_WORDS; index++) begin
                evaluation[index] <= '0;
                control[index] <= '0;
            end
        end else if (retire_i) begin
            sp <= next_sp[23:0];
            esp <= next_esp[23:0];
            csp <= next_csp[23:0];
            if (load_ap_i) ap <= bus_i[23:0];

            // Data access uses the pre-edge address. A simultaneous pointer
            // change prepares the *next* access. Every write refreshes the cache,
            // including argument stores away from the current logical top.
            if (evaluation_operation_i == ESTK_READ) begin
                estkr <= evaluation[esp[ADDRESS_BITS-1:0]];
            end else if (evaluation_operation_i != ESTK_HOLD) begin
                evaluation[esp[ADDRESS_BITS-1:0]] <= evaluation_write;
                estkr <= evaluation_write;
            end
            if (control_operation_i == CSTK_READ) begin
                cstkr <= control[csp[ADDRESS_BITS-1:0]];
            end else if (control_operation_i != CSTK_HOLD) begin
                control[csp[ADDRESS_BITS-1:0]] <= control_write;
                cstkr <= control_write;
            end
        end
    end

    assign root_o={16'b0,root_address_i}<STACK_WORDS && {8'b0,root_address_i}<=sp ?
        evaluation[root_address_i[ADDRESS_BITS-1:0]] : 40'b0;
    assign sp_o = sp;
    assign esp_o = esp;
    assign csp_o = csp;
    assign ap_o = ap;
    assign estkr_o = estkr;
    assign cstkr_o = cstkr;
    assign debug_evaluation_o = {16'b0, debug_address_i} < STACK_WORDS ?
        evaluation[debug_address_i[ADDRESS_BITS-1:0]] : 40'b0;
    assign debug_control_o = {16'b0, debug_address_i} < STACK_WORDS ?
        control[debug_address_i[ADDRESS_BITS-1:0]] : 24'b0;
endmodule

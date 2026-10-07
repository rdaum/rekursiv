// Writable microcode, abstract instruction memory (NAM), and opcode map (CSMAP).
//
// The halted programming interface writes 32-bit lanes. A word becomes readable
// only after every required lane is present: eight lanes for microcode, two for
// NAM, one for CSMAP. Reset invalidates images without resetting the large data
// arrays. This permits memory inference and prevents execution of stale data.
//
// Book pp. 66-68 and 174 describe a pipelined fetch. fetch_nam_i captures an
// opcode and operand from OLD APC. fetch_map_i captures the entry for the OLD
// opcode. Both can retire together; there is deliberately no opcode forwarding.
// Microcode must explicitly prime/refill the pipeline around control transfers.
// The project preserves these dependencies, not the original clock schedule.
module logik_store #(
    parameter integer CODE_WORDS = 256,
    parameter integer NAM_WORDS = 256
) (
    input logic clk_i, rst_i, retire_i,
    input logic boot_write_i,
    input logic [1:0] boot_space_i,
    input logic [15:0] boot_address_i,
    input logic [2:0] boot_lane_i,
    input logic [31:0] boot_data_i,
    output logic boot_fault_o,
    input logic [15:0] pc_i,
    output logic [255:0] instruction_o,
    output logic instruction_valid_o,
    input logic [1:0] apc_operation_i,
    input logic [39:0] bus_i,
    input logic fetch_nam_i, fetch_map_i,
    input logic runtime_root_write_i, input logic [4:0] runtime_root_address_i,
    output logic [39:0] runtime_root_o,
    output logic fetch_fault_o,
    output logic [23:0] apc_o,
    output logic [15:0] dispatch_o,
    output logic [29:0] operand_o,
    input logic [15:0] debug_address_i, root_address_i,
    output logic [39:0] root_literal_o, root_type_o, root_extra_o,
    output logic debug_valid_o,
    output logic [39:0] debug_literal_o, debug_type_o, debug_root_o
);
    `include "rekursiv_control.svh"
    localparam integer CODE_BITS = $clog2(CODE_WORDS);
    localparam integer NAM_BITS = $clog2(NAM_WORDS);

    logic [255:0] control_store [0:CODE_WORDS-1];
    object_word_t nam [0:NAM_WORDS-1];
    micro_address_t opcode_map [0:1023];
    logic [8*CODE_WORDS-1:0] control_valid;
    logic [2*NAM_WORDS-1:0] nam_valid;
    logic [1023:0] map_valid;
    stack_address_t apc;
    logic [24:0] next_apc;
    logic [9:0] opcode;
    logic [29:0] operand;
    micro_address_t dispatch;
    // GC observes only reference-bearing fields of this complete decoded word.
    /* verilator lint_off UNUSEDSIGNAL */
    microinstruction_t inspected_word, root_word;
    /* verilator lint_on UNUSEDSIGNAL */
    object_word_t extra_roots [0:31];
    logic [63:0] roots_valid;
    logic write_control, write_nam, write_map, write_root;
    logic pc_in_range;

    assign pc_in_range = {16'b0, pc_i} < CODE_WORDS;
    assign instruction_o = pc_in_range ? control_store[pc_i[CODE_BITS-1:0]] : 256'b0;
    assign instruction_valid_o = pc_in_range &&
        control_valid[pc_i[CODE_BITS-1:0]*8 +: 8] == 8'hff;

    always_comb begin
        write_control = boot_space_i == 0 && {16'b0, boot_address_i} < CODE_WORDS;
        write_nam = boot_space_i == 1 && {16'b0, boot_address_i} < NAM_WORDS && boot_lane_i < 2;
        write_map = boot_space_i == 2 && boot_address_i < 1024 && boot_lane_i == 0;
        write_root = boot_space_i == 3 && boot_address_i < 32 && boot_lane_i < 2;
        boot_fault_o = boot_write_i && !(write_control || write_nam || write_map || write_root);

        next_apc = {1'b0, apc};
        unique case (apc_operation_i)
            APC_BUS: next_apc = {1'b0, bus_i[23:0]};
            APC_INCREMENT: next_apc = {1'b0, apc} + 1'b1;
            // STEPAPC uses an unsigned offset, as specified on book p. 150.
            APC_STEP: next_apc = {1'b0, apc} + {1'b0, bus_i[23:0]};
            default: ;
        endcase
        fetch_fault_o = next_apc[24] ||
            ((apc_operation_i == APC_BUS || apc_operation_i == APC_STEP) && bus_i[39:24] != 0);
        if (fetch_nam_i) begin
            if ({8'b0, apc} >= NAM_WORDS) fetch_fault_o = 1'b1;
            else if (nam_valid[apc[NAM_BITS-1:0]*2 +: 2] != 2'b11) fetch_fault_o = 1'b1;
        end
        if (fetch_map_i && !map_valid[opcode]) fetch_fault_o = 1'b1;
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            // Eight lane-valid bits per control word. A larger control store
            // deliberately needs a reset vector wider than Verilator's 8K
            // replication heuristic; the data arrays themselves are not reset.
            /* verilator lint_off WIDTHCONCAT */
            control_valid <= '0;
            /* verilator lint_on WIDTHCONCAT */
            nam_valid <= '0;
            map_valid <= '0; roots_valid<='0;
            apc <= '0;
            opcode <= '0;
            operand <= '0;
            dispatch <= '0;
        end else begin
            // boot_write_i is gated by the top-level HALTED state. Execution and
            // programming cannot race, so an outstanding command sees stable
            // control bits without needing to duplicate the entire control word.
            if (boot_write_i) begin
                if (write_control) begin
                    control_store[boot_address_i[CODE_BITS-1:0]][boot_lane_i*32 +: 32] <= boot_data_i;
                    control_valid[boot_address_i[CODE_BITS-1:0]*8 + {29'b0, boot_lane_i}] <= 1'b1;
                end else if (write_nam) begin
                    if (boot_lane_i == 0) nam[boot_address_i[NAM_BITS-1:0]][31:0] <= boot_data_i;
                    else nam[boot_address_i[NAM_BITS-1:0]][39:32] <= boot_data_i[7:0];
                    nam_valid[boot_address_i[NAM_BITS-1:0]*2 + {31'b0, boot_lane_i[0]}] <= 1'b1;
                end else if(write_root) begin
                    if(boot_lane_i==0) extra_roots[boot_address_i[4:0]][31:0]<=boot_data_i;
                    else extra_roots[boot_address_i[4:0]][39:32]<=boot_data_i[7:0];
                    roots_valid[boot_address_i[4:0]*2+{31'b0,boot_lane_i[0]}]<=1;
                end else if (write_map) begin
                    opcode_map[boot_address_i[9:0]] <= boot_data_i[15:0];
                    map_valid[boot_address_i[9:0]] <= 1'b1;
                end
            end
            if (retire_i) begin
                // Explicit roots are generic tagged storage. Retire gates this
                // write together with other local effects; the collector freezes
                // mutator retirement, so registrations cannot change mid-scan.
                if (runtime_root_write_i) begin
                    extra_roots[runtime_root_address_i] <= bus_i;
                    roots_valid[runtime_root_address_i*2 +: 2] <= 2'b11;
                end
                apc <= next_apc[23:0];
                if (fetch_nam_i) begin
                    opcode <= nam[apc[NAM_BITS-1:0]][39:30];
                    operand <= nam[apc[NAM_BITS-1:0]][29:0];
                end
                if (fetch_map_i) dispatch <= opcode_map[opcode];
            end
        end
    end

    // Halted GC uses these read-only outputs to retain tagged constants embedded
    // in microcode. Raw NAM opcode words are not roots: their high bits are code.
    assign debug_valid_o = {16'b0, debug_address_i} < CODE_WORDS &&
        control_valid[debug_address_i[CODE_BITS-1:0]*8 +: 8] == 8'hff;
    assign inspected_word = debug_valid_o ? control_store[debug_address_i[CODE_BITS-1:0]] : 256'b0;
    assign debug_literal_o = inspected_word.immediate;
    assign debug_type_o = inspected_word.object_enable && inspected_word.check_type ?
        inspected_word.expected_type : 40'b0;
    assign root_word={16'b0,root_address_i}<CODE_WORDS &&
        control_valid[root_address_i[CODE_BITS-1:0]*8 +: 8]==8'hff ?
        control_store[root_address_i[CODE_BITS-1:0]] : 256'b0;
    assign root_literal_o=root_word.immediate;
    assign root_type_o=root_word.object_enable && root_word.check_type ? root_word.expected_type : 40'b0;
    assign root_extra_o=root_address_i<32 && roots_valid[root_address_i[4:0]*2 +: 2]==2'b11 ?
        extra_roots[root_address_i[4:0]] : 40'b0;
    // A separate read port avoids feeding the collector's D-bus-derived root
    // address back into the mutator D bus through Root selection.
    assign runtime_root_o = roots_valid[runtime_root_address_i*2 +: 2] == 2'b11 ?
        extra_roots[runtime_root_address_i] : 40'b0;
    assign debug_root_o = debug_address_i < 32 && roots_valid[debug_address_i[4:0]*2 +: 2] == 2'b11 ?
        extra_roots[debug_address_i[4:0]] : 40'b0;
    assign apc_o = apc;
    assign dispatch_o = dispatch;
    assign operand_o = operand;
endmodule

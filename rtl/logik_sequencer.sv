// Microaddress selection and condition history.
//
// Book pp. 188-193 establish the important sequencing distinctions:
//   * Relative targets are current-address + offset, not successor + offset.
//   * Ordinary taken jumps save the successor in UPCOR, but do not push it.
//   * RET reads CSTKR; a separate stack operation removes the return entry.
//   * A service break resumes at the successor and preserves UPCOR.
//
// Project choices: conditions use registered arithmetic flags; LASTCC is the
// selected, inverted condition from the previous retirement. Direct saved-return
// preserves UPCOR on both paths. Targets are checked before narrowing to 16 bits.
module logik_sequencer #(
    parameter integer CODE_WORDS = 256
) (
    input logic clk_i, rst_i, retire_i,
    input logic start_i, input logic save_i, restore_i,
    input logic [15:0] handler_i,
    input logic [15:0] entry_i,
    input logic [3:0] operation_i, condition_i,
    input logic invert_i, halt_i, load_mark_i,
    input logic [15:0] branch_i, dispatch_i,
    input logic [23:0] control_top_i,
    input logic [39:0] bus_i, symbol_i,
    input logic [4:0] flags_i,
    input logic object_ok_i, irq_i,
    input logic use_issued_condition_i, issued_condition_i,
    output logic condition_o, target_fault_o,
    output logic [15:0] pc_o, upcor_o, mark_o,
    output logic last_condition_o
);
    `include "rekursiv_control.svh"
    micro_address_t pc, upcor, mark, saved_pc, saved_upcor, saved_mark;
    logic saved_condition;
    arithmetic_flags_t flags;
    logic last_condition, selected_condition, save_return;
    logic signed [40:0] target;
    assign flags = flags_i;

    always_comb begin
        unique case (condition_i)
            CC_ALWAYS: selected_condition = 1'b1;
            CC_ZERO: selected_condition = flags.zero;
            CC_SIGN: selected_condition = flags.sign;
            CC_CARRY: selected_condition = flags.carry;
            CC_OVERFLOW: selected_condition = flags.overflow;
            CC_CORRECTED_SIGN: selected_condition = flags.corrected_sign;
            // Identity is a 40-bit comparison, not a 32-bit ALU zero test.
            CC_SYMBOL: selected_condition = bus_i == symbol_i;
            CC_LAST: selected_condition = last_condition;
            CC_CONTROL_ZERO: selected_condition = control_top_i == 0;
            CC_OBJECT_OK: selected_condition = object_ok_i;
            CC_INTERRUPT: selected_condition = irq_i;
            default: selected_condition = 1'b0;
        endcase
        condition_o = use_issued_condition_i ? issued_condition_i :
            (selected_condition ^ invert_i);
    end

    always_comb begin
        target = $signed({25'b0, pc}) + 41'sd1;
        save_return = 1'b0;
        unique case (operation_i)
            SEQ_HOLD: target = $signed({25'b0, pc});
            SEQ_JUMP, SEQ_CONDITIONAL_JUMP:
                if (operation_i == SEQ_JUMP || condition_o) begin
                    target = $signed({25'b0, branch_i});
                    save_return = 1'b1;
                end
            SEQ_RELATIVE, SEQ_CONDITIONAL_RELATIVE:
                if (operation_i == SEQ_RELATIVE || condition_o) begin
                    target = $signed({25'b0, pc}) + $signed({bus_i[39], bus_i});
                    save_return = 1'b1;
                end
            SEQ_BUS, SEQ_CONDITIONAL_BUS:
                if (operation_i == SEQ_BUS || condition_o) begin
                    target = $signed({1'b0, bus_i});
                    save_return = 1'b1;
                end
            SEQ_RETURN, SEQ_CONDITIONAL_RETURN:
                if (operation_i == SEQ_RETURN || condition_o) begin
                    target = $signed({17'b0, control_top_i});
                    save_return = 1'b1;
                end
            SEQ_DISPATCH, SEQ_CONDITIONAL_DISPATCH:
                if (operation_i == SEQ_DISPATCH || condition_o) begin
                    target = $signed({25'b0, dispatch_i});
                    save_return = 1'b1;
                end
            SEQ_CONDITIONAL_MARK:
                if (condition_o) begin
                    target = $signed({25'b0, mark});
                    save_return = 1'b1;
                end
            SEQ_TWO_WAY: begin
                target = $signed({25'b0, (condition_o ? branch_i : mark)});
                save_return = 1'b1;
            end
            SEQ_SAVED_RETURN:
                if (condition_o) target = $signed({25'b0, upcor});
            default: ; // Continuation and service break both select the successor.
        endcase
        // HALT retains the address of the retiring instruction for inspection.
        if (halt_i) target = $signed({25'b0, pc});
        target_fault_o = target < 0 || target >= 41'(CODE_WORDS);
    end

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            pc <= '0; saved_pc<='0; saved_upcor<='0; saved_mark<='0; saved_condition<=0;
            upcor <= '0;
            mark <= '0;
            last_condition <= 1'b0;
        end else if(save_i) begin
            saved_pc<=pc; saved_upcor<=upcor; saved_mark<=mark; saved_condition<=last_condition;
            pc<=handler_i;
        end else if(restore_i) begin
            pc<=saved_pc; upcor<=saved_upcor; mark<=saved_mark; last_condition<=saved_condition;
        end else if (start_i) begin
            pc <= entry_i;
        end else if (retire_i) begin
            pc <= target[15:0];
            if (save_return) upcor <= pc + 1'b1;
            if (load_mark_i) mark <= pc;
            last_condition <= condition_o;
        end
    end

    assign pc_o = pc;
    assign upcor_o = upcor;
    assign mark_o = mark;
    assign last_condition_o = last_condition;
endmodule

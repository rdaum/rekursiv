// An Atari ST mouse on a joystick port: quadrature on the four direction
// lines (pin 1 XB, 2 XA, 3 YA, 4 YB) and the left button on the fire line.
// Each edge of either phase is one step. Outputs pulse for one cycle.
module mouse (
    input  logic clk_i,
    input  logic rst_i,
    input  logic up_n_i,      // pin 1: XB
    input  logic down_n_i,    // pin 2: XA
    input  logic left_n_i,    // pin 3: YA
    input  logic right_n_i,   // pin 4: YB
    input  logic fire_n_i,    // pin 6: left button
    output logic x_step_o,
    output logic x_negative_o,
    output logic y_step_o,
    output logic y_negative_o,
    output logic button_o
);
    logic [4:0] first, second, settled;
    logic [15:0] debounce;
    always_ff @(posedge clk_i) begin
        first <= {fire_n_i, right_n_i, left_n_i, down_n_i, up_n_i};
        second <= first;
        settled <= second;
        // {A, B} advances 00, 01, 11, 10 in one direction.
        x_step_o <= second[1:0] != settled[1:0];
        x_negative_o <= !(second[1] ^ settled[0]);
        y_step_o <= second[3:2] != settled[3:2];
        y_negative_o <= second[2] ^ settled[3];
        // The button must hold its new level for about 2 ms.
        if (rst_i) begin
            button_o <= 1'b0;
            debounce <= '0;
        end else if (button_o == !second[4]) debounce <= '0;
        else begin
            debounce <= debounce + 1'b1;
            if (&debounce) button_o <= !second[4];
        end
    end
endmodule

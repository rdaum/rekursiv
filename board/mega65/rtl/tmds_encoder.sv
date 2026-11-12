// DVI 1.0 TMDS 8b/10b encoder for one channel.
// During blanking (de = 0) it emits one of the four control symbols selected
// by c; channel 0 carries {vsync, hsync} there, the other channels carry 0.
module tmds_encoder (
    input  logic       clk,
    input  logic [7:0] d,
    input  logic [1:0] c,
    input  logic       de,
    output logic [9:0] q
);
    // Stage 1: transition minimisation.
    logic [3:0] ones_d;
    logic       use_xnor;
    logic [8:0] q_m;

    always_comb begin
        ones_d   = 4'($countones(d));
        use_xnor = (ones_d > 4'd4) || (ones_d == 4'd4 && !d[0]);
        q_m[0]   = d[0];
        for (int i = 1; i < 8; i++)
            q_m[i] = use_xnor ? ~(q_m[i-1] ^ d[i]) : (q_m[i-1] ^ d[i]);
        q_m[8] = ~use_xnor;
    end

    // Stage 2: DC balance. disparity is the running (ones - zeros) sent so far.
    logic signed [4:0] ones_q, zeros_q, disparity;

    always_comb begin
        ones_q  = 5'($countones(q_m[7:0]));
        zeros_q = 5'sd8 - ones_q;
    end

    always_ff @(posedge clk) begin
        if (!de) begin
            disparity <= '0;
            unique case (c)
                2'b00: q <= 10'b1101010100;
                2'b01: q <= 10'b0010101011;
                2'b10: q <= 10'b0101010100;
                2'b11: q <= 10'b1010101011;
            endcase
        end else if (disparity == 0 || ones_q == zeros_q) begin
            q <= {~q_m[8], q_m[8], q_m[8] ? q_m[7:0] : ~q_m[7:0]};
            disparity <= q_m[8] ? disparity + (ones_q - zeros_q)
                                : disparity + (zeros_q - ones_q);
        end else if ((disparity > 0 && ones_q > zeros_q) ||
                     (disparity < 0 && zeros_q > ones_q)) begin
            q <= {1'b1, q_m[8], ~q_m[7:0]};
            disparity <= disparity + (zeros_q - ones_q) + (q_m[8] ? 5'sd2 : 5'sd0);
        end else begin
            q <= {1'b0, q_m[8], q_m[7:0]};
            disparity <= disparity + (ones_q - zeros_q) - (q_m[8] ? 5'sd0 : 5'sd2);
        end
    end
endmodule

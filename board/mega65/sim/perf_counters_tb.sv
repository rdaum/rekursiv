// The live counters with a 1,000-cycle "second": periodic inputs give known
// counts per window, checked as decimal digits. Run by sim/perf_counters.sh.
module perf_counters_tb;
    logic clk = 0, rst = 1;
    always #5 clk = ~clk;

    logic [31:0] n = 0;
    logic [31:0] rows [8];
    // Per 1,000 cycles: 750 retirements, 125 dispatches, 100 commands,
    // 20 collector cycles, 4 store requests.
    perf_counters #(.CLK_HZ(1000)) dut (
        .clk_i(clk), .rst_i(rst),
        .retire_i(n % 4 != 0), .dispatch_i(n % 8 == 0), .command_i(n % 10 == 0),
        .collecting_i(n % 50 == 0), .store_i(n % 250 == 0), .rows_o(rows));

    always @(posedge clk) n <= n + 1;

    task automatic expect_row(input int row, input logic [31:0] want);
        if (rows[row] !== want) begin
            $display("FAIL row %0d: %h, expected %h", row, rows[row], want);
            $fatal(1);
        end
    endtask

    initial begin
        repeat (3) @(posedge clk);
        rst = 0;
        // Two full windows, then time for the conversion.
        repeat (3000) @(posedge clk);
        expect_row(0, 32'h0000_0125);
        expect_row(1, 32'h0000_0750);
        expect_row(2, 32'h0000_0600);   // 6.00 microinstructions per bytecode
        expect_row(3, 32'h0000_0100);
        expect_row(4, 32'h0000_0250);
        expect_row(5, 32'h0000_0020);
        expect_row(6, 32'h0000_0004);
        expect_row(7, 32'h0000_0002);
        $display("perf_counters: PASS");
        $finish;
    end
endmodule

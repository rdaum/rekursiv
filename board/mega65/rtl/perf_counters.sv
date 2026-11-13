// Live performance counters for the status panel. Over each second of
// machine clock it counts retired mutator microinstructions (the collector's
// are not counted), bytecode dispatches,
// accepted OBJEKT commands, collector cycles and accepted store requests.
// At the end of the second it latches the counts and converts them, one
// after another, to eight decimal digits each (one digit per nibble), so the
// hexadecimal panel shows them in decimal. A value above 99,999,999 shows as
// all nines. The conversion takes a few hundred cycles; the rows keep the
// previous second's digits until it ends.
//
// Rows:
//   0  bytecodes per second
//   1  mutator microinstructions per second
//   2  microinstructions per bytecode, in hundredths (00010123 is 101.23)
//   3  OBJEKT commands per second
//   4  cycles per second without a retirement (waiting or collecting)
//   5  collector cycles per second
//   6  store requests per second
//   7  seconds since reset
module perf_counters #(
    parameter integer CLK_HZ = 25_200_000
)(
    input  logic        clk_i,
    input  logic        rst_i,
    input  logic        retire_i,
    input  logic        dispatch_i,
    input  logic        command_i,
    input  logic        collecting_i,
    input  logic        store_i,
    output logic [31:0] rows_o [8]
);
    localparam integer VALUES = 8;

    logic [31:0] cycle, retired, dispatched, commands, collecting, stores, seconds;
    wire  second = cycle == 32'(CLK_HZ - 1);

    // Latched counts for the conversion.
    logic [31:0] value [VALUES];

    // Division for row 2: quotient = dividend / divisor, restoring, 32 steps.
    logic [31:0] quotient, divisor;
    logic [32:0] remainder;
    logic [5:0]  step;

    // Binary to decimal (double dabble), 32 steps per value.
    logic [31:0] binary, digits;
    logic [2:0]  index;

    typedef enum logic [1:0] {IDLE, DIVIDE, CONVERT} phase_t;
    phase_t phase;

    function automatic logic [31:0] adjust(input logic [31:0] bcd);
        for (int i = 0; i < 8; i++)
            if (bcd[4 * i +: 4] >= 4'd5) bcd[4 * i +: 4] = bcd[4 * i +: 4] + 4'd3;
        return bcd;
    endfunction

    always_ff @(posedge clk_i) begin
        if (rst_i) begin
            cycle <= '0; retired <= '0; dispatched <= '0; commands <= '0;
            collecting <= '0; stores <= '0; seconds <= '0;
            phase <= IDLE;
            for (int i = 0; i < VALUES; i++) rows_o[i] <= '0;
        end else begin
            if (second) begin
                cycle <= '0;
                retired <= 32'(retire_i);
                dispatched <= 32'(dispatch_i);
                commands <= 32'(command_i);
                collecting <= 32'(collecting_i);
                stores <= 32'(store_i);
                seconds <= seconds + 1'b1;
                value[0] <= dispatched;
                value[1] <= retired;
                value[3] <= commands;
                value[4] <= 32'(CLK_HZ) - retired;
                value[5] <= collecting;
                value[6] <= stores;
                value[7] <= seconds + 1'b1;
                // Hundredths of a microinstruction per bytecode.
                remainder <= '0;
                quotient <= retired * 32'd100;
                divisor <= dispatched;
                step <= '0;
                phase <= DIVIDE;
            end else begin
                cycle <= cycle + 1'b1;
                retired <= retired + 32'(retire_i);
                dispatched <= dispatched + 32'(dispatch_i);
                commands <= commands + 32'(command_i);
                collecting <= collecting + 32'(collecting_i);
                stores <= stores + 32'(store_i);
            end

            unique case (phase)
                IDLE: begin end
                DIVIDE: begin
                    // Shift the next dividend bit into the remainder; the
                    // quotient register holds the dividend's remaining bits.
                    logic [32:0] trial;
                    trial = {remainder[31:0], quotient[31]};
                    if (divisor != 0 && trial >= {1'b0, divisor}) begin
                        remainder <= trial - {1'b0, divisor};
                        quotient <= {quotient[30:0], 1'b1};
                    end else begin
                        remainder <= trial;
                        quotient <= {quotient[30:0], 1'b0};
                    end
                    step <= step + 1'b1;
                    if (step == 6'd31) begin
                        phase <= CONVERT;
                        index <= '0;
                        binary <= value[0];
                        digits <= '0;
                        step <= '0;
                    end
                end
                CONVERT: begin
                    if (step == 6'd32) begin
                        // Eight digits hold at most 99,999,999.
                        rows_o[index] <= binary_overflow(index) ? 32'h9999_9999 : digits;
                        step <= '0;
                        digits <= '0;
                        if (index == 3'(VALUES - 1)) phase <= IDLE;
                        else begin
                            index <= index + 1'b1;
                            binary <= index + 1'b1 == 3'd2 ? (divisor == 0 ? 32'd0 : quotient)
                                                             : value[index + 1'b1];
                        end
                    end else begin
                        logic [31:0] adjusted;
                        adjusted = adjust(digits);
                        digits <= {adjusted[30:0], binary[31]};
                        binary <= {binary[30:0], 1'b0};
                        step <= step + 1'b1;
                    end
                end
                default: phase <= IDLE;
            endcase
        end
    end

    // Whether the value being converted exceeds eight digits.
    function automatic logic binary_overflow(input logic [2:0] row);
        logic [31:0] v;
        v = row == 3'd2 ? (divisor == 0 ? 32'd0 : quotient) : value[row];
        return v > 32'd99_999_999;
    endfunction
endmodule

// The interactive devices of docs/devices.md behind one register port:
// counted events (0x100), clocks and the one-shot timer (0x200), the pointer
// (0x300, 0x400) and the input FIFO (0x310). A register access completes in
// the cycle it is presented; address_i is the device byte address.
//
// Differences from the device contract:
// - There is no real-time clock. UTC starts at zero on reset.
// - Coordinates are confined to the screen, WIDTH by HEIGHT.
// - When the FIFO is full a key transition is dropped, and motion is not
//   queued while it is more than half full. Neither sets the overrun flag,
//   which would stop the guest.
module workstation #(
    parameter integer CLK_HZ = 25_200_000,
    parameter integer WIDTH = 640,
    parameter integer HEIGHT = 480,
    parameter integer FIFO_BITS = 8
)(
    input  logic        clk_i,
    input  logic        rst_i,
    input  logic        valid_i,
    input  logic        write_i,
    input  logic [11:0] address_i,
    input  logic [31:0] data_i,
    output logic [31:0] result_o,
    output logic        error_o,
    output logic        irq_o,

    // Physical input: mouse steps, and key transitions by device key code.
    input  logic        x_step_i,
    input  logic        x_negative_i,
    input  logic        y_step_i,
    input  logic        y_negative_i,
    input  logic        key_valid_i,
    input  logic [7:0]  key_code_i,
    input  logic        key_down_i,
    input  logic        button_i,      // the mouse's left button: key 130

    output logic [10:0] cursor_x_o,
    output logic [10:0] cursor_y_o
);
    localparam integer MS_CYCLES = CLK_HZ / 1000;
    localparam logic [26:0] DAY_MS = 27'd86_400_000;

    // ---------------------------------------------------------------- clocks
    logic [15:0] phase;
    logic        ms_tick;
    logic [47:0] monotonic, monotonic_latch, deadline, staged_deadline;
    logic [31:0] utc, utc_latch;
    logic [9:0]  utc_fraction;
    logic [26:0] day_ms;
    logic        armed;
    wire         timer_due = armed && monotonic >= deadline;

    // ---------------------------------------------------------------- events
    logic [FIFO_BITS:0] input_events;
    logic [3:0]         timer_events;
    assign irq_o = input_events != 0 || timer_events != 0;

    // --------------------------------------------------------------- pointer
    logic [10:0] mouse_x, mouse_y, latched_y, staged_x, staged_y, sampled_x, sampled_y;
    logic        linked, sampled;
    logic [31:0] interval;
    logic [47:0] sample_time;
    wire         moved = sampled && (sampled_x != mouse_x || sampled_y != mouse_y) &&
                         monotonic - sample_time >= 48'(interval);

    // ------------------------------------------------------------ input FIFO
    // {kind[1:0], timestamp[26:0], extra[10:0], value[10:0]}
    logic [50:0] fifo [1 << FIFO_BITS];
    logic [FIFO_BITS-1:0] head, tail;
    logic [FIFO_BITS:0]   count;
    wire  [50:0] front = fifo[head];
    wire         empty = count == 0;
    wire         full = count[FIFO_BITS];
    wire         busy = count[FIFO_BITS] || count[FIFO_BITS-1];

    // Three sources share the FIFO's one write per cycle: a key transition,
    // the mouse button, and motion, in that order. A key transition lasts
    // one cycle, so it goes first; the button and motion wait their turn.
    logic        button_q, button_pending, button_down;
    wire         push_key = key_valid_i && !full;
    wire         push_button = !key_valid_i && button_pending && !full;
    wire         push_motion = !key_valid_i && !button_pending && moved && !busy;
    wire         push = push_key || push_button || push_motion;
    wire  [50:0] packet = push_motion ? {2'd1, day_ms, mouse_y, mouse_x} :
        push_button ? {button_down ? 2'd2 : 2'd3, day_ms, 11'd0, 11'd130} :
                      {key_down_i ? 2'd2 : 2'd3, day_ms, 11'd0, 3'd0, key_code_i};

    wire         access = valid_i && !error_o;
    wire         pop = access && write_i && address_i == 12'h324;

    always_comb begin
        result_o = '0;
        error_o = 1'b0;
        if (!write_i) begin
            unique case (address_i)
                12'h100: result_o = {30'd0, timer_events != 0, input_events != 0};
                12'h200: result_o = utc;
                12'h204: result_o = '0;
                12'h208: result_o = monotonic[31:0];
                12'h20c: result_o = {16'd0, monotonic_latch[47:32]};
                12'h300: result_o = 32'(mouse_x);
                12'h304: result_o = 32'(latched_y);
                12'h310: result_o = 32'(count);
                12'h314: begin result_o = 32'(front[50:49]); error_o = empty; end
                12'h318: begin result_o = 32'(front[10:0]); error_o = empty; end
                12'h31c: begin result_o = 32'(front[21:11]); error_o = empty; end
                12'h320: begin result_o = 32'(front[48:22]); error_o = empty; end
                default: error_o = 1'b1;
            endcase
        end else begin
            unique case (address_i)
                12'h104: error_o = data_i[31:4] != 0;
                12'h210, 12'h214, 12'h30c, 12'h400, 12'h404: error_o = 1'b0;
                12'h218, 12'h40c: error_o = data_i > 1;
                12'h324: error_o = data_i != 1 || empty;
                12'h408: error_o = data_i != 1;
                default: error_o = 1'b1;
            endcase
        end
    end

    function automatic logic [10:0] confine(input logic [31:0] value, input integer limit);
        confine = value[31] ? 11'd0 : value >= 32'(limit) ? 11'(limit - 1) : value[10:0];
    endfunction

    always_ff @(posedge clk_i) if (push) fifo[tail] <= packet;

    always_ff @(posedge clk_i) begin
        ms_tick <= 1'b0;
        if (rst_i) begin
            phase <= '0; monotonic <= '0; monotonic_latch <= '0; utc <= '0; utc_latch <= '0;
            utc_fraction <= '0; day_ms <= '0; armed <= 1'b0; deadline <= '0; staged_deadline <= '0;
            input_events <= '0; timer_events <= '0;
            mouse_x <= '0; mouse_y <= '0; latched_y <= '0; staged_x <= '0; staged_y <= '0;
            cursor_x_o <= '0; cursor_y_o <= '0; linked <= 1'b1; sampled <= 1'b0;
            sampled_x <= '0; sampled_y <= '0; interval <= '0; sample_time <= '0;
            head <= '0; tail <= '0; count <= '0;
            button_q <= 1'b0; button_pending <= 1'b0; button_down <= 1'b0;
        end else begin
            // Time.
            if (phase == 16'(MS_CYCLES - 1)) begin
                phase <= '0;
                ms_tick <= 1'b1;
                monotonic <= monotonic + 1'b1;
                day_ms <= day_ms == DAY_MS - 1'b1 ? '0 : day_ms + 1'b1;
                if (utc_fraction == 10'd999) begin
                    utc_fraction <= '0;
                    utc <= utc + 1'b1;
                end else utc_fraction <= utc_fraction + 1'b1;
            end else phase <= phase + 1'b1;

            // The mouse moves the pointer, and the cursor while they are linked.
            if (x_step_i) begin
                if (x_negative_i) begin if (mouse_x != 0) mouse_x <= mouse_x - 1'b1; end
                else if (mouse_x != 11'(WIDTH - 1)) mouse_x <= mouse_x + 1'b1;
            end
            if (y_step_i) begin
                if (y_negative_i) begin if (mouse_y != 0) mouse_y <= mouse_y - 1'b1; end
                else if (mouse_y != 11'(HEIGHT - 1)) mouse_y <= mouse_y + 1'b1;
            end
            if (linked) begin cursor_x_o <= mouse_x; cursor_y_o <= mouse_y; end

            // Input sources.
            button_q <= button_i;
            if (button_i != button_q) begin button_pending <= 1'b1; button_down <= button_i; end
            if (push_button) button_pending <= 1'b0;
            if (!sampled) begin
                sampled <= 1'b1; sampled_x <= mouse_x; sampled_y <= mouse_y;
                sample_time <= monotonic;
            end else if (push_motion) begin
                sampled_x <= mouse_x; sampled_y <= mouse_y; sample_time <= monotonic;
            end
            if (push) tail <= tail + 1'b1;
            if (pop) head <= head + 1'b1;
            count <= count + (FIFO_BITS+1)'(push) - (FIFO_BITS+1)'(pop);

            // Events. An arrival in the same cycle as an acknowledgement stays.
            if (timer_due) armed <= 1'b0;
            begin : events
                logic [FIFO_BITS:0] inputs;
                logic [3:0] timers;
                inputs = input_events;
                timers = timer_events;
                if (access && write_i && address_i == 12'h104) begin
                    if (data_i[0] && inputs != 0) inputs = inputs - 1'b1;
                    if (data_i[1] && timers != 0) timers = timers - 1'b1;
                end
                if (access && write_i && address_i == 12'h218) timers = '0;
                else if (timer_due && timers != 4'd15) timers = timers + 1'b1;
                if (push) inputs = inputs + 1'b1;
                input_events <= inputs;
                timer_events <= timers;
            end

            if (access && !write_i) unique case (address_i)
                12'h200: utc_latch <= utc;
                12'h208: monotonic_latch <= monotonic;
                12'h300: latched_y <= mouse_y;
                default: begin end
            endcase
            if (access && write_i) unique case (address_i)
                12'h210: staged_deadline[31:0] <= data_i;
                12'h214: staged_deadline[47:32] <= data_i[15:0];
                12'h218: begin armed <= data_i[0]; deadline <= staged_deadline; end
                12'h30c: interval <= data_i;
                12'h400: staged_x <= confine(data_i, WIDTH);
                12'h404: staged_y <= confine(data_i, HEIGHT);
                12'h408: begin
                    cursor_x_o <= staged_x; cursor_y_o <= staged_y;
                    if (linked) begin mouse_x <= staged_x; mouse_y <= staged_y; end
                end
                12'h40c: begin
                    linked <= data_i[0];
                    if (data_i[0]) begin cursor_x_o <= mouse_x; cursor_y_o <= mouse_y; end
                end
                default: begin end
            endcase
        end
    end
    wire unused = &{1'b0, utc_latch, ms_tick};
endmodule

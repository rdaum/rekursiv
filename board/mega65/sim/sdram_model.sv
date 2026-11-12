// Simulation model of an IS42S16320F-style SDRAM: 4 banks, 13 row bits,
// 10 column bits, 16 data bits. It follows the programmed CAS latency and
// burst length, and reports protocol violations: access to a closed row,
// commands during a burst, activation of an open bank, use before setup,
// and refresh starvation. Storage is sparse.
module sdram_model (
    input  logic        clk_i,
    input  logic        cke_i,
    input  logic        cs_n_i,
    input  logic        ras_n_i,
    input  logic        cas_n_i,
    input  logic        we_n_i,
    input  logic [1:0]  ba_i,
    input  logic [12:0] a_i,
    input  logic [15:0] dq_i,
    input  logic        dq_driven_i,
    output logic [15:0] dq_o
);
    logic [15:0] memory [int unsigned];
    logic        row_open [4];
    logic [12:0] row [4];
    logic        mode_set = 0, precharged = 0;
    int          refreshes = 0, cas_latency = 0, burst = 0;
    int          since_refresh = 0;
    int          errors = 0;

    // An active burst: remaining beats, and for reads the cycles until data.
    int          beats = 0, read_delay = 0;
    logic        writing = 0, auto_precharge = 0;
    logic [1:0]  burst_bank;
    int unsigned burst_address;

    function automatic int unsigned location(input logic [1:0] bank, input logic [12:0] r,
                                             input logic [9:0] column);
        return {7'b0, bank, r, column};
    endfunction

    task automatic violation(input string message);
        errors++;
        $display("sdram_model: %s at %0t", message, $time);
    endtask

    wire [2:0] command = {ras_n_i, cas_n_i, we_n_i};

    always @(posedge clk_i) begin
        dq_o = 16'hxxxx;
        since_refresh++;
        if (mode_set && since_refresh > 2000) begin
            violation("refresh starved");
            since_refresh = 0;
        end
        // Data phase of a burst in progress.
        if (beats != 0 && writing) begin
            if (!dq_driven_i) violation("write beat not driven");
            memory[burst_address] = dq_i;
            burst_address++;
            beats--;
        end else if (beats != 0) begin
            if (read_delay != 0) read_delay--;
            if (read_delay == 0) begin
                dq_o = memory.exists(burst_address) ? memory[burst_address] : 16'h0000;
                burst_address++;
                beats--;
            end
        end
        if (beats == 0 && auto_precharge) begin
            row_open[burst_bank] = 0;
            auto_precharge = 0;
        end

        if (cke_i && !cs_n_i && command != 3'b111) begin
            if (beats != 0) violation("command during a burst");
            unique case (command)
                3'b010: begin   // precharge
                    if (a_i[10]) for (int b = 0; b < 4; b++) row_open[b] = 0;
                    else row_open[ba_i] = 0;
                    precharged = 1;
                end
                3'b001: begin   // auto refresh
                    if (!precharged) violation("refresh before precharge");
                    for (int b = 0; b < 4; b++) if (row_open[b]) violation("refresh with a row open");
                    refreshes++;
                    since_refresh = 0;
                end
                3'b000: begin   // load mode
                    if (refreshes < 2) violation("mode set before two refreshes");
                    cas_latency = int'(a_i[6:4]);
                    burst = 1 << a_i[2:0];
                    if (a_i[9] || a_i[3]) violation("unsupported mode");
                    mode_set = 1;
                end
                3'b011: begin   // activate
                    if (!mode_set) violation("activate before mode set");
                    if (row_open[ba_i]) violation("activate on an open bank");
                    row_open[ba_i] = 1;
                    row[ba_i] = a_i;
                end
                3'b101, 3'b100: begin   // read, write
                    if (!row_open[ba_i]) violation("access to a closed row");
                    burst_bank = ba_i;
                    burst_address = location(ba_i, row[ba_i], a_i[9:0]);
                    auto_precharge = a_i[10];
                    beats = burst;
                    writing = command == 3'b100;
                    if (writing) begin
                        if (!dq_driven_i) violation("write beat not driven");
                        memory[burst_address] = dq_i;
                        burst_address++;
                        beats--;
                    end else read_delay = cas_latency - 1;
                end
                default: violation("unknown command");
            endcase
        end
    end
endmodule

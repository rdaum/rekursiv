// Controller against the card model: set up, read sector 1, write sector 2,
// read it back. Built and run by sim/sd.sh.
module sd_spi_tb;
    logic clk = 0, rst = 1;
    always #5 clk = ~clk;

    logic sd_clk, sd_cs_n, sd_mosi, sd_miso;
    logic valid, ready, write, done, error, failed, read_byte;
    logic [31:0] sector;
    logic [8:0]  read_index, write_index;
    logic [7:0]  read_data, write_data;

    sd_spi #(.SLOW_DIVIDE(4), .FAST_DIVIDE(1)) dut (
        .clk_i(clk), .rst_i(rst),
        .sd_clk_o(sd_clk), .sd_cs_n_o(sd_cs_n), .sd_mosi_o(sd_mosi), .sd_miso_i(sd_miso),
        .valid_i(valid), .ready_o(ready), .write_i(write), .sector_i(sector),
        .done_o(done), .error_o(error), .failed_o(failed), .sdhc_o(), .debug_o(),
        .read_byte_o(read_byte), .read_index_o(read_index), .read_data_o(read_data),
        .write_index_o(write_index), .write_data_i(write_data));

    sd_card_model #(.IMAGE("build/sd_test.img"), .SECTORS(64)) card (
        .sd_clk_i(sd_clk), .sd_cs_n_i(sd_cs_n), .sd_mosi_i(sd_mosi), .sd_miso_o(sd_miso));

    logic [7:0] buffer [512];
    always_ff @(posedge clk) if (read_byte) buffer[read_index] <= read_data;
    always_ff @(posedge clk) write_data <= 8'(write_index * 3 + 7);

    integer errors = 0;

    task automatic request(input logic w, input logic [31:0] s);
        @(posedge clk);
        while (!ready) @(posedge clk);
        valid <= 1; write <= w; sector <= s;
        @(posedge clk);
        valid <= 0;
        while (!done) @(posedge clk);
        if (error) begin errors++; $display("request %0d sector %0d failed", w, s); end
    endtask

    initial begin
        valid = 0; write = 0; sector = 0;
        repeat (4) @(posedge clk);
        rst = 0;
        fork
            begin
                repeat (2000000) @(posedge clk);
                $display("FAIL: timeout (failed=%0d)", failed);
                $finish;
            end
        join_none
        request(0, 1);
        // The test image holds byte (sector * 7 + offset) at each position.
        for (int i = 0; i < 512; i++)
            if (buffer[i] !== 8'(7 + i)) begin
                if (errors < 5) $display("read sector 1 byte %0d = %02x", i, buffer[i]);
                errors++;
            end
        request(1, 2);
        request(0, 2);
        for (int i = 0; i < 512; i++)
            if (buffer[i] !== 8'(i * 3 + 7)) begin
                if (errors < 10) $display("readback byte %0d = %02x", i, buffer[i]);
                errors++;
            end
        request(0, 64);   // past the end of the card: must fail
        if (!error) begin errors++; $display("out-of-range read succeeded"); end
        else errors = errors - 1;
        if (errors == 0) $display("PASS");
        else $display("FAIL: %0d errors", errors);
        $finish;
    end
endmodule

// Simulation model of an SDHC card in SPI mode, backed by a disk image.
// Supports CMD0, CMD8, CMD55/ACMD41, CMD58, CMD16, CMD17 and CMD24. The image
// is loaded from IMAGE at start; writes change the in-memory copy only.
// SPI mode 0: MOSI is sampled on the rising edge, MISO changes on the falling.
module sd_card_model #(
    parameter IMAGE = "card.img",
    parameter integer SECTORS = 4096
)(
    input  logic sd_clk_i,
    input  logic sd_cs_n_i,
    input  logic sd_mosi_i,
    output logic sd_miso_o
);
    logic [7:0] image [SECTORS * 512];
    integer file, loaded;
    initial begin
        for (int i = 0; i < SECTORS * 512; i++) image[i] = 8'h00;
        file = $fopen(IMAGE, "rb");
        if (file == 0) $fatal(1, "sd_card_model: cannot open %s", IMAGE);
        loaded = $fread(image, file);
        $fclose(file);
    end

    // Response bytes waiting to go out on MISO.
    logic [7:0] queue [$];

    // Receive side
    logic [7:0] in_shift = 8'hff;
    logic [2:0] in_bits = '0;
    logic [7:0] frame [6];
    integer     frame_bytes = 0;
    logic       idle = 1'b1, app = 1'b0;
    integer     op_cond_polls = 0;

    typedef enum { COMMANDS, WRITE_WAIT_TOKEN, WRITE_DATA } mode_t;
    mode_t      mode = COMMANDS;
    integer     write_address, write_count;

    task automatic respond_r1(input logic [7:0] r1);
        queue.push_back(8'hff);   // NCR
        queue.push_back(r1);
    endtask

    task automatic command(input logic [5:0] index, input logic [31:0] argument);
        logic [7:0] r1;
        r1 = idle ? 8'h01 : 8'h00;
        if ($test$plusargs("trace")) $display("card: CMD%0d %08x app=%0d", index, argument, app);
        if (app && index == 6'd41) begin
            app = 1'b0;
            op_cond_polls++;
            if (op_cond_polls >= 2) idle = 1'b0;
            respond_r1(idle ? 8'h01 : 8'h00);
            return;
        end
        app = 1'b0;
        unique case (index)
            6'd0: begin idle = 1'b1; respond_r1(8'h01); end
            6'd8: begin
                respond_r1(r1);
                queue.push_back(8'h00); queue.push_back(8'h00);
                queue.push_back(8'h01); queue.push_back(argument[7:0]);
            end
            6'd55: begin app = 1'b1; respond_r1(r1); end
            6'd58: begin
                respond_r1(r1);
                queue.push_back(8'hc0); queue.push_back(8'hff);
                queue.push_back(8'h80); queue.push_back(8'h00);
            end
            6'd16: respond_r1(r1);
            6'd17: begin
                if (argument >= SECTORS) respond_r1(8'h40);
                else begin
                    respond_r1(8'h00);
                    queue.push_back(8'hff);
                    queue.push_back(8'hfe);
                    for (int i = 0; i < 512; i++) queue.push_back(image[argument * 512 + i]);
                    queue.push_back(8'h00); queue.push_back(8'h00);
                end
            end
            6'd24: begin
                if (argument >= SECTORS) respond_r1(8'h40);
                else begin
                    respond_r1(8'h00);
                    write_address = argument * 512;
                    mode = WRITE_WAIT_TOKEN;
                end
            end
            default: respond_r1(r1 | 8'h04);   // illegal command
        endcase
    endtask

    always @(posedge sd_clk_i) begin
        if (!sd_cs_n_i) begin
            in_shift = {in_shift[6:0], sd_mosi_i};
            in_bits = in_bits + 1'b1;
            if (in_bits == 0) begin
                unique case (mode)
                    COMMANDS: begin
                        if (frame_bytes == 0 && in_shift[7:6] != 2'b01) begin
                        end else begin
                            frame[frame_bytes] = in_shift;
                            frame_bytes++;
                            if (frame_bytes == 6) begin
                                frame_bytes = 0;
                                command(frame[0][5:0], {frame[1], frame[2], frame[3], frame[4]});
                            end
                        end
                    end
                    WRITE_WAIT_TOKEN: if (in_shift == 8'hfe) begin
                        mode = WRITE_DATA;
                        write_count = 0;
                    end
                    WRITE_DATA: begin
                        if (write_count < 512) image[write_address + write_count] = in_shift;
                        write_count++;
                        if (write_count == 514) begin
                            mode = COMMANDS;
                            queue.push_back(8'he5);          // data accepted
                            queue.push_back(8'h00);          // busy
                            queue.push_back(8'h00);
                            queue.push_back(8'hff);
                        end
                    end
                endcase
            end
        end
    end

    // Transmit side. A byte's bit 7 is valid before its first rising edge;
    // falls 1-7 shift out the rest, and fall 8 loads the next byte.
    logic [7:0] out_shift = 8'hff;
    logic [2:0] out_falls = 3'd0;
    initial sd_miso_o = 1'b1;
    always @(negedge sd_clk_i) begin
        if (sd_cs_n_i) begin
            out_shift = 8'hff;
            out_falls = 3'd0;
        end else if (out_falls == 3'd7) begin
            out_shift = queue.size() != 0 ? queue.pop_front() : 8'hff;
            out_falls = 3'd0;
        end else begin
            out_shift = {out_shift[6:0], 1'b1};
            out_falls = out_falls + 1'b1;
        end
        sd_miso_o = out_shift[7];
    end
endmodule

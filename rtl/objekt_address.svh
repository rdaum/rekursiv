// Architectural address-generation pipeline register. PREPARE sees the old
// selected object and the new index ALU result. Its bounds/selection status is
// deferred until a prepared memory operation consumes this register. It is safe
// to prepare one past the last element while completing a loop's final access.
typedef struct packed {
    logic [39:0] reference;
    logic signed [39:0] index;
    logic [23:0] address;
    logic [3:0] status;
} prepared_access_t;

// Return {physical address, deferred status}. Keep the function's local values
// scalar: Yosys 0.33 cannot resolve members of a function-local packed struct.
function automatic [27:0] prepare_address(
    input logic signed [39:0] index,
    input logic [23:0] base, size,
    input logic [3:0] status
);
    logic [23:0] physical_address;
    logic [3:0] deferred_status;
    logic [64:0] address;
    begin
        physical_address=0; deferred_status=status;
        address={41'b0,base}+{25'b0,index}-65'd1;
        if(status==OK) begin
            if(index<=0 || $unsigned(index)>{16'b0,size} ||
                address>=65'd16777216 || address>=65'(MEMORY_WORDS)) deferred_status=BOUNDS;
            else physical_address=address[23:0];
        end
        prepare_address={physical_address,deferred_status};
    end
endfunction

// Shared packed pager payload. Validity is stored separately so publication can
// expose all metadata on one edge. Keep this order aligned with Entry/Service
// and the scalar debug/service ports; 171 payload bits plus one valid bit.
// Included inside each owning module for the Yosys frontend (no include guard).
typedef struct packed {
    logic cond;
    logic modified;
    logic is_new;
    logic [39:0] representation;
    logic [23:0] base;
    logic [39:0] class_reference;
    logic [23:0] size;
    logic [39:0] reference;
} pager_entry_t;

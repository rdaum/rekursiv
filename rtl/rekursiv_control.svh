// Shared processor types and project control encodings.
//
// This file is included *inside* each processor module. Module-local typedefs
// provide one definition of the wire format without requiring package imports
// in the Yosys 0.33 frontend. There is intentionally no include guard: each
// module needs its own declarations. No storage or executable logic is here.
//
// These are project encodings, not a reconstruction of the original 128-bit
// control word. Keep the packed struct in sync with Instruction::encode() in
// crates/rekursiv-asm/src/processor.rs. That encoder has fixed-vector tests.
typedef logic [39:0] object_word_t;
typedef logic [23:0] stack_address_t;
typedef logic [15:0] micro_address_t;

typedef enum logic [3:0] {
    SEQ_CONTINUE = 0, SEQ_HOLD = 1,
    SEQ_JUMP = 2, SEQ_CONDITIONAL_JUMP = 3,
    SEQ_RELATIVE = 4, SEQ_CONDITIONAL_RELATIVE = 5,
    SEQ_BUS = 6, SEQ_CONDITIONAL_BUS = 7,
    SEQ_RETURN = 8, SEQ_CONDITIONAL_RETURN = 9,
    SEQ_DISPATCH = 10, SEQ_CONDITIONAL_DISPATCH = 11,
    SEQ_CONDITIONAL_MARK = 12, SEQ_TWO_WAY = 13,
    SEQ_SAVED_RETURN = 14, SEQ_SERVICE = 15
} sequence_op_t;

typedef enum logic [3:0] {
    BUS_IMMEDIATE = 0, BUS_ESTK = 1, BUS_CSTK = 2, BUS_OBJECT = 3,
    BUS_REGISTER = 4, BUS_APC = 5, BUS_AP = 6, BUS_SP = 7,
    BUS_NAMARG = 8, BUS_UPCOR = 9, BUS_Q = 10, BUS_SYMBOL = 11
} bus_source_t;

typedef enum logic [3:0] {
    CC_ALWAYS = 0, CC_ZERO = 1, CC_SIGN = 2, CC_CARRY = 3,
    CC_OVERFLOW = 4, CC_CORRECTED_SIGN = 5, CC_SYMBOL = 6,
    CC_LAST = 7, CC_CONTROL_ZERO = 8, CC_OBJECT_OK = 9, CC_INTERRUPT = 10
} condition_t;

typedef enum logic [3:0] {
    ALU_PASS = 0, ALU_ADD = 1, ALU_SUB = 2, ALU_SUB_REVERSE = 3,
    ALU_AND = 4, ALU_OR = 5, ALU_XOR = 6, ALU_NOT = 7, ALU_ROTATE = 8,
    ALU_MULTIPLY_SIGNED = 9, ALU_MULTIPLY_UNSIGNED = 10,
    ALU_PRODUCT_HIGH = 11, ALU_PRODUCT_LOW = 12
} alu_op_t;

typedef enum logic [2:0] {
    SOURCE_REGISTER = 0, SOURCE_BUS = 1, SOURCE_ESTK = 2,
    SOURCE_Q = 3, SOURCE_BRANCH = 4
} alu_source_t;

typedef enum logic [1:0] {
    CARRY_ZERO = 0, CARRY_ONE = 1, CARRY_ZERO_FLAG = 2
} carry_source_t;

typedef enum logic [1:0] {
    SHIFT_NONE = 0, SHIFT_LEFT = 1, SHIFT_RIGHT = 2, SHIFT_ARITHMETIC_RIGHT = 3
} destination_shift_t;

typedef enum logic [1:0] {
    POINTER_HOLD = 0, POINTER_BUS = 1, POINTER_INCREMENT = 2, POINTER_DECREMENT = 3
} pointer_op_t;

typedef enum logic [1:0] {
    ADDRESS_HOLD = 0, ADDRESS_BUS = 1, ADDRESS_SP = 2, ADDRESS_ARGUMENT = 3
} address_op_t;

typedef enum logic [2:0] {
    ESTK_HOLD = 0, ESTK_READ = 1, ESTK_BUS = 2, ESTK_ALU = 3, ESTK_COMPACT = 4
} evaluation_op_t;

typedef enum logic [3:0] {
    CSTK_HOLD = 0, CSTK_READ = 1, CSTK_BUS = 2, CSTK_UPCOR = 3,
    CSTK_APC = 4, CSTK_AP = 5, CSTK_SP = 6, CSTK_INCREMENT = 7, CSTK_DECREMENT = 8
} control_stack_op_t;

typedef enum logic [1:0] {
    APC_HOLD = 0, APC_BUS = 1, APC_INCREMENT = 2, APC_STEP = 3
} abstract_counter_op_t;

typedef enum logic [3:0] {
    FAULT_NONE = 0, FAULT_ENCODING = 1, FAULT_CODE = 2,
    FAULT_STACK = 3, FAULT_OBJECT = 4, FAULT_FETCH = 5
} processor_fault_t;

// Flags describe unshifted F. Register/Q/stack destinations receive shifted Y.
// Packed order preserves the existing observation interface: Z is bit zero.
typedef struct packed {
    logic corrected_sign;
    logic overflow;
    logic carry;
    logic sign;
    logic zero;
} arithmetic_flags_t;

// Declarations run from most significant bit to least significant bit.
// Reserved fields must be zero; rejecting them avoids silent image corruption.
typedef struct packed {
    logic [38:0] reserved_high;        // 255:217
    logic allocation_dynamic;         // 216: size from NUMERIK register A
    logic [3:0] recovery;              // 215:212: privileged collector operation
    logic [5:0] compact_code;          // 211:206, project compact representation
    logic allocation_scan;            // 205
    logic [23:0] allocation_size;      // 204:181
    object_word_t expected_type;       // 180:141
    logic check_type;                  // 140
    logic [2:0] value_register;        // 139:137
    logic load_value_register;         // 136
    logic [3:0] object_read;           // 135:132
    logic [1:0] object_memory;         // 131:130
    logic [2:0] object_register;       // 129:127
    logic [3:0] object_index;          // 126:123
    logic [3:0] object_pager;          // 122:119
    logic object_enable;              // 118
    logic fetch_map;                   // 117: lookup the OLD opcode
    logic fetch_nam;                   // 116: load opcode and operand
    abstract_counter_op_t apc;        // 115:114
    logic reserved_113;
    logic load_ap;                    // 112
    control_stack_op_t cstk;           // 111:108
    pointer_op_t csp;                  // 107:106
    evaluation_op_t estk;              // 105:103
    pointer_op_t sp;                   // 102:101
    address_op_t esp;                  // 100:99
    logic reserved_98;
    logic write_flags;                // 97
    logic load_q;                     // 96
    logic write_register;             // 95
    logic reserved_94;
    destination_shift_t shift;        // 93:92
    carry_source_t carry;             // 91:90
    alu_source_t s_source;            // 89:87
    alu_source_t r_source;            // 86:84
    alu_op_t alu;                     // 83:80
    logic [3:0] rb;                    // 79:76
    logic [3:0] ra;                    // 75:72
    logic [15:0] branch;               // 71:56
    logic load_symbol;                // 55
    logic load_mark;                  // 54
    logic halt;                       // 53
    logic invert_condition;           // 52
    condition_t condition;            // 51:48
    sequence_op_t sequence_op;        // 47:44
    bus_source_t bus_source;          // 43:40
    object_word_t immediate;          // 39:0
} microinstruction_t;

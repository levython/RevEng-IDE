//! Dalvik Opcode Dictionary — lookup info, descriptions, and syntax for every Dalvik opcode.

/// A Dalvik opcode entry.
pub struct OpcodeInfo {
    pub mnemonic: &'static str,
    pub opcode: u16,
    pub format: &'static str,
    pub description: &'static str,
    pub category: OpcodeCategory,
}

#[derive(Clone, Copy, PartialEq)]
pub enum OpcodeCategory {
    Move,
    Return,
    Const,
    Monitor,
    CheckCast,
    Array,
    Throw,
    Goto,
    Compare,
    Conditional,
    Invoke,
    FieldAccess,
    Arithmetic,
    Conversion,
}

impl OpcodeCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Move => "Move",
            Self::Return => "Return",
            Self::Const => "Const",
            Self::Monitor => "Monitor",
            Self::CheckCast => "Type",
            Self::Array => "Array",
            Self::Throw => "Throw",
            Self::Goto => "Goto",
            Self::Compare => "Compare",
            Self::Conditional => "Branch",
            Self::Invoke => "Invoke",
            Self::FieldAccess => "Field",
            Self::Arithmetic => "Math",
            Self::Conversion => "Convert",
        }
    }
}

pub struct SmaliOpcodes;

impl SmaliOpcodes {
    /// Look up an opcode by mnemonic (e.g., "invoke-virtual", "const-string").
    pub fn lookup(mnemonic: &str) -> Option<&'static OpcodeInfo> {
        OPCODES.iter().find(|op| op.mnemonic == mnemonic)
    }

    /// Get all opcodes matching a prefix (for autocomplete).
    pub fn prefix_match(prefix: &str) -> Vec<&'static OpcodeInfo> {
        let lower = prefix.to_lowercase();
        Self::all()
            .iter()
            .filter(|op| op.mnemonic.starts_with(&lower))
            .collect()
    }

    /// Get description for a mnemonic (for editor hover).
    pub fn describe(mnemonic: &str) -> Option<String> {
        Self::lookup(mnemonic).map(|op| {
            format!(
                "{} (0x{:02X})\nFormat: {}\nCategory: {}\n\n{}",
                op.mnemonic,
                op.opcode,
                op.format,
                op.category.label(),
                op.description
            )
        })
    }

    /// All opcodes.
    pub fn all() -> &'static [OpcodeInfo] {
        OPCODES
    }
}

use OpcodeCategory::*;

static OPCODES: &[OpcodeInfo] = &[
    OpcodeInfo { mnemonic: "nop", opcode: 0x00, format: "10x", description: "No operation", category: Move },
    OpcodeInfo { mnemonic: "move", opcode: 0x01, format: "12x", description: "Move value between registers", category: Move },
    OpcodeInfo { mnemonic: "move/from16", opcode: 0x02, format: "22x", description: "Move from 16-bit register address", category: Move },
    OpcodeInfo { mnemonic: "move/16", opcode: 0x03, format: "32x", description: "Move between 16-bit register addresses", category: Move },
    OpcodeInfo { mnemonic: "move-wide", opcode: 0x04, format: "12x", description: "Move wide (64-bit) value between register pairs", category: Move },
    OpcodeInfo { mnemonic: "move-wide/from16", opcode: 0x05, format: "22x", description: "Move wide from 16-bit register address", category: Move },
    OpcodeInfo { mnemonic: "move-wide/16", opcode: 0x06, format: "32x", description: "Move wide between 16-bit register addresses", category: Move },
    OpcodeInfo { mnemonic: "move-object", opcode: 0x07, format: "12x", description: "Move object reference between registers", category: Move },
    OpcodeInfo { mnemonic: "move-object/from16", opcode: 0x08, format: "22x", description: "Move object from 16-bit register address", category: Move },
    OpcodeInfo { mnemonic: "move-object/16", opcode: 0x09, format: "32x", description: "Move object between 16-bit register addresses", category: Move },
    OpcodeInfo { mnemonic: "move-result", opcode: 0x0A, format: "11x", description: "Move single-word result of invoke into register", category: Move },
    OpcodeInfo { mnemonic: "move-result-wide", opcode: 0x0B, format: "11x", description: "Move double-word result into register pair", category: Move },
    OpcodeInfo { mnemonic: "move-result-object", opcode: 0x0C, format: "11x", description: "Move object result of invoke into register", category: Move },
    OpcodeInfo { mnemonic: "move-exception", opcode: 0x0D, format: "11x", description: "Move caught exception into register", category: Move },

    OpcodeInfo { mnemonic: "return-void", opcode: 0x0E, format: "10x", description: "Return from void method", category: Return },
    OpcodeInfo { mnemonic: "return", opcode: 0x0F, format: "11x", description: "Return 32-bit value", category: Return },
    OpcodeInfo { mnemonic: "return-wide", opcode: 0x10, format: "11x", description: "Return 64-bit value", category: Return },
    OpcodeInfo { mnemonic: "return-object", opcode: 0x11, format: "11x", description: "Return object reference", category: Return },

    OpcodeInfo { mnemonic: "const/4", opcode: 0x12, format: "11n", description: "Load signed 4-bit constant", category: Const },
    OpcodeInfo { mnemonic: "const/16", opcode: 0x13, format: "21s", description: "Load signed 16-bit constant", category: Const },
    OpcodeInfo { mnemonic: "const", opcode: 0x14, format: "31i", description: "Load full 32-bit constant", category: Const },
    OpcodeInfo { mnemonic: "const/high16", opcode: 0x15, format: "21h", description: "Load high 16 bits of 32-bit constant", category: Const },
    OpcodeInfo { mnemonic: "const-wide/16", opcode: 0x16, format: "21s", description: "Load signed 16-bit into wide pair", category: Const },
    OpcodeInfo { mnemonic: "const-wide/32", opcode: 0x17, format: "31i", description: "Load signed 32-bit into wide pair", category: Const },
    OpcodeInfo { mnemonic: "const-wide", opcode: 0x18, format: "51l", description: "Load full 64-bit constant into register pair", category: Const },
    OpcodeInfo { mnemonic: "const-wide/high16", opcode: 0x19, format: "21h", description: "Load high 16 bits into wide pair", category: Const },
    OpcodeInfo { mnemonic: "const-string", opcode: 0x1A, format: "21c", description: "Load string reference from string table", category: Const },
    OpcodeInfo { mnemonic: "const-string/jumbo", opcode: 0x1B, format: "31c", description: "Load string reference (jumbo index)", category: Const },
    OpcodeInfo { mnemonic: "const-class", opcode: 0x1C, format: "21c", description: "Load class reference from type table", category: Const },

    OpcodeInfo { mnemonic: "monitor-enter", opcode: 0x1D, format: "11x", description: "Acquire monitor lock on object", category: Monitor },
    OpcodeInfo { mnemonic: "monitor-exit", opcode: 0x1E, format: "11x", description: "Release monitor lock on object", category: Monitor },

    OpcodeInfo { mnemonic: "check-cast", opcode: 0x1F, format: "21c", description: "Cast object to given type, throw ClassCastException if fail", category: CheckCast },
    OpcodeInfo { mnemonic: "instance-of", opcode: 0x20, format: "22c", description: "Check if object is instance of type", category: CheckCast },

    OpcodeInfo { mnemonic: "array-length", opcode: 0x21, format: "12x", description: "Get length of array", category: Array },
    OpcodeInfo { mnemonic: "new-instance", opcode: 0x22, format: "21c", description: "Create new instance of a class", category: CheckCast },
    OpcodeInfo { mnemonic: "new-array", opcode: 0x23, format: "22c", description: "Create new array of given type and size", category: Array },
    OpcodeInfo { mnemonic: "filled-new-array", opcode: 0x24, format: "35c", description: "Create and fill new array with given values", category: Array },
    OpcodeInfo { mnemonic: "filled-new-array/range", opcode: 0x25, format: "3rc", description: "Create and fill array from register range", category: Array },
    OpcodeInfo { mnemonic: "fill-array-data", opcode: 0x26, format: "31t", description: "Fill array with inline data", category: Array },

    OpcodeInfo { mnemonic: "throw", opcode: 0x27, format: "11x", description: "Throw an exception", category: Throw },
    OpcodeInfo { mnemonic: "goto", opcode: 0x28, format: "10t", description: "Unconditional branch (8-bit offset)", category: Goto },
    OpcodeInfo { mnemonic: "goto/16", opcode: 0x29, format: "20t", description: "Unconditional branch (16-bit offset)", category: Goto },
    OpcodeInfo { mnemonic: "goto/32", opcode: 0x2A, format: "30t", description: "Unconditional branch (32-bit offset)", category: Goto },

    OpcodeInfo { mnemonic: "cmpl-float", opcode: 0x2D, format: "23x", description: "Compare floats (less bias)", category: Compare },
    OpcodeInfo { mnemonic: "cmpg-float", opcode: 0x2E, format: "23x", description: "Compare floats (greater bias)", category: Compare },
    OpcodeInfo { mnemonic: "cmpl-double", opcode: 0x2F, format: "23x", description: "Compare doubles (less bias)", category: Compare },
    OpcodeInfo { mnemonic: "cmpg-double", opcode: 0x30, format: "23x", description: "Compare doubles (greater bias)", category: Compare },
    OpcodeInfo { mnemonic: "cmp-long", opcode: 0x31, format: "23x", description: "Compare two long values", category: Compare },

    OpcodeInfo { mnemonic: "if-eq", opcode: 0x32, format: "22t", description: "Branch if equal", category: Conditional },
    OpcodeInfo { mnemonic: "if-ne", opcode: 0x33, format: "22t", description: "Branch if not equal", category: Conditional },
    OpcodeInfo { mnemonic: "if-lt", opcode: 0x34, format: "22t", description: "Branch if less than", category: Conditional },
    OpcodeInfo { mnemonic: "if-ge", opcode: 0x35, format: "22t", description: "Branch if greater or equal", category: Conditional },
    OpcodeInfo { mnemonic: "if-gt", opcode: 0x36, format: "22t", description: "Branch if greater than", category: Conditional },
    OpcodeInfo { mnemonic: "if-le", opcode: 0x37, format: "22t", description: "Branch if less or equal", category: Conditional },
    OpcodeInfo { mnemonic: "if-eqz", opcode: 0x38, format: "21t", description: "Branch if equal to zero", category: Conditional },
    OpcodeInfo { mnemonic: "if-nez", opcode: 0x39, format: "21t", description: "Branch if not equal to zero", category: Conditional },
    OpcodeInfo { mnemonic: "if-ltz", opcode: 0x3A, format: "21t", description: "Branch if less than zero", category: Conditional },
    OpcodeInfo { mnemonic: "if-gez", opcode: 0x3B, format: "21t", description: "Branch if greater or equal to zero", category: Conditional },
    OpcodeInfo { mnemonic: "if-gtz", opcode: 0x3C, format: "21t", description: "Branch if greater than zero", category: Conditional },
    OpcodeInfo { mnemonic: "if-lez", opcode: 0x3D, format: "21t", description: "Branch if less or equal to zero", category: Conditional },

    OpcodeInfo { mnemonic: "aget", opcode: 0x44, format: "23x", description: "Get value from int array", category: Array },
    OpcodeInfo { mnemonic: "aget-wide", opcode: 0x45, format: "23x", description: "Get wide value from array", category: Array },
    OpcodeInfo { mnemonic: "aget-object", opcode: 0x46, format: "23x", description: "Get object reference from array", category: Array },
    OpcodeInfo { mnemonic: "aget-boolean", opcode: 0x47, format: "23x", description: "Get boolean from array", category: Array },
    OpcodeInfo { mnemonic: "aget-byte", opcode: 0x48, format: "23x", description: "Get byte from array", category: Array },
    OpcodeInfo { mnemonic: "aget-char", opcode: 0x49, format: "23x", description: "Get char from array", category: Array },
    OpcodeInfo { mnemonic: "aget-short", opcode: 0x4A, format: "23x", description: "Get short from array", category: Array },
    OpcodeInfo { mnemonic: "aput", opcode: 0x4B, format: "23x", description: "Put value into int array", category: Array },
    OpcodeInfo { mnemonic: "aput-wide", opcode: 0x4C, format: "23x", description: "Put wide value into array", category: Array },
    OpcodeInfo { mnemonic: "aput-object", opcode: 0x4D, format: "23x", description: "Put object reference into array", category: Array },
    OpcodeInfo { mnemonic: "aput-boolean", opcode: 0x4E, format: "23x", description: "Put boolean into array", category: Array },
    OpcodeInfo { mnemonic: "aput-byte", opcode: 0x4F, format: "23x", description: "Put byte into array", category: Array },
    OpcodeInfo { mnemonic: "aput-char", opcode: 0x50, format: "23x", description: "Put char into array", category: Array },
    OpcodeInfo { mnemonic: "aput-short", opcode: 0x51, format: "23x", description: "Put short into array", category: Array },

    OpcodeInfo { mnemonic: "iget", opcode: 0x52, format: "22c", description: "Get instance field value (int)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-wide", opcode: 0x53, format: "22c", description: "Get instance field value (wide)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-object", opcode: 0x54, format: "22c", description: "Get instance field value (object)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-boolean", opcode: 0x55, format: "22c", description: "Get instance field value (boolean)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-byte", opcode: 0x56, format: "22c", description: "Get instance field value (byte)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-char", opcode: 0x57, format: "22c", description: "Get instance field value (char)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iget-short", opcode: 0x58, format: "22c", description: "Get instance field value (short)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iput", opcode: 0x59, format: "22c", description: "Put value into instance field (int)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iput-wide", opcode: 0x5A, format: "22c", description: "Put value into instance field (wide)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iput-object", opcode: 0x5B, format: "22c", description: "Put value into instance field (object)", category: FieldAccess },
    OpcodeInfo { mnemonic: "iput-boolean", opcode: 0x5C, format: "22c", description: "Put value into instance field (boolean)", category: FieldAccess },

    OpcodeInfo { mnemonic: "sget", opcode: 0x60, format: "21c", description: "Get static field value (int)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sget-wide", opcode: 0x61, format: "21c", description: "Get static field value (wide)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sget-object", opcode: 0x62, format: "21c", description: "Get static field value (object)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sget-boolean", opcode: 0x63, format: "21c", description: "Get static field value (boolean)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sput", opcode: 0x67, format: "21c", description: "Put static field value (int)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sput-wide", opcode: 0x68, format: "21c", description: "Put static field value (wide)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sput-object", opcode: 0x69, format: "21c", description: "Put static field value (object)", category: FieldAccess },
    OpcodeInfo { mnemonic: "sput-boolean", opcode: 0x6A, format: "21c", description: "Put static field value (boolean)", category: FieldAccess },

    OpcodeInfo { mnemonic: "invoke-virtual", opcode: 0x6E, format: "35c", description: "Invoke virtual method on object", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-super", opcode: 0x6F, format: "35c", description: "Invoke superclass method", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-direct", opcode: 0x70, format: "35c", description: "Invoke direct (private/init) method", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-static", opcode: 0x71, format: "35c", description: "Invoke static method", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-interface", opcode: 0x72, format: "35c", description: "Invoke interface method", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-virtual/range", opcode: 0x74, format: "3rc", description: "Invoke virtual with register range", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-super/range", opcode: 0x75, format: "3rc", description: "Invoke super with register range", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-direct/range", opcode: 0x76, format: "3rc", description: "Invoke direct with register range", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-static/range", opcode: 0x77, format: "3rc", description: "Invoke static with register range", category: Invoke },
    OpcodeInfo { mnemonic: "invoke-interface/range", opcode: 0x78, format: "3rc", description: "Invoke interface with register range", category: Invoke },

    OpcodeInfo { mnemonic: "neg-int", opcode: 0x7B, format: "12x", description: "Negate int", category: Arithmetic },
    OpcodeInfo { mnemonic: "not-int", opcode: 0x7C, format: "12x", description: "Bitwise NOT int", category: Arithmetic },
    OpcodeInfo { mnemonic: "add-int", opcode: 0x90, format: "23x", description: "Add two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "sub-int", opcode: 0x91, format: "23x", description: "Subtract two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "mul-int", opcode: 0x92, format: "23x", description: "Multiply two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "div-int", opcode: 0x93, format: "23x", description: "Divide two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "rem-int", opcode: 0x94, format: "23x", description: "Remainder of two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "and-int", opcode: 0x95, format: "23x", description: "Bitwise AND two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "or-int", opcode: 0x96, format: "23x", description: "Bitwise OR two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "xor-int", opcode: 0x97, format: "23x", description: "Bitwise XOR two ints", category: Arithmetic },
    OpcodeInfo { mnemonic: "shl-int", opcode: 0x98, format: "23x", description: "Shift left int", category: Arithmetic },
    OpcodeInfo { mnemonic: "shr-int", opcode: 0x99, format: "23x", description: "Arithmetic shift right int", category: Arithmetic },
    OpcodeInfo { mnemonic: "ushr-int", opcode: 0x9A, format: "23x", description: "Logical shift right int", category: Arithmetic },

    OpcodeInfo { mnemonic: "add-int/2addr", opcode: 0xB0, format: "12x", description: "Add int (2-address form, dest = dest + src)", category: Arithmetic },
    OpcodeInfo { mnemonic: "sub-int/2addr", opcode: 0xB1, format: "12x", description: "Sub int (2-address form)", category: Arithmetic },
    OpcodeInfo { mnemonic: "mul-int/2addr", opcode: 0xB2, format: "12x", description: "Mul int (2-address form)", category: Arithmetic },
    OpcodeInfo { mnemonic: "div-int/2addr", opcode: 0xB3, format: "12x", description: "Div int (2-address form)", category: Arithmetic },
    OpcodeInfo { mnemonic: "add-int/lit8", opcode: 0xD8, format: "22b", description: "Add int with 8-bit literal", category: Arithmetic },
    OpcodeInfo { mnemonic: "add-int/lit16", opcode: 0xD0, format: "22s", description: "Add int with 16-bit literal", category: Arithmetic },

    OpcodeInfo { mnemonic: "int-to-long", opcode: 0x81, format: "12x", description: "Convert int to long", category: Conversion },
    OpcodeInfo { mnemonic: "int-to-float", opcode: 0x82, format: "12x", description: "Convert int to float", category: Conversion },
    OpcodeInfo { mnemonic: "int-to-double", opcode: 0x83, format: "12x", description: "Convert int to double", category: Conversion },
    OpcodeInfo { mnemonic: "long-to-int", opcode: 0x84, format: "12x", description: "Convert long to int", category: Conversion },
    OpcodeInfo { mnemonic: "float-to-int", opcode: 0x87, format: "12x", description: "Convert float to int", category: Conversion },
    OpcodeInfo { mnemonic: "double-to-int", opcode: 0x8A, format: "12x", description: "Convert double to int", category: Conversion },
    OpcodeInfo { mnemonic: "int-to-byte", opcode: 0x8D, format: "12x", description: "Convert int to byte (sign-extend)", category: Conversion },
    OpcodeInfo { mnemonic: "int-to-char", opcode: 0x8E, format: "12x", description: "Convert int to char (zero-extend)", category: Conversion },
    OpcodeInfo { mnemonic: "int-to-short", opcode: 0x8F, format: "12x", description: "Convert int to short (sign-extend)", category: Conversion },
];

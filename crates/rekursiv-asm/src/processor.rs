//! Project microinstructions. Numeric values are independent of the historical control word.
use crate::{Command, Status, Word};

macro_rules! field {
    ($name:ident { $($v:ident = $n:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name { #[default] $($v = $n),+ }
        impl std::str::FromStr for $name {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(if s.eq_ignore_ascii_case(stringify!($v)) {return Ok(Self::$v);})+
                Err(())
            }
        }
    };
}
field!(Bus { Immediate=0, Estk=1, Cstk=2, Object=3, Register=4, Apc=5, Ap=6, Sp=7, Namarg=8, Upcor=9, Q=10, Symbol=11, SymbolHigh=12, Device=13, Root=14 });
field!(Seq { Continue=0, Hold=1, Jump=2, ConditionalJump=3, Relative=4, ConditionalRelative=5, Bus=6, ConditionalBus=7, Return=8, ConditionalReturn=9, Dispatch=10, ConditionalDispatch=11, ConditionalMark=12, TwoWay=13, SavedReturn=14, Service=15 });
field!(Condition { Always=0, Zero=1, Sign=2, Carry=3, Overflow=4, CorrectedSign=5, Symbol=6, Last=7, ControlZero=8, ObjectOk=9, Interrupt=10 });
field!(Alu { Pass=0, Add=1, Sub=2, SubReverse=3, And=4, Or=5, Xor=6, Not=7, Rotate=8, MultiplySigned=9, MultiplyUnsigned=10, ProductHigh=11, ProductLow=12, Float=13, FloatStatus=14 });
// Numeric operations are independent of language primitive numbers.
field!(FloatOp { Add=0, Subtract=1, Multiply=2, Divide=3, Sqrt=4, Compare=5, FromSigned=6, ToSigned=7, FromUnsigned=8, ToUnsigned=9 });
field!(Rounding { NearestEven=0, TowardZero=1, Down=2, Up=3, NearestAway=4 });
field!(Device { None=0, Read=1, Write=2 });
field!(Source { Register=0, Bus=1, Estk=2, Q=3, Branch=4 });
field!(Carry { Zero=0, One=1, ZeroFlag=2 });
field!(Shift { None=0, Left=1, Right=2, ArithmeticRight=3 });
field!(Address { Hold=0, Bus=1, Sp=2, Argument=3 });
field!(Pointer { Hold=0, Bus=1, Increment=2, Decrement=3 });
field!(Estk { Hold=0, Read=1, Bus=2, Alu=3, Compact=4, Wide=5 });
field!(Cstk { Hold=0, Read=1, Bus=2, Upcor=3, Apc=4, Ap=5, Sp=6, Increment=7, Decrement=8 });
field!(Apc { Hold=0, Bus=1, Increment=2, Step=3 });
field!(Fetch { Hold=0, Nam=1, Map=2, Both=3 });

// Privileged OBJEKT controls, usable only inside an allocation recovery handler.
field!(Recovery { None=0, Begin=1, Root=2, Slot=3, Info=4, Mark=5, ReadBody=6, Stage=7, WriteBody=8, Commit=9, Return=10 });

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Instruction {
    pub data: Word,
    pub bus: Bus,
    pub seq: Seq,
    pub condition: Condition,
    pub invert: bool,
    pub halt: bool,
    pub mark: bool,
    pub symbol: bool,
    pub branch: u16,
    pub ra: u8,
    pub rb: u8,
    pub alu: Alu,
    pub float: FloatOp,
    pub rounding: Rounding,
    pub device: Device,
    pub write_root: bool,
    pub r: Source,
    pub s: Source,
    pub carry: Carry,
    pub shift: Shift,
    pub write_register: bool,
    pub load_q: bool,
    pub flags: bool,
    pub esp: Address,
    pub sp: Pointer,
    pub estk: Estk,
    pub csp: Pointer,
    pub cstk: Cstk,
    pub load_ap: bool,
    pub apc: Apc,
    pub fetch: Fetch,
    /// Uses the processor D bus in place of Command::data.
    pub object: Option<Command>,
    pub compact_code: u8,
    pub recovery: Recovery,
    /// Allocation size comes from register A; the D bus still supplies the class.
    pub allocation_dynamic: bool,
}
impl Instruction {
    pub fn literal(data: Word) -> Self {
        Self {
            data,
            ..Self::default()
        }
    }
    pub fn halt() -> Self {
        Self {
            halt: true,
            ..Self::default()
        }
    }
    /// Eight little-endian 32-bit lanes, loaded through the halted programming port.
    pub fn encode(self) -> Result<[u32; 8], Status> {
        if self.ra >= 16 || self.rb >= 16 || self.compact_code > 3 {
            return Err(Status::BadCommand);
        }
        if (self.alu != Alu::Float
            && (self.float != FloatOp::Add || self.rounding != Rounding::NearestEven))
            || (self.alu == Alu::Float
                && (self.object.is_some()
                    || self.recovery != Recovery::None
                    || self.shift != Shift::None
                    || self.carry != Carry::Zero
                    || self.estk == Estk::Compact))
        {
            return Err(Status::BadCommand);
        }
        if self.device != Device::None
            && (self.object.is_some() || self.alu == Alu::Float || self.recovery != Recovery::None)
        {
            return Err(Status::BadCommand);
        }
        if (self.write_root || self.bus == Bus::Root) && self.recovery != Recovery::None {
            return Err(Status::BadCommand);
        }
        let mut words = [0u32; 8];
        let mut put = |offset: usize, width: usize, value: u64| {
            for bit in 0..width {
                if value >> bit & 1 != 0 {
                    words[(offset + bit) / 32] |= 1 << ((offset + bit) % 32);
                }
            }
        };
        put(0, 40, self.data.bits());
        put(40, 4, self.bus as u64);
        put(44, 4, self.seq as u64);
        put(48, 4, self.condition as u64);
        put(52, 1, self.invert as u64);
        put(53, 1, self.halt as u64);
        put(54, 1, self.mark as u64);
        put(55, 1, self.symbol as u64);
        put(56, 16, self.branch as u64);
        put(72, 4, self.ra as u64);
        put(76, 4, self.rb as u64);
        put(80, 4, self.alu as u64);
        put(84, 3, self.r as u64);
        put(87, 3, self.s as u64);
        put(90, 2, self.carry as u64);
        put(92, 2, self.shift as u64);
        put(95, 1, self.write_register as u64);
        put(96, 1, self.load_q as u64);
        put(97, 1, self.flags as u64);
        put(99, 2, self.esp as u64);
        put(101, 2, self.sp as u64);
        put(103, 3, self.estk as u64);
        put(106, 2, self.csp as u64);
        put(108, 4, self.cstk as u64);
        put(112, 1, self.load_ap as u64);
        put(114, 2, self.apc as u64);
        put(116, 2, self.fetch as u64);
        if let Some(command) = self.object {
            let p = command.encode()?;
            put(118, 1, 1);
            put(119, 4, p.pager as u64);
            put(123, 4, p.index as u64);
            put(127, 3, p.register as u64);
            put(130, 2, p.memory as u64);
            put(132, 4, p.read as u64);
            put(136, 1, p.load_vr as u64);
            put(137, 3, p.vr as u64);
            put(140, 1, p.check_type as u64);
            put(141, 40, p.expected_type);
            put(181, 24, p.alloc_size as u64);
            put(205, 1, p.alloc_scan as u64);
        }
        put(206, 6, self.compact_code as u64);
        put(212, 4, self.recovery as u64);
        put(216, 1, self.allocation_dynamic as u64);
        put(217, 4, self.float as u64);
        put(221, 3, self.rounding as u64);
        put(224, 2, self.device as u64);
        put(226, 1, self.write_root as u64);
        Ok(words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packing_has_explicit_positions() {
        let i = Instruction {
            data: Word::from_bits(0x123456789a).unwrap(),
            seq: Seq::Jump,
            branch: 0xbeef,
            ra: 3,
            rb: 9,
            ..Instruction::default()
        };
        assert_eq!(
            &i.encode().unwrap()[..3],
            &[0x3456789a, 0xef002012, 0x000093be]
        );
        assert!(Instruction { ra: 16, ..i }.encode().is_err());
    }
}

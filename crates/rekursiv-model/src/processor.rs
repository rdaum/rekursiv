//! Test reference model: predicts register and memory results after each instruction.
//! It does not run the machine. Tests compare these results against the Verilog,
//! which implements clock cycles and request/response handshakes.
use rekursiv_asm::{processor::*, Command, Word};

// Matches the simulation wrapper. Individual RTL modules remain parameterized.
pub const CODE_WORDS: usize = 1024;
pub const STACK_WORDS: usize = 32;
pub const NAM_WORDS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Processor {
    pub pc: u16,
    pub upcor: u16,
    pub mark: u16,
    pub ucar: u16,
    pub sp: u32,
    pub esp: u32,
    pub csp: u32,
    pub ap: u32,
    pub apc: u32,
    pub estkr: u64,
    pub cstkr: u32,
    pub symbol: u64,
    pub object: u64,
    pub q: u32,
    pub product: u64,
    pub flags: u8,
    pub lastcc: bool,
    pub opcode: usize,
    pub namarg: u32,
    pub rf: [u32; 16],
    pub estk: [u64; STACK_WORDS],
    pub cstk: [u32; STACK_WORDS],
    pub halted: bool,
    pub service: bool,
    pub service_code: u8,
}
impl Default for Processor {
    fn default() -> Self {
        Self {
            pc: 0,
            upcor: 0,
            mark: 0,
            ucar: 0,
            sp: 0,
            esp: 0,
            csp: 0,
            ap: 0,
            apc: 0,
            estkr: 0,
            cstkr: 0,
            symbol: 0,
            object: 0,
            q: 0,
            product: 0,
            flags: 0,
            lastcc: false,
            opcode: 0,
            namarg: 0,
            rf: [0; 16],
            estk: [0; STACK_WORDS],
            cstk: [0; STACK_WORDS],
            halted: false,
            service: false,
            service_code: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Image {
    pub code: Vec<Option<Instruction>>,
    pub nam: Vec<Option<u64>>,
    pub map: Vec<Option<u16>>,
    pub collector_entry: Option<u16>,
    pub roots: [Word; 32],
}
impl Default for Image {
    fn default() -> Self {
        Self {
            code: vec![None; CODE_WORDS],
            nam: vec![None; NAM_WORDS],
            map: vec![None; 1024],
            collector_entry: None,
            roots: [Word::ZERO; 32],
        }
    }
}
impl Image {
    pub fn from_assembly(
        assembly: &rekursiv_asm::text::Assembly,
    ) -> Result<Self, rekursiv_asm::Status> {
        use rekursiv_asm::Status;
        let mut image = Self::default();
        for (&address, &i) in &assembly.code {
            *image
                .code
                .get_mut(address as usize)
                .ok_or(Status::BoundsError)? = Some(i);
        }
        for (&address, &word) in &assembly.nam {
            *image
                .nam
                .get_mut(address as usize)
                .ok_or(Status::BoundsError)? = Some(word);
        }
        for (&opcode, &target) in &assembly.map {
            if target as usize >= CODE_WORDS {
                return Err(Status::BoundsError);
            }
            image.map[opcode as usize] = Some(target);
        }
        for (&slot, &word) in &assembly.roots {
            image.roots[slot as usize] = word;
        }
        for entry in [assembly.entry, assembly.collector].into_iter().flatten() {
            if !image.code.get(entry as usize).is_some_and(Option::is_some) {
                return Err(Status::BadCommand);
            }
        }
        image.collector_entry = assembly.collector;
        Ok(image)
    }

    /// Install the machine collector in free control-store space. Its ordinary
    /// microinstructions use the same encoder and processor as application code.
    pub fn with_ram_collector(
        mut self,
        entry: u16,
        pager_entries: usize,
    ) -> Result<Self, rekursiv_asm::Status> {
        let code = rekursiv_asm::collector::program(
            entry,
            pager_entries,
            20 + STACK_WORDS + 2 * CODE_WORDS + 32,
        )?;
        let end = entry as usize + code.len();
        if end > self.code.len() || self.code[entry as usize..end].iter().any(Option::is_some) {
            return Err(rekursiv_asm::Status::BadCommand);
        }
        for (slot, instruction) in self.code[entry as usize..end].iter_mut().zip(code) {
            *slot = Some(instruction);
        }
        self.collector_entry = Some(entry);
        Ok(self)
    }
    pub fn program(instructions: &[Instruction]) -> Self {
        assert!(instructions.len() <= CODE_WORDS);
        let mut image = Self::default();
        for (slot, instruction) in image.code.iter_mut().zip(instructions) {
            *slot = Some(*instruction);
        }
        image
    }
}
impl Processor {
    pub fn bus(&self, i: Instruction) -> u64 {
        match i.bus {
            Bus::Immediate => i.data.bits(),
            Bus::Estk => self.estkr,
            Bus::Cstk => self.cstkr as u64,
            Bus::Object => self.object,
            Bus::Register => self.rf[i.ra as usize] as u64,
            Bus::Apc => self.apc as u64,
            Bus::Ap => self.ap as u64,
            Bus::Sp => self.sp as u64,
            Bus::Namarg => self.namarg as u64,
            Bus::Upcor => self.upcor as u64,
            Bus::Q => self.q as u64,
            Bus::Symbol => self.symbol,
        }
    }
    pub fn prepare(&self, image: &Image, irq: bool) -> Result<(Self, Option<Command>), u8> {
        let i = image
            .code
            .get(self.pc as usize)
            .copied()
            .flatten()
            .ok_or(2u8)?;
        i.encode().map_err(|_| 1u8)?;
        // This method models mutator retirement. Recovery transitions are
        // specified by collect_ram; privileged controls cannot run here.
        if i.recovery != Recovery::None {
            return Err(1);
        }
        if i.seq == Seq::Hold {
            return Ok((self.clone(), None));
        }
        let d = self.bus(i);
        let source = |src: Source, reg: u8| match src {
            Source::Register => self.rf[reg as usize],
            Source::Bus => d as u32,
            Source::Estk => self.estkr as u32,
            Source::Q => self.q,
            Source::Branch => i.branch as i16 as i32 as u32,
        };
        let r = source(i.r, i.ra);
        let s = source(i.s, i.rb);
        let cin = match i.carry {
            Carry::Zero => 0,
            Carry::One => 1,
            Carry::ZeroFlag => (self.flags & 1) as u64,
        };
        let mut n = self.clone();
        let (f, carry, overflow) = match i.alu {
            Alu::Add | Alu::Sub | Alu::SubReverse => {
                let (a, b, subtract) = match i.alu {
                    Alu::Add => (r, s, false),
                    Alu::Sub => (r, s, true),
                    _ => (s, r, true),
                };
                let wide = a as u64 + if subtract { (!b) as u64 } else { b as u64 } + cin;
                let signed = if subtract {
                    (a as i32 as i64) - (b as i32 as i64) - 1 + cin as i64
                } else {
                    (a as i32 as i64) + (b as i32 as i64) + cin as i64
                };
                (
                    wide as u32,
                    wide > u32::MAX as u64,
                    !(i32::MIN as i64..=i32::MAX as i64).contains(&signed),
                )
            }
            op => {
                let value = match op {
                    Alu::Pass => r,
                    Alu::And => r & s,
                    Alu::Or => r | s,
                    Alu::Xor => r ^ s,
                    Alu::Not => !r,
                    Alu::Rotate => r.rotate_left(s & 31),
                    Alu::MultiplySigned => {
                        n.product = ((r as i32 as i64) * (s as i32 as i64)) as u64;
                        n.product as u32
                    }
                    Alu::MultiplyUnsigned => {
                        n.product = r as u64 * s as u64;
                        n.product as u32
                    }
                    Alu::ProductHigh => (self.product >> 32) as u32,
                    Alu::ProductLow => self.product as u32,
                    _ => unreachable!(),
                };
                (value, false, false)
            }
        };
        let y = match i.shift {
            Shift::None => f,
            Shift::Left => f << 1,
            Shift::Right => f >> 1,
            Shift::ArithmeticRight => ((f as i32) >> 1) as u32,
        };
        let cc = (match i.condition {
            Condition::Always => true,
            Condition::Zero => self.flags & 1 != 0,
            Condition::Sign => self.flags & 2 != 0,
            Condition::Carry => self.flags & 4 != 0,
            Condition::Overflow => self.flags & 8 != 0,
            Condition::CorrectedSign => self.flags & 16 != 0,
            Condition::Symbol => self.symbol == d,
            Condition::Last => self.lastcc,
            Condition::ControlZero => self.cstkr == 0,
            Condition::ObjectOk => true,
            Condition::Interrupt => irq,
        }) ^ i.invert;
        let sequential = self.pc as i64 + 1;
        let mut target = sequential;
        let taken = match i.seq {
            Seq::Jump | Seq::Relative | Seq::Bus | Seq::Return | Seq::Dispatch | Seq::TwoWay => {
                true
            }
            Seq::ConditionalJump
            | Seq::ConditionalRelative
            | Seq::ConditionalBus
            | Seq::ConditionalReturn
            | Seq::ConditionalDispatch
            | Seq::ConditionalMark
            | Seq::SavedReturn => cc,
            _ => false,
        };
        if taken {
            target = match i.seq {
                Seq::Jump | Seq::ConditionalJump => i.branch as i64,
                Seq::Relative | Seq::ConditionalRelative => {
                    self.pc as i64 + (((d << 24) as i64) >> 24)
                }
                Seq::Bus | Seq::ConditionalBus => d as i64,
                Seq::Return | Seq::ConditionalReturn => self.cstkr as i64,
                Seq::Dispatch | Seq::ConditionalDispatch => self.ucar as i64,
                Seq::ConditionalMark => self.mark as i64,
                Seq::TwoWay => {
                    if cc {
                        i.branch as i64
                    } else {
                        self.mark as i64
                    }
                }
                Seq::SavedReturn => self.upcor as i64,
                _ => unreachable!(),
            };
            if i.seq != Seq::SavedReturn {
                n.upcor = sequential as u16;
            }
        }
        if i.halt {
            target = self.pc as i64;
        }
        if !(0..CODE_WORDS as i64).contains(&target) {
            return Err(2);
        }
        n.pc = target as u16;
        let pointer = |old: u32, op: Pointer| -> Result<u32, u8> {
            let value = match op {
                Pointer::Hold => old as i64,
                Pointer::Bus => d as i64,
                Pointer::Increment => old as i64 + 1,
                Pointer::Decrement => old as i64 - 1,
            };
            if !(0..STACK_WORDS as i64).contains(&value) {
                Err(3)
            } else {
                Ok(value as u32)
            }
        };
        n.sp = pointer(self.sp, i.sp)?;
        n.csp = pointer(self.csp, i.csp)?;
        n.esp = match i.esp {
            Address::Hold => self.esp as u64,
            Address::Bus => d,
            Address::Sp => n.sp as u64,
            Address::Argument => self.ap as u64 + i.branch as u64,
        }
        .try_into()
        .map_err(|_| 3u8)?;
        if n.esp as usize >= STACK_WORDS {
            return Err(3);
        }
        if i.load_ap {
            if d >= STACK_WORDS as u64 {
                return Err(3);
            }
            n.ap = d as u32;
        }
        let cd = match i.cstk {
            Cstk::Bus => d,
            Cstk::Upcor => self.upcor as u64,
            Cstk::Apc => self.apc as u64,
            Cstk::Ap => self.ap as u64,
            Cstk::Sp => self.sp as u64,
            Cstk::Increment => self.cstkr as u64 + 1,
            Cstk::Decrement => self.cstkr.wrapping_sub(1) as u64,
            _ => 0,
        };
        if cd > 0xffffff {
            return Err(3);
        }
        let apc = match i.apc {
            Apc::Hold => self.apc as u64,
            Apc::Bus => d,
            Apc::Increment => self.apc as u64 + 1,
            Apc::Step => self.apc as u64 + d,
        };
        if apc > 0xffffff {
            return Err(5);
        }
        n.apc = apc as u32;
        if matches!(i.fetch, Fetch::Nam | Fetch::Both) {
            let word = image
                .nam
                .get(self.apc as usize)
                .copied()
                .flatten()
                .ok_or(5u8)?;
            n.opcode = (word >> 30) as usize;
            n.namarg = (word & 0x3fffffff) as u32;
        }
        if matches!(i.fetch, Fetch::Map | Fetch::Both) {
            n.ucar = image.map[self.opcode].ok_or(5u8)?;
        }
        if i.mark {
            n.mark = self.pc;
        }
        if i.symbol {
            n.symbol = d;
        }
        n.lastcc = cc;
        if i.write_register {
            n.rf[i.rb as usize] = y;
        }
        if i.load_q {
            n.q = y;
        }
        if i.flags {
            let sign = f >> 31 != 0;
            n.flags = (f == 0) as u8
                | ((sign as u8) << 1)
                | ((carry as u8) << 2)
                | ((overflow as u8) << 3)
                | (((sign ^ overflow) as u8) << 4);
        }
        match i.estk {
            Estk::Hold => (),
            Estk::Read => n.estkr = self.estk[self.esp as usize],
            op => {
                let data = match op {
                    Estk::Bus => d,
                    Estk::Alu => y as u64,
                    Estk::Compact => Word::compact(i.compact_code, y).map_err(|_| 1u8)?.bits(),
                    _ => unreachable!(),
                };
                n.estk[self.esp as usize] = data;
                n.estkr = data;
            }
        }
        match i.cstk {
            Cstk::Hold => (),
            Cstk::Read => n.cstkr = self.cstk[self.csp as usize],
            _ => {
                n.cstk[self.csp as usize] = cd as u32;
                n.cstkr = cd as u32;
            }
        }
        n.halted = i.halt;
        n.service = i.seq == Seq::Service && cc && !i.halt;
        if n.service {
            n.service_code = (i.branch & 15) as u8;
        }
        if i.allocation_dynamic && self.rf[i.ra as usize] >= (1 << 24) {
            return Err(1);
        }
        let object = i.object.map(|mut c| {
            if i.allocation_dynamic {
                c.alloc_size = self.rf[i.ra as usize];
            }
            c.data = Word::from_bits(d).unwrap();
            c
        });
        Ok((n, object))
    }
}

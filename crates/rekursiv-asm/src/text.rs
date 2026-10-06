//! Text microassembler. One line describes parallel controls in one horizontal
//! control word. Labels and constants are resolved before the shared encoder
//! validates the instruction. Diagnostics retain source line and column.
use crate::{processor::*, Command, Word};
use std::{collections::BTreeMap, fmt, str::FromStr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub column: usize,
    pub message: String,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug)]
struct Token {
    text: String,
    column: usize,
}
fn error(line: usize, token: Option<&Token>, message: impl Into<String>) -> Error {
    Error {
        line,
        column: token.map_or(1, |t| t.column),
        message: message.into(),
    }
}
fn lex(line: usize, source: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b';' || c == b'#' || (c == b'/' && bytes.get(i + 1) == Some(&b'/')) {
            break;
        }
        let start = i;
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        } else if b":,=!-+*()".contains(&c) {
            i += 1;
        } else {
            return Err(Error {
                line,
                column: i + 1,
                message: "unexpected character".into(),
            });
        }
        out.push(Token {
            text: source[start..i].into(),
            column: start + 1,
        });
    }
    Ok(out)
}
fn identifier(t: &str) -> bool {
    t.as_bytes()
        .first()
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        && t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
struct Expression<'a> {
    tokens: &'a [Token],
    at: usize,
    line: usize,
    symbols: &'a BTreeMap<String, i64>,
}
impl Expression<'_> {
    fn take(&mut self, s: &str) -> bool {
        if self.tokens.get(self.at).is_some_and(|t| t.text == s) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn atom(&mut self) -> Result<i64> {
        if self.take("-") {
            return self
                .atom()?
                .checked_neg()
                .ok_or_else(|| error(self.line, self.tokens.get(self.at), "integer overflow"));
        }
        if self.take("+") {
            return self.atom();
        }
        if self.take("(") {
            let v = self.sum()?;
            if !self.take(")") {
                return Err(error(self.line, self.tokens.get(self.at), "expected ')'"));
            }
            return Ok(v);
        }
        let t = self
            .tokens
            .get(self.at)
            .ok_or_else(|| error(self.line, self.tokens.last(), "expected expression"))?;
        self.at += 1;
        if let Some(v) = self.symbols.get(&t.text) {
            return Ok(*v);
        }
        let clean = t.text.replace('_', "");
        let number = if let Some(s) = clean.strip_prefix("0x") {
            i64::from_str_radix(s, 16)
        } else if let Some(s) = clean.strip_prefix("0b") {
            i64::from_str_radix(s, 2)
        } else {
            clean.parse()
        };
        number.map_err(|_| {
            error(
                self.line,
                Some(t),
                format!("unknown symbol or invalid integer '{}'", t.text),
            )
        })
    }
    fn product(&mut self) -> Result<i64> {
        let mut n = self.atom()?;
        while self.take("*") {
            n = n.checked_mul(self.atom()?).ok_or_else(|| {
                error(self.line, self.tokens.get(self.at - 1), "integer overflow")
            })?;
        }
        Ok(n)
    }
    fn sum(&mut self) -> Result<i64> {
        let mut n = self.product()?;
        loop {
            if self.take("+") {
                n = n.checked_add(self.product()?).ok_or_else(|| {
                    error(self.line, self.tokens.get(self.at - 1), "integer overflow")
                })?;
            } else if self.take("-") {
                n = n.checked_sub(self.product()?).ok_or_else(|| {
                    error(self.line, self.tokens.get(self.at - 1), "integer overflow")
                })?;
            } else {
                return Ok(n);
            }
        }
    }
}
fn expression(line: usize, tokens: &[Token], symbols: &BTreeMap<String, i64>) -> Result<i64> {
    if tokens.len() > 128 {
        return Err(error(line, tokens.first(), "expression exceeds 128 tokens"));
    }
    let mut p = Expression {
        tokens,
        at: 0,
        line,
        symbols,
    };
    let v = p.sum()?;
    if p.at != tokens.len() {
        return Err(error(line, tokens.get(p.at), "unexpected expression token"));
    }
    Ok(v)
}
fn bounded(
    line: usize,
    tokens: &[Token],
    symbols: &BTreeMap<String, i64>,
    maximum: i64,
) -> Result<u64> {
    let v = expression(line, tokens, symbols)?;
    if v < 0 || v > maximum {
        Err(error(
            line,
            tokens.first(),
            format!("value must be in 0..={maximum}"),
        ))
    } else {
        Ok(v as u64)
    }
}
fn word(line: usize, tokens: &[Token], symbols: &BTreeMap<String, i64>) -> Result<Word> {
    let n = expression(line, tokens, symbols)?;
    if !(-(1i64 << 39)..(1i64 << 40)).contains(&n) {
        return Err(error(line, tokens.first(), "value exceeds 40 bits"));
    }
    Ok(Word::from_bits(n as u64 & ((1 << 40) - 1)).unwrap())
}
fn enumeration<T: FromStr>(line: usize, tokens: &[Token]) -> Result<T> {
    if tokens.len() != 1 {
        return Err(error(line, tokens.first(), "expected a control name"));
    }
    tokens[0].text.parse().map_err(|_| {
        error(
            line,
            tokens.first(),
            format!("unknown control '{}'", tokens[0].text),
        )
    })
}
fn fields(line: usize, tokens: &[Token]) -> Result<Vec<&[Token]>> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, t) in tokens.iter().enumerate() {
        match t.text.as_str() {
            "(" => depth += 1,
            ")" => {
                if depth == 0 {
                    return Err(error(line, Some(t), "unmatched ')'"));
                }
                depth -= 1;
            }
            "," if depth == 0 => {
                if start == i {
                    return Err(error(line, Some(t), "empty field"));
                }
                result.push(&tokens[start..i]);
                start = i + 1;
            }
            _ => (),
        }
    }
    if depth != 0 {
        return Err(error(line, tokens.last(), "unclosed '('"));
    }
    if start == tokens.len() {
        return Err(error(line, tokens.last(), "empty field"));
    }
    result.push(&tokens[start..]);
    Ok(result)
}
fn instruction(
    line: usize,
    tokens: &[Token],
    symbols: &BTreeMap<String, i64>,
) -> Result<Instruction> {
    let mut i = Instruction::default();
    let mut object = Command::default();
    let mut has_object = false;
    let mut seen = BTreeMap::new();
    for field in fields(line, tokens)? {
        let key = field[0].text.to_ascii_lowercase();
        if seen.insert(key.clone(), ()).is_some() {
            return Err(error(
                line,
                field.first(),
                format!("duplicate field '{key}'"),
            ));
        }
        let value = if field.len() == 1 {
            &field[1..]
        } else {
            if field[1].text != "=" {
                return Err(error(line, field.get(1), "expected '='"));
            }
            &field[2..]
        };
        let flag = || {
            if field.len() == 1 {
                Ok(true)
            } else {
                Err(error(line, value.first(), "flag has no operand"))
            }
        };
        let number = |max| bounded(line, value, symbols, max);
        match key.as_str() {
            "nop" => {
                flag()?;
            }
            "halt" => i.halt = flag()?,
            "ldsym" => i.symbol = flag()?,
            "ldmark" => i.mark = flag()?,
            "ldrb" => i.write_register = flag()?,
            "ldq" => i.load_q = flag()?,
            "flags" => i.flags = flag()?,
            "ldap" => i.load_ap = flag()?,
            "d" => {
                if value.len() == 1 {
                    if let Ok(bus) = value[0].text.parse::<Bus>() {
                        if bus == Bus::Immediate {
                            return Err(error(
                                line,
                                value.first(),
                                "d=immediate requires a numeric value",
                            ));
                        }
                        i.bus = bus;
                        continue;
                    }
                }
                i.data = word(line, value, symbols)?;
            }
            "seq" => i.seq = enumeration(line, value)?,
            "cc" => {
                let v = if value.first().is_some_and(|t| t.text == "!") {
                    i.invert = true;
                    &value[1..]
                } else {
                    value
                };
                i.condition = enumeration(line, v)?;
            }
            "brch" => {
                let n = expression(line, value, symbols)?;
                if !(-32768..=65535).contains(&n) {
                    return Err(error(line, value.first(), "branch field exceeds 16 bits"));
                }
                i.branch = n as u16;
            }
            "ra" => i.ra = number(15)? as u8,
            "rb" => i.rb = number(15)? as u8,
            "alu" => i.alu = enumeration(line, value)?,
            "r" => i.r = enumeration(line, value)?,
            "s" => i.s = enumeration(line, value)?,
            "cin" => i.carry = enumeration(line, value)?,
            "shift" => i.shift = enumeration(line, value)?,
            "esp" => i.esp = enumeration(line, value)?,
            "sp" => i.sp = enumeration(line, value)?,
            "estk" => i.estk = enumeration(line, value)?,
            "csp" => i.csp = enumeration(line, value)?,
            "cstk" => i.cstk = enumeration(line, value)?,
            "apc" => i.apc = enumeration(line, value)?,
            "fetch" => i.fetch = enumeration(line, value)?,
            "compact" => i.compact_code = number(3)? as u8,
            "gc" => i.recovery = enumeration(line, value)?,
            "page" => {
                object.pager = enumeration(line, value)?;
                has_object = true;
            }
            "idx" => {
                object.index = enumeration(line, value)?;
                has_object = true;
            }
            "reg" => {
                object.register = enumeration(line, value)?;
                has_object = true;
            }
            "mem" => {
                object.memory = enumeration(line, value)?;
                has_object = true;
            }
            "read" => {
                object.read = enumeration(line, value)?;
                has_object = true;
            }
            "vr" => {
                object.vr = number(7)? as u8;
                has_object = true;
            }
            "ldvr" => {
                object.load_vr = flag()?;
                has_object = true;
            }
            "class" => {
                object.expected_type = Some(word(line, value, symbols)?);
                has_object = true;
            }
            "size" => {
                if value.len() == 1 && value[0].text.eq_ignore_ascii_case("ra") {
                    i.allocation_dynamic = true;
                } else {
                    object.alloc_size = number((1 << 24) - 1)? as u32;
                }
                has_object = true;
            }
            "scan" => {
                object.alloc_scan = number(1)? != 0;
                has_object = true;
            }
            _ => return Err(error(line, field.first(), format!("unknown field '{key}'"))),
        }
    }
    if has_object {
        if i.recovery != Recovery::None {
            return Err(error(
                line,
                tokens.first(),
                "object and collector commands cannot share a word",
            ));
        }
        // Command validates the interface shape. For a register D source, the
        // actual operand is checked by OBJEKT when the instruction executes.
        object.data = if i.bus == Bus::Immediate {
            i.data
        } else {
            Word::reference(1, true).unwrap()
        };
        i.object = Some(object);
    }
    i.encode()
        .map_err(|e| error(line, tokens.first(), format!("invalid control word: {e:?}")))?;
    Ok(i)
}

#[derive(Clone, Debug, Default)]
pub struct Assembly {
    pub code: BTreeMap<u16, Instruction>,
    pub nam: BTreeMap<u16, u64>,
    pub map: BTreeMap<u16, u16>,
    pub roots: BTreeMap<u8, Word>,
    pub entry: Option<u16>,
    pub collector: Option<u16>,
    pub symbols: BTreeMap<String, i64>,
}
/// Predefined constants supply hardware capacities; source cannot redefine them.
/// `.org` and `.equ` require already known constants. Instruction operands,
/// entry points, root words, and opcode-map targets can reference later labels.
pub fn assemble(source: &str, origin: u16, constants: &[(&str, i64)]) -> Result<Assembly> {
    let mut result = Assembly::default();
    for &(name, value) in constants {
        if !identifier(name) || result.symbols.insert(name.into(), value).is_some() {
            return Err(error(1, None, "invalid or duplicate predefined constant"));
        }
    }
    let mut pending = Vec::new();
    let mut address = origin as u32;
    for (index, source) in source.lines().enumerate() {
        let line = index + 1;
        let mut tokens = lex(line, source)?;
        if tokens.is_empty() {
            continue;
        }
        if tokens.get(1).is_some_and(|t| t.text == ":") {
            if !identifier(&tokens[0].text)
                || address > u16::MAX as u32
                || result
                    .symbols
                    .insert(tokens[0].text.clone(), address as i64)
                    .is_some()
            {
                return Err(error(line, tokens.first(), "invalid or duplicate label"));
            }
            tokens.drain(..2);
            if tokens.is_empty() {
                continue;
            }
        }
        match tokens[0].text.as_str() {
            ".org" => address = bounded(line, &tokens[1..], &result.symbols, 65535)? as u32,
            ".equ" => {
                if tokens.len() < 4 || !identifier(&tokens[1].text) || tokens[2].text != "=" {
                    return Err(error(
                        line,
                        tokens.first(),
                        "expected .equ NAME = expression",
                    ));
                }
                let value = expression(line, &tokens[3..], &result.symbols)?;
                if result
                    .symbols
                    .insert(tokens[1].text.clone(), value)
                    .is_some()
                {
                    return Err(error(line, tokens.get(1), "duplicate constant"));
                }
            }
            name if name.starts_with('.') => {
                pending.push((line, None, tokens));
            }
            _ => {
                if address > 65535 {
                    return Err(error(
                        line,
                        tokens.first(),
                        "control-store address overflow",
                    ));
                }
                pending.push((line, Some(address as u16), tokens));
                address += 1;
            }
        }
    }
    for (line, address, tokens) in pending {
        if let Some(address) = address {
            let i = instruction(line, &tokens, &result.symbols)?;
            if result.code.insert(address, i).is_some() {
                return Err(error(
                    line,
                    tokens.first(),
                    "overlapping control-store words",
                ));
            }
            continue;
        }
        let args = fields(line, &tokens[1..])?;
        let n = |index: usize, max: i64| {
            args.get(index)
                .ok_or_else(|| error(line, tokens.first(), "missing directive argument"))
                .and_then(|t| bounded(line, t, &result.symbols, max))
        };
        let expected = match tokens[0].text.as_str() {
            ".entry" => {
                if result.entry.replace(n(0, 65535)? as u16).is_some() {
                    return Err(error(line, tokens.first(), "duplicate .entry"));
                }
                1
            }
            ".collector" => {
                if result.collector.replace(n(0, 65535)? as u16).is_some() {
                    return Err(error(line, tokens.first(), "duplicate .collector"));
                }
                1
            }
            ".map" => {
                if result
                    .map
                    .insert(n(0, 1023)? as u16, n(1, 65535)? as u16)
                    .is_some()
                {
                    return Err(error(line, tokens.first(), "duplicate opcode mapping"));
                }
                2
            }
            ".nam" => {
                let a = n(0, 65535)? as u16;
                let opcode = n(1, 1023)?;
                let operand = n(2, (1 << 30) - 1)?;
                if result.nam.insert(a, (opcode << 30) | operand).is_some() {
                    return Err(error(line, tokens.first(), "duplicate NAM word"));
                }
                3
            }
            ".root" => {
                let a = n(0, 31)? as u8;
                let t = args
                    .get(1)
                    .ok_or_else(|| error(line, tokens.first(), "missing root value"))?;
                if result
                    .roots
                    .insert(a, word(line, t, &result.symbols)?)
                    .is_some()
                {
                    return Err(error(line, tokens.first(), "duplicate root slot"));
                }
                2
            }
            _ => return Err(error(line, tokens.first(), "unknown directive")),
        };
        if args.len() != expected {
            return Err(error(line, tokens.first(), "too many directive arguments"));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_constants_horizontal_fields_and_image_directives() {
        let source="\n.equ CLASS = 0xa000000064\n.entry start\n.org 4\nstart: d=CLASS, page=Allocate, size=2*3, scan=1\n d=Object, estk=Bus, ldsym\n seq=ConditionalJump, cc=!Zero, brch=done\n d=-1, idx=Load\ndone: halt\n.map 7, start\n.nam 0, 7, 0b101\n.root 31, CLASS\n";
        let p = assemble(source, 0, &[]).unwrap();
        assert_eq!(p.entry, Some(4));
        assert_eq!(p.code.len(), 5);
        let c = p.code[&4].object.unwrap();
        assert_eq!(c.alloc_size, 6);
        assert_eq!(c.data.bits(), 0xa000000064);
        assert_eq!(p.code[&6].branch, 8);
        assert!(p.code[&6].invert);
        assert_eq!(p.code[&7].data.bits(), (1 << 40) - 1);
        assert_eq!(p.map[&7], 4);
        assert_eq!(p.nam[&0], 7 << 30 | 5);
        assert_eq!(p.roots[&31].bits(), 0xa000000064);
    }
    #[test]
    fn lexer_reports_position_and_parser_rejects_ambiguous_or_truncated_input() {
        let e = assemble("; comment\n  d=@", 0, &[]).unwrap_err();
        assert_eq!((e.line, e.column), (2, 5));
        for source in [
            "d=missing",
            "x: nop\nx: halt",
            "d=0, d=1",
            "alu=magic",
            "ra=16",
            "size=16777216",
            "brch=65536",
            "d=0x10000000000",
            "d=-549755813889",
            "halt=",
            "d=1,",
            "gc=Begin, page=Fetch",
            ".org 4\nnop\n.org 4\nnop",
            ".root 32, 0",
            ".equ N = 9223372036854775807*2",
            ".entry x, 4\nx: halt",
            ".unknown 0",
            ".org 65535\nnop\nnop",
        ] {
            assert!(assemble(source, 0, &[]).is_err(), "accepted {source}");
        }
        assert!(assemble(".equ SIZE = 3", 0, &[("SIZE", 2)]).is_err());
    }
    #[test]
    fn new_control_fields_have_fixed_encoding() {
        let p = assemble("d=Symbol, gc=Root", 0, &[]).unwrap();
        let bits = p.code[&0].encode().unwrap();
        assert_eq!(bits[1], 11 << 8);
        assert_eq!(bits[6], 2 << 20);
        let p = assemble(
            "d=0xa000000001, page=Allocate, size=ra, ra=7, scan=0",
            0,
            &[],
        )
        .unwrap();
        let bits = p.code[&0].encode().unwrap();
        assert_eq!(bits[6], 1 << 24);
        assert_eq!(p.code[&0].ra, 7);
    }
    #[test]
    fn collector_source_assembles_at_distinct_origins() {
        let a = crate::collector::program(128, 16, 595).unwrap();
        let b = crate::collector::program(32, 16, 595).unwrap();
        assert_eq!(a.len(), 68);
        for (mut x, y) in a.into_iter().zip(b) {
            if matches!(x.seq, Seq::Jump | Seq::ConditionalJump) {
                x.branch -= 96;
            }
            assert_eq!(x.encode().unwrap(), y.encode().unwrap());
        }
    }
}

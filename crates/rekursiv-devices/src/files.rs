//! Byte-oriented external file storage. The CPU marshals paths and data through
//! registers; this endpoint never reads guest memory or constructs guest values.
//! Mounted bytes and writes stay in this device instance. There is no implicit
//! host filesystem write, and an opaque handle is valid only for this session.
use super::Request;
use std::collections::BTreeMap;

#[derive(Clone)]
struct OpenFile {
    name: Vec<u8>,
    position: usize,
    writable: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Effect {
    Register(u32, u32),
    ReadByte,
    PropertyByte,
    Command(u32),
}
/// Register-based storage with explicit host mounting. Paths are UTF-8/byte
/// sequences with `/` separators, independent of the host's native pathname.
pub struct Files {
    /// File content exposed to the machine; guest writes modify these bytes.
    pub contents: BTreeMap<Vec<u8>, Vec<u8>>,
    handles: BTreeMap<u32, OpenFile>,
    next_handle: u32,
    selected: u32,
    position: u32,
    flags: u32,
    count: u32,
    status: u32,
    path: Vec<u8>,
    path_len: usize,
    transfer: Vec<u8>,
    cursor: usize,
    property: usize,
    properties: [Vec<u8>; 3],
    property_cursor: usize,
}
impl Files {
    /// Construct an empty volume with a root-directory property and boot name.
    pub fn new(root_path: &[u8], boot_name: &[u8], separator: u8) -> Self {
        Self {
            contents: BTreeMap::new(),
            handles: BTreeMap::new(),
            next_handle: 0x10000,
            selected: 0,
            position: 0,
            flags: 0,
            count: 0,
            status: 0,
            path: Vec::new(),
            path_len: 0,
            transfer: Vec::new(),
            cursor: 0,
            property: 0,
            properties: [root_path.to_vec(), boot_name.to_vec(), vec![separator]],
            property_cursor: 0,
        }
    }
    fn name(&self) -> Vec<u8> {
        let separator = self.properties[2][0];
        let mut path = self.path.as_slice();
        if path.starts_with(&self.properties[0]) {
            path = &path[self.properties[0].len()..];
        }
        let mut normalized = vec![b'/'];
        normalized.extend(path.iter().map(|&b| if b == separator { b'/' } else { b }));
        while normalized.starts_with(b"//") {
            normalized.remove(0);
        }
        normalized
    }
    pub(super) fn preview(&self, r: Request) -> Option<(u32, Option<Effect>)> {
        Some(match (r.address - 0x700, r.write) {
            (0, true) if (1..=8).contains(&r.data) => (0, Some(Effect::Command(r.data))),
            (4, false) => (self.status, None),
            (8, true) if r.data <= 4096 => (0, Some(Effect::Register(8, r.data))),
            (12, true) if r.data <= 255 && self.path.len() < self.path_len => {
                (0, Some(Effect::Register(12, r.data)))
            }
            (16 | 20 | 24 | 32, true) => (0, Some(Effect::Register(r.address - 0x700, r.data))),
            (20, false) => (self.selected, None),
            (24, false) => (self.position, None),
            (32, false) => (self.count, None),
            (36, false) if self.cursor < self.transfer.len() => {
                (self.transfer[self.cursor] as u32, Some(Effect::ReadByte))
            }
            (36, true) if r.data <= 255 && self.transfer.len() < 1 << 20 => {
                (0, Some(Effect::Register(36, r.data)))
            }
            (40, false) => (
                self.handles
                    .get(&self.selected)
                    .and_then(|f| self.contents.get(&f.name))
                    .map_or(0, |b| b.len() as u32),
                None,
            ),
            (64, true) if r.data < 3 => (0, Some(Effect::Register(64, r.data))),
            (68, false) => (self.properties[self.property].len() as u32, None),
            (72, true) if r.data as usize <= self.properties[self.property].len() => {
                (0, Some(Effect::Register(72, r.data)))
            }
            (76, false) if self.property_cursor < self.properties[self.property].len() => (
                self.properties[self.property][self.property_cursor] as u32,
                Some(Effect::PropertyByte),
            ),
            _ => return None,
        })
    }
    pub(super) fn commit(&mut self, effect: Effect) {
        match effect {
            Effect::ReadByte => self.cursor += 1,
            Effect::PropertyByte => self.property_cursor += 1,
            Effect::Register(register, value) => match register {
                8 => {
                    self.path_len = value as usize;
                    self.path.clear();
                }
                12 => self.path.push(value as u8),
                16 => self.flags = value,
                20 => self.selected = value,
                24 => self.position = value,
                32 => {
                    self.count = value;
                    self.transfer.clear();
                    self.cursor = 0;
                }
                36 => self.transfer.push(value as u8),
                64 => {
                    self.property = value as usize;
                    self.property_cursor = 0;
                }
                72 => self.property_cursor = value as usize,
                _ => unreachable!(),
            },
            Effect::Command(command) => self.command(command),
        }
    }
    fn command(&mut self, command: u32) {
        self.status = 1;
        if command == 8 {
            // Commit an external boot-name property from the staged path.
            // This does not rename a file or write a machine snapshot.
            if self.path.len() == self.path_len {
                self.properties[1] = self.path.clone();
                self.property_cursor = 0;
                self.status = 0;
            }
            return;
        }
        if command == 1 {
            if self.path.len() != self.path_len || self.path.is_empty() || self.flags > 1 {
                return;
            }
            let name = self.name();
            if !self.contents.contains_key(&name) {
                if self.flags == 0 {
                    return;
                }
                self.contents.insert(name.clone(), Vec::new());
            }
            let Some(next) = self.next_handle.checked_add(1) else {
                return;
            };
            self.selected = self.next_handle;
            self.next_handle = next;
            self.handles.insert(
                self.selected,
                OpenFile {
                    name,
                    position: 0,
                    writable: self.flags != 0,
                },
            );
            self.position = 0;
            self.status = 0;
            return;
        }
        if command == 7 {
            if self.path.len() == self.path_len && self.contents.remove(&self.name()).is_some() {
                self.status = 0;
            }
            return;
        }
        if command == 2 {
            if self.handles.remove(&self.selected).is_some() {
                self.status = 0;
            }
            return;
        }
        let Some(file) = self.handles.get_mut(&self.selected) else {
            return;
        };
        let Some(bytes) = self.contents.get_mut(&file.name) else {
            return;
        };
        match command {
            3 => {
                if self.count > 1 << 20 {
                    return;
                }
                let start = file.position.min(bytes.len());
                let end = start.saturating_add(self.count as usize).min(bytes.len());
                self.transfer = bytes[start..end].to_vec();
                self.count = self.transfer.len() as u32;
                self.cursor = 0;
                file.position += self.count as usize;
            }
            4 => {
                if !file.writable || self.count as usize != self.transfer.len() {
                    return;
                }
                let Some(end) = file.position.checked_add(self.transfer.len()) else {
                    return;
                };
                // This model deliberately bounds its external volume capacity.
                if end > 64 << 20 {
                    return;
                }
                bytes.resize(bytes.len().max(end), 0);
                bytes[file.position..end].copy_from_slice(&self.transfer);
                file.position = end;
            }
            5 => file.position = self.position as usize,
            6 => (), // query an existing handle
            _ => unreachable!(),
        }
        self.position = file.position as u32;
        self.status = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write(f: &mut Files, offset: u32, value: u32) {
        let (_, effect) = f
            .preview(Request {
                address: 0x700 + offset,
                write: true,
                data: value,
            })
            .unwrap();
        f.commit(effect.unwrap());
    }
    fn read(f: &mut Files, offset: u32) -> u32 {
        let (value, effect) = f
            .preview(Request {
                address: 0x700 + offset,
                write: false,
                data: 0,
            })
            .unwrap();
        if let Some(effect) = effect {
            f.commit(effect);
        }
        value
    }
    #[test]
    fn handles_reads_short_reads_and_volatile_writes() {
        let mut f = Files::new(b"/", b"/boot.image", b'/');
        f.contents.insert(b"/test".to_vec(), b"abcd".to_vec());
        write(&mut f, 8, 4);
        for b in b"test" {
            write(&mut f, 12, *b as u32);
        }
        write(&mut f, 16, 1);
        write(&mut f, 0, 1);
        assert_eq!(read(&mut f, 4), 0);
        let handle = read(&mut f, 20);
        write(&mut f, 32, 8);
        write(&mut f, 0, 3);
        assert_eq!(read(&mut f, 32), 4);
        // An unconsumed read response must not advance the transfer cursor.
        let r = Request {
            address: 0x724,
            write: false,
            data: 0,
        };
        assert_eq!(f.preview(r).unwrap().0, b'a' as u32);
        assert_eq!(f.preview(r).unwrap().0, b'a' as u32);
        assert_eq!(
            (0..4).map(|_| read(&mut f, 36) as u8).collect::<Vec<_>>(),
            b"abcd"
        );
        assert_eq!(read(&mut f, 24), 4);
        write(&mut f, 24, 1);
        write(&mut f, 0, 5);
        write(&mut f, 32, 2);
        write(&mut f, 36, b'X' as u32);
        write(&mut f, 36, b'Y' as u32);
        write(&mut f, 0, 4);
        assert_eq!(read(&mut f, 4), 0);
        assert_eq!(f.contents[b"/test".as_slice()], b"aXYd");
        write(&mut f, 0, 2);
        write(&mut f, 20, handle);
        write(&mut f, 0, 6);
        assert_eq!(read(&mut f, 4), 1);
    }
}

//! Invented Squeak graphs with explicit expected behavior; no distribution needed.
use rekursiv_smalltalk::squeak::{
    image::{Body, Endian, Header, Image, Object},
    layout::MethodHeader,
};
use std::collections::BTreeMap;
pub fn int(n: i32) -> u32 {
    ((n as u32) << 1) | 1
}

pub struct Fixture {
    pub image: Image,
    pub special: [u32; 31],
    pub context: u32,
    pub receiver: u32,
    pub class: u32,
    next: u32,
}
impl Fixture {
    pub fn new() -> Self {
        let mut f = Self {
            image: Image {
                sha256: "synthetic".into(),
                header: Header {
                    endian: Endian::Little,
                    header_bytes: 64,
                    heap_bytes: 0,
                    old_base: 0x1000,
                    special_objects: 0,
                    last_hash: 999,
                    window_size: (640, 480),
                    fullscreen: 0,
                },
                objects: BTreeMap::new(),
                free_bytes: 0,
            },
            special: [0; 31],
            context: 0,
            receiver: 0,
            class: 0x1000,
            next: 0x1000,
        };
        f.class = f.pointers(0x1000, vec![0x1100, 0x1100, int(6)]); // metaclass
        let nil = f.pointers(f.class, vec![]);
        f.special.fill(nil);
        f.special[1] = f.pointers(f.class, vec![]);
        f.special[2] = f.pointers(f.class, vec![]);
        for (slot, format, fixed) in [
            (4, 6, 0),
            (5, 0, 0),
            (6, 8, 0),
            (7, 2, 0),
            (9, 6, 2),
            (10, 3, 6),
            (11, 3, 6),
            (12, 1, 2),
            (13, 8, 0),
            (15, 1, 2),
            (16, 12, 0),
            (18, 1, 3),
            (19, 1, 1),
            (26, 8, 0),
        ] {
            f.special[slot] = f.pointers(
                f.class,
                vec![nil, nil, int((format << 7) | ((fixed + 1) << 1))],
            );
        }
        f.receiver = f.pointers(f.class, vec![int(42), int(77)]);
        f.context = f.pointers(f.special[10], vec![nil; 38]);
        let process = f.pointers(f.class, vec![nil, f.context, int(3), nil]);
        let queues = f.pointers(f.special[7], vec![nil; 3]);
        let scheduler = f.pointers(f.class, vec![queues, process]);
        f.special[3] = f.pointers(f.class, vec![nil, scheduler]);
        let specials = f.pointers(f.special[7], f.special.to_vec());
        f.image.header.special_objects = specials;
        f
    }
    pub fn object(&mut self, class: u32, body: Body, format: u8) -> u32 {
        let oop = self.next;
        self.next += 0x100;
        self.image.objects.insert(
            oop,
            Object {
                oop,
                class,
                hash: 13,
                format,
                body,
            },
        );
        oop
    }
    pub fn pointers(&mut self, class: u32, fields: Vec<u32>) -> u32 {
        self.object(class, Body::Pointers(fields), 3)
    }
    pub fn method(
        &mut self,
        bytes: &[u8],
        literals: Vec<u32>,
        args: u32,
        temps: u32,
        primitive: u32,
    ) -> u32 {
        let header = MethodHeader(
            (args << 24) | (temps << 18) | (1 << 17) | ((literals.len() as u32) << 9) | primitive,
        );
        self.object(
            self.special[16],
            Body::Method {
                header,
                literals,
                bytes: bytes.to_vec(),
            },
            12,
        )
    }
    pub fn start(&mut self, bytes: &[u8], literals: Vec<u32>, temps: Vec<u32>) {
        let ip = 4 * (literals.len() + 1) + 1;
        let method = self.method(bytes, literals, 0, temps.len() as u32, 0);
        let mut fields = vec![self.special[0]; 38];
        fields[..6].copy_from_slice(&[
            self.special[0],
            int(ip as i32),
            int(temps.len() as i32),
            method,
            self.special[0],
            self.receiver,
        ]);
        fields[6..6 + temps.len()].copy_from_slice(&temps);
        self.image.objects.get_mut(&self.context).unwrap().body = Body::Pointers(fields);
    }
    pub fn selector(&mut self, name: &str) -> u32 {
        self.object(self.class, Body::Bytes(name.as_bytes().to_vec()), 8)
    }
    pub fn bind(&mut self, class: u32, selector: u32, method: u32) {
        let nil = self.special[0];
        let hash = self.image.objects[&selector].hash as usize;
        let mut selectors = vec![nil; 8];
        selectors[hash & 7] = selector;
        let mut methods = vec![nil; 8];
        methods[hash & 7] = method;
        let methods = self.pointers(self.special[7], methods);
        let mut fields = vec![int(1), methods];
        fields.extend(selectors);
        let dict = self.pointers(self.class, fields);
        if let Body::Pointers(fields) = &mut self.image.objects.get_mut(&class).unwrap().body {
            fields[1] = dict;
        }
    }
    pub fn primitive(&mut self, number: u32, args: u32, fallback: &[u8]) -> u32 {
        let selector = self.selector("testPrimitive");
        let method = self.method(fallback, vec![], args, args, number);
        self.bind(self.class, selector, method);
        selector
    }
}

impl Fixture {
    pub fn set_special(&mut self, index: usize, oop: u32) {
        self.special[index] = oop;
        if let Body::Pointers(fields) = &mut self
            .image
            .objects
            .get_mut(&self.image.header.special_objects)
            .unwrap()
            .body
        {
            fields[index] = oop;
        }
    }
    pub fn specials_for_blocks(&mut self) {
        let mut fields = vec![self.special[0]; 64];
        for (index, arity, name) in [(24, 1, "blockCopy:"), (25, 0, "value"), (26, 1, "value:")] {
            fields[index * 2] = self.selector(name);
            fields[index * 2 + 1] = int(arity);
        }
        let array = self.pointers(self.special[7], fields);
        self.set_special(23, array);
    }
}

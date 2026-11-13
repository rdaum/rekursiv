//! Host tests: the disk store against rekursiv-model's in-memory BackingStore,
//! and atomicity across interrupted commits.
extern crate std;
use super::*;
use crate::disk::{DiskError, Sector};
use rekursiv_asm::{Status, StoreOp, StoreRequest, Word};
use rekursiv_model::store::BackingStore;
use std::vec::Vec;

/// A file held in memory. After `writes_left` reaches zero, writes fail.
#[derive(Clone)]
struct MemDisk {
    sectors: Vec<Sector>,
    writes_left: Option<usize>,
}

impl MemDisk {
    fn new(sectors: usize) -> Self {
        Self {
            sectors: std::vec![[0; SECTOR]; sectors],
            writes_left: None,
        }
    }
}

impl Disk for MemDisk {
    fn read(&mut self, sector: u32, data: &mut Sector) -> Result<(), DiskError> {
        *data = *self.sectors.get(sector as usize).ok_or(DiskError)?;
        Ok(())
    }
    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), DiskError> {
        if let Some(left) = &mut self.writes_left {
            if *left == 0 {
                return Err(DiskError);
            }
            *left -= 1;
        }
        *self.sectors.get_mut(sector as usize).ok_or(DiskError)? = *data;
        Ok(())
    }
}

fn formatted(sectors: usize, capacity: u32) -> MemDisk {
    let mut disk = MemDisk::new(sectors);
    format(&mut disk, sectors as u32, capacity).unwrap();
    disk
}

/// Deterministic xorshift generator.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn reference(id: u64, scan: bool) -> u64 {
    1 << 39 | u64::from(scan) << 37 | id
}

fn to_model(r: Request) -> StoreRequest {
    StoreRequest {
        op: StoreOp::try_from(r.op).unwrap(),
        reference: Word::from_bits(r.reference).unwrap(),
        class: Word::from_bits(r.class).unwrap(),
        size: r.size,
        cond: r.cond,
        offset: r.offset,
        data: Word::from_bits(r.data).unwrap(),
    }
}

fn check(store: &mut Store<MemDisk>, model: &mut BackingStore, r: Request, step: usize) -> bool {
    let got = store.request(r);
    let want = model.request(to_model(r), false);
    assert_eq!(got.status, want.status as u8, "step {step}: {r:?}");
    if want.status == Status::Ok {
        assert_eq!(
            (got.reference, got.class, got.size, got.cond, got.data),
            (
                want.reference.bits(),
                want.class.bits(),
                want.size,
                want.cond,
                want.data.bits()
            ),
            "step {step}: {r:?}"
        );
    }
    want.status == Status::Ok
}

/// Every committed record, read back through FindRecord and ReadWord.
fn contents(store: &mut Store<MemDisk>, ids: u64) -> Vec<(u64, Reply, Vec<u64>)> {
    let mut out = Vec::new();
    for id in 0..ids {
        let m = store.request(Request {
            op: op::FIND_RECORD,
            reference: id,
            ..Request::default()
        });
        assert_eq!(m.status, status::OK);
        if m.reference == NIL {
            continue;
        }
        let body = (0..m.size)
            .map(|offset| {
                let w = store.request(Request {
                    op: op::READ_WORD,
                    reference: m.reference,
                    offset,
                    ..Request::default()
                });
                assert_eq!(w.status, status::OK);
                w.data
            })
            .collect();
        out.push((id, m, body));
    }
    out
}

fn model_contents(model: &BackingStore) -> Vec<(u64, u64, u64, u32, bool, Vec<u64>)> {
    model
        .records
        .iter()
        .map(|(&id, r)| {
            (
                id,
                r.reference.bits(),
                r.class.bits(),
                r.body.len() as u32,
                r.cond,
                r.body.iter().map(|w| w.bits()).collect(),
            )
        })
        .collect()
}

fn store_contents(
    store: &mut Store<MemDisk>,
    ids: u64,
) -> Vec<(u64, u64, u64, u32, bool, Vec<u64>)> {
    contents(store, ids)
        .into_iter()
        .map(|(id, m, body)| (id, m.reference, m.class, m.size, m.cond, body))
        .collect()
}

/// A random request, mostly well formed so that saves complete.
fn random_request(rng: &mut Random, open: &mut Option<(u64, u32, u32)>) -> Request {
    let class = reference(100 + rng.below(3), true);
    let pick = rng.below(100);
    if let Some((r, size, next)) = *open {
        if pick < 75 {
            if next < size {
                *open = Some((r, size, next + 1));
                return Request {
                    op: op::WRITE_WORD,
                    reference: r,
                    offset: next,
                    data: rng.next() & ((1 << 40) - 1),
                    ..Request::default()
                };
            }
            *open = None;
            return Request {
                op: op::COMMIT_SAVE,
                reference: r,
                ..Request::default()
            };
        }
    }
    let id = 1 + rng.below(40);
    let r = reference(id, rng.below(4) != 0);
    match pick {
        0..=24 => {
            let size = [0, 1, 2, 5, 61, 62, 63, 64, 130][rng.below(9) as usize];
            *open = Some((r, size, 0));
            Request {
                op: op::BEGIN_SAVE,
                reference: r,
                class,
                size,
                cond: rng.below(2) == 1,
                ..Request::default()
            }
        }
        25..=39 => Request {
            op: op::METADATA,
            reference: r,
            ..Request::default()
        },
        40..=54 => Request {
            op: op::READ_WORD,
            reference: r,
            offset: rng.below(70) as u32,
            ..Request::default()
        },
        55..=64 => Request {
            op: op::NEXT_RECORD,
            reference: rng.below(45),
            ..Request::default()
        },
        65..=72 => Request {
            op: op::FIND_RECORD,
            reference: rng.below(45),
            ..Request::default()
        },
        73..=78 => {
            *open = None;
            Request {
                op: op::BEGIN_BATCH,
                ..Request::default()
            }
        }
        79..=84 => Request {
            op: op::COMMIT_BATCH,
            ..Request::default()
        },
        85..=87 => {
            *open = None;
            Request {
                op: op::ABORT_BATCH,
                ..Request::default()
            }
        }
        88..=91 => Request {
            op: op::WRITE_WORD,
            reference: r,
            offset: rng.below(3) as u32,
            ..Request::default()
        },
        92..=95 => Request {
            op: op::COMMIT_SAVE,
            reference: r,
            ..Request::default()
        },
        _ => Request {
            op: op::METADATA,
            reference: rng.next() & ((1 << 40) - 1),
            ..Request::default()
        },
    }
}

#[test]
fn matches_the_reference_store_and_survives_reopening() {
    let mut committed = [0usize; 2];
    let mut largest = 0;
    for seed in 1..=20u64 {
        let mut rng = Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut store = Store::open(formatted(4096, 256)).unwrap();
        let mut model = BackingStore::default();
        let mut open = None;
        for step in 0..3000 {
            let r = random_request(&mut rng, &mut open);
            if check(&mut store, &mut model, r, step) {
                match r.op {
                    op::COMMIT_SAVE => committed[0] += 1,
                    op::COMMIT_BATCH => committed[1] += 1,
                    _ => {}
                }
            }
            largest = largest.max(model.records.len());
            if step % 500 == 499 {
                // Reopening discards staging, like a reset.
                let disk = store.into_disk();
                store = Store::open(disk).unwrap();
                model.discard_staging();
                open = None;
                assert_eq!(
                    store_contents(&mut store, 45),
                    model_contents(&model),
                    "seed {seed} step {step}"
                );
            }
        }
    }
    // The random requests must reach the interesting paths, not only errors.
    std::eprintln!(
        "saves committed {}, batches committed {}, most records {largest}",
        committed[0],
        committed[1]
    );
    assert!(committed[0] > 1000 && committed[1] > 100 && largest > 30);
}

#[test]
fn an_interrupted_commit_is_all_or_nothing() {
    let r1 = reference(5, true);
    let r2 = reference(9, false);
    let class = reference(100, true);
    let base = {
        let mut store = Store::open(formatted(1024, 64)).unwrap();
        for (r, v) in [(r1, 11), (r2, 22)] {
            store.request(Request {
                op: op::BEGIN_SAVE,
                reference: r,
                class,
                size: 1,
                ..Request::default()
            });
            store.request(Request {
                op: op::WRITE_WORD,
                reference: r,
                data: v,
                ..Request::default()
            });
            assert_eq!(
                store
                    .request(Request {
                        op: op::COMMIT_SAVE,
                        reference: r,
                        ..Request::default()
                    })
                    .status,
                0
            );
        }
        store.into_disk()
    };
    let before = {
        let mut s = Store::open(base.clone()).unwrap();
        store_contents(&mut s, 16)
    };
    // A batch replacing both records, interrupted after every possible write.
    let mut outcomes = [0, 0];
    for budget in 0..40 {
        let mut disk = base.clone();
        disk.writes_left = Some(budget);
        let mut store = Store::open(disk).unwrap();
        store.request(Request {
            op: op::BEGIN_BATCH,
            ..Request::default()
        });
        for (r, v) in [(r1, 33), (r2, 44)] {
            store.request(Request {
                op: op::BEGIN_SAVE,
                reference: r,
                class,
                size: 70,
                ..Request::default()
            });
            for offset in 0..70 {
                store.request(Request {
                    op: op::WRITE_WORD,
                    reference: r,
                    offset,
                    data: v + u64::from(offset),
                    ..Request::default()
                });
            }
            store.request(Request {
                op: op::COMMIT_SAVE,
                reference: r,
                ..Request::default()
            });
        }
        store.request(Request {
            op: op::COMMIT_BATCH,
            ..Request::default()
        });
        let mut disk = store.into_disk();
        disk.writes_left = None;
        let mut reopened = Store::open(disk).unwrap();
        let now = store_contents(&mut reopened, 16);
        if now == before {
            outcomes[0] += 1;
        } else {
            assert_eq!(now.len(), 2);
            for (i, (_, _, _, size, _, body)) in now.iter().enumerate() {
                assert_eq!(*size, 70);
                let v = [33, 44][i];
                assert_eq!(*body, (0..70).map(|o| v + o).collect::<Vec<u64>>());
            }
            outcomes[1] += 1;
        }
    }
    assert!(outcomes[0] > 0 && outcomes[1] > 0, "{outcomes:?}");
}

#[test]
fn records_spanning_sectors_round_trip() {
    let mut store = Store::open(formatted(2048, 64)).unwrap();
    let r = reference(3, true);
    let class = reference(100, true);
    assert_eq!(
        store
            .request(Request {
                op: op::BEGIN_SAVE,
                reference: r,
                class,
                size: 300,
                cond: true,
                ..Request::default()
            })
            .status,
        0
    );
    for offset in 0..300 {
        let reply = store.request(Request {
            op: op::WRITE_WORD,
            reference: r,
            offset,
            data: 0xab_0000_0000 | u64::from(offset),
            ..Request::default()
        });
        assert_eq!(reply.status, 0);
    }
    assert_eq!(
        store
            .request(Request {
                op: op::COMMIT_SAVE,
                reference: r,
                ..Request::default()
            })
            .status,
        0
    );
    let mut store = Store::open(store.into_disk()).unwrap();
    let m = store.request(Request {
        op: op::METADATA,
        reference: r,
        ..Request::default()
    });
    assert_eq!(
        (m.status, m.reference, m.class, m.size, m.cond),
        (0, r, class, 300, true)
    );
    for offset in [0, 58, 59, 60, 123, 299] {
        let w = store.request(Request {
            op: op::READ_WORD,
            reference: r,
            offset,
            ..Request::default()
        });
        assert_eq!(w.data, 0xab_0000_0000 | u64::from(offset));
    }
}

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
}

#[test]
fn window_names_the_words_in_the_sector_last_read() {
    let mut store = Store::open(formatted(512, 16)).unwrap();
    let reference = 2 << 38 | 7;
    let class = 2 << 38 | 100;
    let save = |store: &mut Store<_>, request: Request| {
        assert_eq!(store.request(request).status, status::OK);
    };
    save(
        &mut store,
        Request {
            op: op::BEGIN_SAVE,
            reference,
            class,
            size: 70,
            ..Request::default()
        },
    );
    for offset in 0..70 {
        save(
            &mut store,
            Request {
                op: op::WRITE_WORD,
                reference,
                offset,
                data: u64::from(offset) + 1000,
                ..Request::default()
            },
        );
    }
    save(
        &mut store,
        Request {
            op: op::COMMIT_SAVE,
            reference,
            ..Request::default()
        },
    );
    // The header's sector holds words 0..59, after the 40-byte header.
    save(
        &mut store,
        Request {
            op: op::METADATA,
            reference,
            ..Request::default()
        },
    );
    assert_eq!(
        store.window(),
        Some(Window {
            reference,
            offset: 0,
            index: 10,
            count: 59
        })
    );
    // Word 59 begins the second sector, which holds the remaining 11.
    let reply = store.request(Request {
        op: op::READ_WORD,
        reference,
        offset: 59,
        ..Request::default()
    });
    assert_eq!((reply.status, reply.data), (status::OK, 1059));
    assert_eq!(
        store.window(),
        Some(Window {
            reference,
            offset: 59,
            index: 0,
            count: 11
        })
    );
}

#[test]
fn identities_far_apart_get_index_blocks_on_demand() {
    // The board failed saving identity 266,837 against a flat index sized for
    // 262,144. Index blocks are now allocated only where identities are saved.
    let mut store = Store::open(formatted(9000, 1 << 26)).unwrap();
    let class = 2 << 38 | 100;
    let ids = [266_837u64, 5_000_000, 70];
    for (n, id) in ids.iter().enumerate() {
        let reference = 2 << 38 | id;
        let save = |store: &mut Store<_>, request: Request| {
            assert_eq!(store.request(request).status, status::OK);
        };
        save(
            &mut store,
            Request {
                op: op::BEGIN_SAVE,
                reference,
                class,
                size: 2,
                ..Request::default()
            },
        );
        for offset in 0..2 {
            save(
                &mut store,
                Request {
                    op: op::WRITE_WORD,
                    reference,
                    offset,
                    data: n as u64 * 10 + u64::from(offset),
                    ..Request::default()
                },
            );
        }
        save(
            &mut store,
            Request {
                op: op::COMMIT_SAVE,
                reference,
                ..Request::default()
            },
        );
    }
    let top = store.superblock().index_top;
    assert_eq!(top, 9000 - 3, "one index block per identity range");
    let mut store = Store::open(store.into_disk()).unwrap();
    for (n, id) in ids.iter().enumerate() {
        let reply = store.request(Request {
            op: op::READ_WORD,
            reference: 2 << 38 | id,
            offset: 1,
            ..Request::default()
        });
        assert_eq!((reply.status, reply.data), (status::OK, n as u64 * 10 + 1));
    }
    let mut next = 0;
    let mut order = std::vec::Vec::new();
    loop {
        let reply = store.request(Request {
            op: op::NEXT_RECORD,
            reference: next,
            ..Request::default()
        });
        assert_eq!(reply.status, status::OK);
        if reply.reference == NIL {
            break;
        }
        next = reply.reference & ID_MASK;
        order.push(next);
    }
    assert_eq!(order, [70, 266_837, 5_000_000]);
}

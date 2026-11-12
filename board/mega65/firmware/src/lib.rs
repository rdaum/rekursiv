//! Disk processor library: sector devices, FAT32 lookup and the object store.
//! Builds for the board's RISC-V core and for the host, where it is tested.
#![no_std]

pub mod disk;
pub mod fat;
pub mod store;

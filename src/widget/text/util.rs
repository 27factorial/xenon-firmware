use core::hash::{BuildHasherDefault, Hasher};

pub(crate) type CharHasher = BuildHasherDefault<IdentityHasherU32>;

const UNIMPLEMENTED: &str = "32 bit identity hasher only supports hashing 32 bit values";

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub(crate) struct IdentityHasherU32(Option<u32>);

impl Hasher for IdentityHasherU32 {
    fn finish(&self) -> u64 {
        self.0.expect("at least one value must be hashed") as u64
    }

    fn write(&mut self, _: &[u8]) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_u8(&mut self, _: u8) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_u16(&mut self, _: u16) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_u32(&mut self, i: u32) {
        self.0 = Some(i);
    }

    fn write_u64(&mut self, _: u64) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_u128(&mut self, _: u128) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_usize(&mut self, _: usize) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_i8(&mut self, _: i8) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_i16(&mut self, _: i16) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32)
    }

    fn write_i64(&mut self, _: i64) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_i128(&mut self, _: i128) {
        unimplemented!("{}", UNIMPLEMENTED);
    }

    fn write_isize(&mut self, _: isize) {
        unimplemented!("{}", UNIMPLEMENTED);
    }
}

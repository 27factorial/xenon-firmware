use crate::macros::syscalls;
use embassy_time::Instant;

syscalls! {
    pub extern "wasm" fn get_time(
        caller
    ) -> Result<u64, wasmi::Error> {
        Ok(Instant::now().as_micros())
    }
}

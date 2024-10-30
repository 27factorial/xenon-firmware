use crate::app::types::Env;
use crate::macros::syscall;
use embassy_time::Instant;
use wasmi::Caller;

#[syscall]
pub extern "wasm" fn get_time(_: Caller<'_, Env>) -> Result<u64, wasmi::Error> {
    Ok(Instant::now().as_micros())
}

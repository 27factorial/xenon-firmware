use embassy_executor::SendSpawner;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use esp_hal::rng::Trng;
use types::Executor;

pub mod convert;
pub mod syscall;
pub mod types;

static WASM_MODULE: &[u8] = include_bytes!("../../../assets/test-app.wasm");

pub(crate) fn start(rng: Trng<'static>, spawner: SendSpawner) -> Result<(), wasmi::Error> {
    // TODO: Load wasm module from "filesystem" and handle errors more gracefully.
    let mut executor = Executor::new(rng, spawner, WASM_MODULE)?;

    executor.run()
}

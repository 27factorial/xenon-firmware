use embassy_executor::SendSpawner;
use esp_hal::rng::Trng;
use types::Executor;

pub mod cpu;
pub mod types;
pub mod syscall;
pub mod convert;
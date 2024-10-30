#![no_std]
#![no_main]
#![feature(strict_provenance)]
#![feature(exposed_provenance)]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![allow(clippy::empty_loop)]
#![warn(fuzzy_provenance_casts)]
#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(not(target_pointer_width = "32"))]
compile_error!("Xenon may only be used on 32-bit architectures.");

extern crate alloc;

pub mod allocator;
pub mod app;
pub mod driver;
pub mod float;
pub mod fs;
pub mod logger;
pub(crate) mod macros;
pub mod widget;

use allocator::ALLOCATOR;
use app::cpu::AppCpu;
use esp_hal::config::WatchdogConfig;
use core::array;
use core::panic::PanicInfo;
use core::ptr::with_exposed_provenance_mut;
use driver::lcd;
use embassy_executor::Spawner;
use embassy_time::{Instant, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_backtrace as _;
use esp_hal::clock::Clocks;
use esp_hal::dma::Dma;
use esp_hal::gpio::Io;
use esp_hal::peripherals::{Peripherals, RADIO_CLK, TIMG0};
use esp_hal::prelude::*;
use esp_hal::psram;
use esp_hal::rng::{Rng, Trng};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::AnyTimer;
use esp_println::println;
use esp_storage::FlashStorage;
use esp_wifi::{EspWifiInitFor, EspWifiInitialization};
// use fs::FS_START;
// use fs::{Filesystem, FILESYSTEM};
use macros::make_static;

pub const DRIVER_SWI: u8 = 2;
pub const VERSION: &str = match option_env!("CARGO_PKG_VERSION") {
    Some(ver) => ver,
    None => "0.0.0",
};

pub static EXAMPLE_TEXT: &str = "The quick brown fox jumps over the lazy dog.";

#[inline(always)]
fn log_init(task: &str) {
    log::debug!("{task} initialized");
}

fn init_wireless(
    timer: impl Into<AnyTimer>,
    rng: Rng,
    radio_clocks: RADIO_CLK,
) -> EspWifiInitialization {
    use EspWifiInitFor::*;
    let timer = timer.into();

    let ret = esp_wifi::init(Ble, timer, rng, radio_clocks)
        .expect("BLE to be properly initialized");

    log_init("BLE");

    ret
}

fn init_embassy(timer0: impl Into<AnyTimer>, timer1: impl Into<AnyTimer>) {
    esp_hal_embassy::init([timer0.into(), timer1.into()]);

    log_init("embassy");
}

fn useit<T>(x: T) -> T {
    core::hint::black_box(x)
}

#[main]
async fn main(spawner: Spawner) {
    let mut hal_config = esp_hal::Config::default();
    hal_config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(hal_config);

    logger::init_logger_from_env();
    log_init("logging");

    let (psram_start, psram_size) =
        psram::init_psram(peripherals.PSRAM, psram::PsramConfig::default());
    unsafe {
        ALLOCATOR.init(
            psram_start,
            psram_size,
        )
    };
    log_init("heap");

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let dma = Dma::new(peripherals.DMA);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let trng = Trng::new(peripherals.RNG, peripherals.ADC1);
    let rng = trng.rng;

    init_embassy(timg0.timer0, timg0.timer1);

    // let fs = Filesystem::new(FlashStorage::new(), rng).await.unwrap();
    // FILESYSTEM.init(fs);
    // log_init("filesystem");

    spawner.must_spawn(lcd::start(
        peripherals.SPI2,
        io.pins.gpio7,
        io.pins.gpio9,
        io.pins.gpio44,
        dma,
    ));

    // init_wireless(timg1.timer1, rng, peripherals.RADIO_CLK, clocks);

    // let mut app_cpu = AppCpu::new(peripherals.CPU_CTRL);
    // app_cpu.start(trng, spawner.make_send());

    loop {
        Timer::after_secs(1).await;
    }
}

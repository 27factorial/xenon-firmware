#![no_std]
#![no_main]
#![feature(strict_provenance)]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
#![feature(async_closure)]

#![forbid(unsafe_op_in_unsafe_fn)]

#[cfg(not(target_pointer_width = "32"))]
compile_error!("Xenon may only be used on 32-bit architectures.");

extern crate alloc;

use allocator::ALLOCATOR;
use driver::{lcd, shell};
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::{ClockControl, Clocks};
use esp_hal::dma::Dma;
use esp_hal::gpio::Io;
use esp_hal::peripherals::{Peripherals, RADIO_CLK, TIMG0};
use esp_hal::prelude::*;
use esp_hal::psram;
use esp_hal::rng::{Rng, Trng};
use esp_hal::system::SystemControl;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::timer::ErasedTimer;
use esp_storage::FlashStorage;
use esp_wifi::{EspWifiInitFor, EspWifiInitialization};
use fs::{Filesystem, FILESYSTEM};
use macros::make_static;

pub mod allocator;
pub mod app;
pub mod driver;
pub mod float;
pub mod fs;
pub(crate) mod macros;
pub mod widget;

pub(crate) const VERSION: &str = match option_env!("CARGO_PKG_VERSION") {
    Some(ver) => ver,
    None => "0.0.0",
};

pub static EXAMPLE_TEXT: &str = "The quick brown fox jumps over the lazy dog.";

#[inline(always)]
fn log_init(task: &str) {
    log::info!("{task} initialized");
}

fn init_wireless(
    timer: impl Into<ErasedTimer>,
    rng: Rng,
    radio_clocks: RADIO_CLK,
    clocks: &Clocks<'static>,
) -> EspWifiInitialization {
    use EspWifiInitFor::*;
    let timer = timer.into();

    let ret = esp_wifi::initialize(Ble, timer, rng, radio_clocks, clocks)
        .expect("BLE to be properly initialized");

    log_init("BLE");

    ret
}

fn init_embassy(clocks: &Clocks<'static>, timer_group: TIMG0) {
    let timer_group = TimerGroup::new(timer_group, clocks);
    let timer0: ErasedTimer = timer_group.timer0.into();
    let timer1: ErasedTimer = timer_group.timer1.into();
    esp_hal_embassy::init(clocks, [timer0, timer1]);
    log_init("embassy");
}

#[main]
async fn main(spawner: Spawner) {
    let peripherals = Peripherals::take();

    // Logging and allocation init
    esp_println::logger::init_logger_from_env();
    log_init("logging");

    psram::init_psram(peripherals.PSRAM);
    unsafe { ALLOCATOR.init(psram::psram_vaddr_start() as *mut u8, psram::PSRAM_BYTES) };
    log_init("allocator");

    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = &*make_static!(
        Clocks<'static>,
        ClockControl::max(system.clock_control).freeze()
    );
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let dma = Dma::new(peripherals.DMA);
    let wireless_timers = TimerGroup::new(peripherals.TIMG1, clocks);
    let trng = Trng::new(peripherals.RNG, peripherals.ADC1);
    let rng = trng.rng;

    init_embassy(clocks, peripherals.TIMG0);

    let fs = Filesystem::new(FlashStorage::new(), rng).await.unwrap();
    FILESYSTEM.init(fs);
    log_init("filesystem");

    spawner.must_spawn(lcd::start(
        peripherals.SPI2,
        io.pins.gpio7,
        io.pins.gpio9,
        io.pins.gpio44,
        dma,
        clocks,
    ));

    spawner.must_spawn(shell::start(peripherals.USB_DEVICE));

    init_wireless(wireless_timers.timer0, rng, peripherals.RADIO_CLK, clocks);

    // let mut app_cpu = AppCpu::new(peripherals.CPU_CTRL);
    // app_cpu.start(
    //     trng,
    //     system.software_interrupt_control.software_interrupt0,
    // );
}

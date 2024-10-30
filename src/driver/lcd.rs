// The display and driver board in the watch come from Adafruit. Much of the code in this module
// is derived from the Adafruit example Arduino sketch for the SHARP LS013B7DH05 and its driver
// board.
//
// Fun fact: this is most likely the display (the module, not the driver board) that was used in the
// Pebble and Pebble Steel, which were the inspiration for making this smartwatch.
//
// The product can be found at https://www.adafruit.com/product/3502
// Adafruit's example code can be found at https://github.com/adafruit/Adafruit_SHARP_Memory_Display
//
// Adafruit's example code is licensed under the BSD 3-Clause License at
// https://github.com/adafruit/Adafruit_SHARP_Memory_Display/blob/master/license.txt

use crate::log_init;
use crate::macros::singleton;
use crate::widget::Widget;
use bitflags::bitflags;
use core::convert::Infallible;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::task;
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as CsRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{DrawTarget, OriginDimensions, Size};
use embedded_graphics::Pixel;
use embedded_hal_async::spi::SpiBus;
use esp_hal::clock::Clocks;
use esp_hal::dma::{Dma, DmaPriority, DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::gpio::{GpioPin, Level, Output};
use esp_hal::peripherals::SPI2;
use esp_hal::spi::master::Spi;
use esp_hal::spi::{SpiBitOrder, SpiMode};
use fugit::RateExtU32;

pub(crate) const LCD_X: u8 = 144;
pub(crate) const LCD_Y: u8 = 168;
pub(crate) const LCD_BUFFER_SIZE: usize = (LCD_X as usize * LCD_Y as usize) / 8;
pub(crate) const LCD_DMA_BUFFER_SIZE: usize = SPI_BUFFER_SIZE * LCD_Y as usize + 2;
pub(crate) const LCD_SPI_FREQ: u32 = 2_000_000;
pub(crate) const LCD_REFRESH_TIME: Duration = Duration::from_hz(60);
const BYTES_PER_LINE: usize = LCD_X as usize / 8;
const SPI_BUFFER_SIZE: usize = BYTES_PER_LINE + 2;

static LCD_INITIALIZED: AtomicBool = AtomicBool::new(false);
pub static LCD_BUFFER: Mutex<CsRawMutex, LcdBuffer> = Mutex::new(LcdBuffer::new());

macro_rules! data {
    ($($val:expr),* $(,)?) => {
        &[$(ToU8::to_u8($val)),*]
    }
}

pub async fn draw<W: Widget>(widget: W) {
    let mut buffer = LCD_BUFFER.lock().await;
    widget.render(&mut buffer);
}

pub async fn clear() {
    let mut buffer = LCD_BUFFER.lock().await;
    buffer.clear()
}

#[task]
pub async fn start(
    spi: SPI2,
    sck: GpioPin<7>,
    mosi: GpioPin<9>,
    cs: GpioPin<44>,
    dma: Dma<'static>,
) -> ! {
    let mut local_buffer;
    let (lcd_rx, lcd_rx_descriptors, lcd_tx, lcd_tx_descriptors) =
        dma_buffers!(LCD_DMA_BUFFER_SIZE);

    let tx = DmaTxBuf::new(lcd_tx_descriptors, lcd_tx).unwrap();
    let rx = DmaRxBuf::new(lcd_rx_descriptors, lcd_rx).unwrap();

    let spi = Spi::new(spi, LCD_SPI_FREQ.Hz(), SpiMode::Mode0)
        .with_sck(sck)
        .with_mosi(mosi)
        .with_bit_order(SpiBitOrder::LSBFirst, SpiBitOrder::LSBFirst)
        .with_dma(
            dma.channel0
                .configure_for_async(false, DmaPriority::Priority0),
        )
        .with_buffers(rx, tx);

    let mut lcd = Lcd::new(spi, cs);

    log_init("display");

    loop {
        // yielding ensures that other tasks get a chance to run, since otherwise running the
        // display might take up all the executor's time.
        // TODO: Check if this is necessary when copying to a local buffer.
        yield_now().await;

        let render_start = Instant::now();

        {
            // Copying to a local buffer prevents holding the mutex lock for ~14ms while the display
            // is being drawn to.
            let mut buffer = LCD_BUFFER.lock().await;
            local_buffer = *buffer;
            buffer.refreshed();
        }

        if local_buffer.needs_clear() {
            lcd.clear().await;
        } else if local_buffer.needs_refresh() {
            lcd.refresh(&mut local_buffer).await;
        }

        let elapsed = render_start.elapsed();

        if elapsed < LCD_REFRESH_TIME {
            // Limit the framerate to the maximum allowed.
            // FIXME: This timer always seems to stop before the 16.66.. ms is up,
            // maybe something to do with embassy's default tick frequency?
            Timer::after(LCD_REFRESH_TIME - elapsed).await;
        }
    }
}

type OutputPin<const N: u8> = Output<'static, GpioPin<N>>;

bitflags! {
    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    struct LcdCommand: u8 {
        const WRITE = 0b0000_0001;
        const VCOM  = 0b0000_0010;
        const CLEAR = 0b0000_0100;
    }
}

trait ToU8 {
    fn to_u8(self) -> u8;
}

impl ToU8 for LcdCommand {
    #[inline(always)]
    fn to_u8(self) -> u8 {
        self.bits()
    }
}

impl ToU8 for u8 {
    #[inline(always)]
    fn to_u8(self) -> u8 {
        self
    }
}

pub struct Lcd<Spi> {
    spi: Spi,
    cs: OutputPin<44>,
    vcom: LcdCommand,
}

impl<Spi> Lcd<Spi> {
    pub fn new(spi: Spi, cs: GpioPin<44>) -> Self {
        singleton! {
            LCD_INITIALIZED,
            "attempted to initialize LCD more than once",
            || {
                let cs = Output::new_typed(cs, Level::Low);

                Self {
                    spi,
                    cs,
                    vcom: LcdCommand::VCOM,
                }
            }
        }
    }

    #[inline(always)]
    fn toggle_vcom(&mut self) {
        if self.vcom == LcdCommand::VCOM {
            self.vcom = LcdCommand::empty();
        } else {
            self.vcom = LcdCommand::VCOM;
        }
    }
}

impl<Spi: SpiBus> Lcd<Spi> {
    pub async fn clear(&mut self) {
        self.cs.set_high();

        self.write_command(data!(self.vcom | LcdCommand::CLEAR, 0x00))
            .await;
        self.toggle_vcom();

        self.cs.set_low();
    }

    pub async fn refresh(&mut self, buffer: &mut LcdBuffer) {
        let mut spi_data = heapless::Vec::<_, LCD_DMA_BUFFER_SIZE>::new();

        self.cs.set_high();
        // self.write_command(data!(self.vcom | LcdCommand::WRITE)).await;
        spi_data
            .extend_from_slice(data!(self.vcom | LcdCommand::WRITE))
            .unwrap();
        self.toggle_vcom();

        for line_number in buffer.min_changed..buffer.max_changed {
            let mut data = [0x00u8; SPI_BUFFER_SIZE];

            data[0] = line_number + 1;
            data[1..BYTES_PER_LINE + 1].copy_from_slice(buffer.get_line(line_number as usize));

            spi_data.extend_from_slice(&data).unwrap()
        }

        spi_data.extend_from_slice(data!(0x00)).unwrap();

        self.write_command(spi_data.as_slice()).await;
        self.cs.set_low();
        buffer.refreshed();
    }

    async fn write_command(&mut self, data: &[u8]) {
        self.spi
            .write(data)
            .await
            .expect("failed to write data to SPI bus");
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct LcdBuffer {
    buf: [u8; LCD_BUFFER_SIZE],
    min_changed: u8,
    max_changed: u8,
    clear: bool,
}

impl LcdBuffer {
    pub const fn new() -> Self {
        Self {
            buf: [0xff; LCD_BUFFER_SIZE],
            min_changed: LCD_Y,
            max_changed: 0,
            clear: false,
        }
    }

    pub fn fill(&mut self, byte: u8) {
        self.buf.fill(byte);
        self.min_changed = 0;
        self.max_changed = LCD_Y;
    }

    pub fn clear(&mut self) {
        self.buf.fill(0xff);
        self.min_changed = LCD_Y;
        self.max_changed = 0;
        self.clear = true;
    }

    pub fn set_pixel<T>(&mut self, x: T, y: T, color: BinaryColor)
    where
        T: TryInto<u8>,
    {
        let (Ok(x), Ok(y)) = (x.try_into(), y.try_into()) else {
            // If it's out of range for u8, it's obviously out of range
            // for the width or height of the LCD.
            return;
        };

        self.set_pixel_internal(x, y, color)
    }

    pub(crate) fn set_pixel_internal(&mut self, x: u8, y: u8, color: BinaryColor) {
        const LCD_WHITE_LUT: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
        const LCD_BLACK_LUT: [u8; 8] = [!1, !2, !4, !8, !16, !32, !64, !128];

        if x < LCD_X && y < LCD_Y {
            let (index, bit) = Self::get_index_and_bit(x, y);

            if color.is_on() {
                self.buf[index] &= LCD_BLACK_LUT[bit];
            } else {
                self.buf[index] |= LCD_WHITE_LUT[bit];
            }

            self.min_changed = self.min_changed.min(y);
            self.max_changed = self.max_changed.max(y.saturating_add(1)).min(LCD_Y);
        }
    }

    pub fn copy_from_buffer(&mut self, other: &Self) {
        *self = *other
    }

    pub fn get_line(&self, n: usize) -> &[u8] {
        let index = n * BYTES_PER_LINE;

        &self.buf[index..index + BYTES_PER_LINE]
    }

    pub fn get_line_mut(&mut self, n: usize) -> &mut [u8] {
        let index = n * BYTES_PER_LINE;

        &mut self.buf[index..index + BYTES_PER_LINE]
    }

    pub fn refreshed(&mut self) {
        self.min_changed = LCD_Y;
        self.max_changed = 0;
        self.clear = false;
    }

    pub fn needs_refresh(&self) -> bool {
        self.min_changed != LCD_Y && self.max_changed != 0
    }

    pub fn needs_clear(&self) -> bool {
        self.clear
    }

    #[inline]
    const fn get_index_and_bit(x: u8, y: u8) -> (usize, usize) {
        let index = (x as usize + LCD_X as usize * y as usize) >> 3;
        let bit = (x & 7) as usize;

        (index, bit)
    }
}

impl Default for LcdBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OriginDimensions for LcdBuffer {
    fn size(&self) -> Size {
        Size::new(LCD_X as u32, LCD_Y as u32)
    }
}

impl DrawTarget for LcdBuffer {
    type Color = BinaryColor;

    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels.into_iter() {
            self.set_pixel(point.x, point.y, color);
        }

        Ok::<_, Infallible>(())
    }
}

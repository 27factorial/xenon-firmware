use alloc::boxed::Box;
use alloc::vec::Vec;
use core::iter::{self, FusedIterator};
use embedded_graphics::image::ImageDrawable;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{
    DrawTarget, DrawTargetExt, OriginDimensions, PixelIteratorExt, Point, Size,
};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::Pixel;
use miniz_oxide::inflate::{self, TINFLStatus};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BITMAP_COLOR_BLACK: u8 = 0b00;
const BITMAP_COLOR_WHITE: u8 = 0b01;
const BITMAP_COLOR_TRANSPARENT: u8 = 0b11;
pub(crate) const MAX_BITMAP_WIDTH: u8 = 240;
pub(crate) const MAX_BITMAP_HEIGHT: u8 = u8::MAX;

// 2 bpp
// a maximum of MAX_WIDTH pixels wide, divided by 8 to get bytes
// a maximum of MAX_HEIGHT lines high.
pub(crate) const MAX_IMAGE_SIZE: usize =
    2 * (MAX_BITMAP_WIDTH / 8) as usize * MAX_BITMAP_HEIGHT as usize;

#[inline(always)]
pub const fn bitmap_buffer() -> [u8; MAX_IMAGE_SIZE] {
    [0; MAX_IMAGE_SIZE]
}

pub fn expected_data_len(width: u8, height: u8) -> usize {
    let width_extra_byte = if width % 4 == 0 { 0 } else { 1 };
    let width_bytes = (width / 4) as usize + width_extra_byte;
    let height_lines = height as usize;
    width_bytes * height_lines
}

fn check(width: u8, height: u8, data: &[u8]) -> Result<(), BitmapError> {
    if width > MAX_BITMAP_WIDTH {
        return Err(BitmapError::InvalidDimensions { width, height });
    }

    let expected_len = expected_data_len(width, height);

    if expected_len != data.len() {
        return Err(BitmapError::LengthMismatch {
            expected: expected_len,
            actual: data.len(),
        });
    }

    Ok(())
}

fn set_pixel_internal(width: u8, height: u8, x: u8, y: u8, color: PixelColor, data: &mut [u8]) {
    const SET_LUT: [u8; 4] = [192, 48, 12, 3];
    const UNSET_LUT: [u8; 4] = [!192, !48, !12, !3];
    const WHITE_LUT: [u8; 4] = [64, 16, 4, 1];

    if x < width && y < height {
        let (index, pos) = get_index_pos_internal(width, x, y);

        match color {
            PixelColor::Black => {
                data[index] &= UNSET_LUT[pos];
            }
            PixelColor::White => {
                data[index] &= UNSET_LUT[pos];
                data[index] |= WHITE_LUT[pos];
            }
            PixelColor::Transparent => {
                data[index] |= SET_LUT[pos];
            }
        }
    }
}

fn get_pixel_internal(width: u8, height: u8, x: u8, y: u8, data: &[u8]) -> Option<PixelColor> {
    const WHITE_LUT: [u8; 4] = [
        BITMAP_COLOR_WHITE << 6,
        BITMAP_COLOR_WHITE << 4,
        BITMAP_COLOR_WHITE << 2,
        BITMAP_COLOR_WHITE,
    ];
    const TRANSPARENT_LUT: [u8; 4] = [
        BITMAP_COLOR_TRANSPARENT << 6,
        BITMAP_COLOR_TRANSPARENT << 4,
        BITMAP_COLOR_TRANSPARENT << 2,
        BITMAP_COLOR_TRANSPARENT,
    ];

    if x < width && y < height {
        let (index, mask) = get_index_mask_internal(width, x, y);

        match data[index] & mask {
            BITMAP_COLOR_BLACK => Some(PixelColor::Black),
            color if WHITE_LUT.contains(&color) => Some(PixelColor::White),
            color if TRANSPARENT_LUT.contains(&color) => Some(PixelColor::Transparent),
            _ => unreachable!("masking with {mask:#8b} ensures no other values can be matched"),
        }
    } else {
        None
    }
}

#[inline]
const fn get_index_pos_internal(width: u8, x: u8, y: u8) -> (usize, usize) {
    let actual_width = if width % 8 == 0 {
        width as usize
    } else {
        // This can never overflow, since the constructor checks that the width is <= 240 bits.
        (width as usize).next_multiple_of(4)
    };

    let index = (x as usize + actual_width * y as usize) / 4;
    let pos = (x % 4) as usize;

    (index, pos)
}

#[inline]
const fn get_index_mask_internal(width: u8, x: u8, y: u8) -> (usize, u8) {
    const MASK: [u8; 4] = [192, 48, 12, 3];

    let (index, pos) = get_index_pos_internal(width, x, y);
    let mask = MASK[pos];

    (index, mask)
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CompressedBitmap {
    width: u8,
    height: u8,
    data: Box<[u8]>,
}

impl CompressedBitmap {
    pub fn new(bytes: &[u8]) -> Result<Self, BitmapError> {
        let mut iter = bytes.iter();

        let &width = iter.next().ok_or(BitmapError::NoWidth)?;
        let &height = iter.next().ok_or(BitmapError::NoHeight)?;

        Ok(Self {
            width,
            height,
            data: iter.as_slice().to_vec().into_boxed_slice(),
        })
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn decompress(self) -> Result<Bitmap, BitmapError> {
        let mut bytes = [0u8; MAX_IMAGE_SIZE];

        let len = inflate::decompress_slice_iter_to_slice(
            &mut bytes,
            iter::once(&self.data[..]),
            true,
            false,
        )
        .map_err(BitmapError::DecompressionFailed)?;

        Bitmap::new(self.width, self.height, &bytes[..len])
    }

    pub fn decompress_to_ref<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<BitmapRef<'buf>, BitmapError> {
        let len =
            inflate::decompress_slice_iter_to_slice(buf, iter::once(&self.data[..]), true, false)
                .map_err(BitmapError::DecompressionFailed)?;

        BitmapRef::new(self.width, self.height, &buf[..len])
    }

    pub fn decompress_to_ref_mut<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<BitmapRefMut<'buf>, BitmapError> {
        let len =
            inflate::decompress_slice_iter_to_slice(buf, iter::once(&self.data[..]), true, false)
                .map_err(BitmapError::DecompressionFailed)?;

        BitmapRefMut::new(self.width, self.height, &mut buf[..len])
    }
}

impl OriginDimensions for CompressedBitmap {
    fn size(&self) -> Size {
        Size::new(self.width as _, self.height as _)
    }
}

impl ImageDrawable for CompressedBitmap {
    type Color = BinaryColor;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut buf = [0; MAX_IMAGE_SIZE];

        let bitmap = self
            .decompress_to_ref(&mut buf)
            .expect("failed to decompress bitmap");

        bitmap.draw(target)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut buf = [0; MAX_IMAGE_SIZE];

        let bitmap = self
            .decompress_to_ref(&mut buf)
            .expect("failed to decompress bitmap");

        bitmap.draw_sub_image(target, area)
    }
}

pub struct CompressedBitmapRef<'data> {
    width: u8,
    height: u8,
    data: &'data [u8],
}

impl<'data> CompressedBitmapRef<'data> {
    pub fn new(width: u8, height: u8, data: &'data [u8]) -> Self {
        Self {
            width,
            height,
            data,
        }
    }

    pub fn from_encoded(bytes: &'data [u8]) -> Result<Self, BitmapError> {
        let mut iter = bytes.iter();

        let &width = iter.next().ok_or(BitmapError::NoWidth)?;
        let &height = iter.next().ok_or(BitmapError::NoHeight)?;

        Ok(Self {
            width,
            height,
            data: iter.as_slice(),
        })
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn decompress(self) -> Result<Bitmap, BitmapError> {
        let mut bytes = [0u8; MAX_IMAGE_SIZE];

        let len =
            inflate::decompress_slice_iter_to_slice(&mut bytes, iter::once(self.data), true, false)
                .map_err(BitmapError::DecompressionFailed)?;

        Bitmap::new(self.width, self.height, &bytes[..len])
    }

    pub fn decompress_to_ref<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<BitmapRef<'buf>, BitmapError> {
        let len = inflate::decompress_slice_iter_to_slice(buf, iter::once(self.data), true, false)
            .map_err(BitmapError::DecompressionFailed)?;

        BitmapRef::new(self.width, self.height, &buf[..len])
    }

    pub fn decompress_to_ref_mut<'buf>(
        &self,
        buf: &'buf mut [u8],
    ) -> Result<BitmapRefMut<'buf>, BitmapError> {
        let len = inflate::decompress_slice_iter_to_slice(buf, iter::once(self.data), true, false)
            .map_err(BitmapError::DecompressionFailed)?;

        BitmapRefMut::new(self.width, self.height, &mut buf[..len])
    }
}

impl OriginDimensions for CompressedBitmapRef<'_> {
    fn size(&self) -> Size {
        Size::new(self.width as _, self.height as _)
    }
}

impl ImageDrawable for CompressedBitmapRef<'_> {
    type Color = BinaryColor;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut buf = [0; MAX_IMAGE_SIZE];

        let bitmap = self
            .decompress_to_ref(&mut buf)
            .expect("failed to decompress bitmap");

        bitmap.draw(target)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut buf = [0; MAX_IMAGE_SIZE];

        let bitmap = self
            .decompress_to_ref(&mut buf)
            .expect("failed to decompress bitmap");

        bitmap.draw_sub_image(target, area)
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct Bitmap {
    width: u8,
    height: u8,
    data: Box<[u8]>,
}

impl Bitmap {
    pub fn new(
        width: u8,
        height: u8,
        data: impl Into<Vec<u8>> + AsRef<[u8]>,
    ) -> Result<Self, BitmapError> {
        check(width, height, data.as_ref())?;

        Ok(Self {
            width,
            height,
            data: data.into().into_boxed_slice(),
        })
    }

    pub fn from_encoded(encoded: &[u8]) -> Result<Self, BitmapError> {
        let mut iter = encoded.iter();

        let &width = iter.next().ok_or(BitmapError::NoWidth)?;
        let &height = iter.next().ok_or(BitmapError::NoHeight)?;

        Self::new(width, height, iter.as_slice())
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn set_pixel(&mut self, x: u8, y: u8, color: PixelColor) {
        set_pixel_internal(self.width, self.height, x, y, color, &mut self.data)
    }

    pub fn get_pixel(&self, x: u8, y: u8) -> Option<PixelColor> {
        get_pixel_internal(self.width, self.height, x, y, &self.data)
    }

    pub fn as_ref(&self) -> BitmapRef<'_> {
        BitmapRef {
            width: self.width,
            height: self.height,
            data: &self.data,
        }
    }

    pub fn as_mut(&mut self) -> BitmapRefMut<'_> {
        BitmapRefMut {
            width: self.width,
            height: self.height,
            data: &mut self.data,
        }
    }

    pub fn pixels(&self) -> Pixels<'_> {
        Pixels {
            x: 0,
            y: 0,
            bitmap: self.as_ref(),
        }
    }
}

impl OriginDimensions for Bitmap {
    fn size(&self) -> Size {
        Size::new(self.width as _, self.height as _)
    }
}

impl ImageDrawable for Bitmap {
    type Color = BinaryColor;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.as_ref().draw(target)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.as_ref().draw_sub_image(target, area)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct BitmapRef<'data> {
    width: u8,
    height: u8,
    data: &'data [u8],
}

impl<'data> BitmapRef<'data> {
    pub fn new(width: u8, height: u8, data: &'data [u8]) -> Result<Self, BitmapError> {
        check(width, height, data)?;

        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn new_prechecked(width: u8, height: u8, data: &'data [u8]) -> Self {
        Self {
            width,
            height,
            data,
        }
    }

    pub fn from_encoded(encoded: &'data [u8]) -> Result<Self, BitmapError> {
        let mut iter = encoded.iter();

        let &width = iter.next().ok_or(BitmapError::NoWidth)?;
        let &height = iter.next().ok_or(BitmapError::NoHeight)?;

        Self::new(width, height, iter.as_slice())
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub(crate) fn data(&self) -> &[u8] {
        self.data
    }

    pub fn get_pixel(&self, x: u8, y: u8) -> Option<PixelColor> {
        get_pixel_internal(self.width, self.height, x, y, self.data)
    }

    pub fn to_image(self) -> Bitmap {
        Bitmap {
            width: self.width,
            height: self.height,
            data: self.data.into(),
        }
    }

    pub fn pixels(&self) -> Pixels<'_> {
        Pixels {
            x: 0,
            y: 0,
            bitmap: *self,
        }
    }
}

impl OriginDimensions for BitmapRef<'_> {
    fn size(&self) -> Size {
        Size::new(self.width as _, self.height as _)
    }
}

impl ImageDrawable for BitmapRef<'_> {
    type Color = BinaryColor;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.pixels().draw(target)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.draw(&mut target.translated(-area.top_left).clipped(area))
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct BitmapRefMut<'data> {
    width: u8,
    height: u8,
    data: &'data mut [u8],
}

impl<'data> BitmapRefMut<'data> {
    pub fn new(width: u8, height: u8, data: &'data mut [u8]) -> Result<Self, BitmapError> {
        check(width, height, data)?;

        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub fn from_encoded(encoded: &'data mut [u8]) -> Result<Self, BitmapError> {
        let mut iter = encoded.iter_mut();

        let &mut width = iter.next().ok_or(BitmapError::NoWidth)?;
        let &mut height = iter.next().ok_or(BitmapError::NoHeight)?;

        Self::new(width, height, iter.into_slice())
    }

    pub fn width(&self) -> u8 {
        self.width
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub(crate) fn data(&self) -> &[u8] {
        self.data
    }

    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }

    pub fn set_pixel(&mut self, x: u8, y: u8, color: PixelColor) {
        set_pixel_internal(self.width, self.height, x, y, color, self.data)
    }

    pub fn get_pixel(&self, x: u8, y: u8) -> Option<PixelColor> {
        get_pixel_internal(self.width, self.height, x, y, self.data)
    }

    pub fn as_ref(&self) -> BitmapRef<'_> {
        BitmapRef {
            width: self.width,
            height: self.height,
            data: self.data,
        }
    }

    pub fn to_image(self) -> Bitmap {
        Bitmap {
            width: self.width,
            height: self.height,
            data: (&*self.data).into(),
        }
    }

    pub fn pixels(&self) -> Pixels<'_> {
        Pixels {
            x: 0,
            y: 0,
            bitmap: self.as_ref(),
        }
    }
}

impl OriginDimensions for BitmapRefMut<'_> {
    fn size(&self) -> Size {
        Size::new(self.width as _, self.height as _)
    }
}

impl ImageDrawable for BitmapRefMut<'_> {
    type Color = BinaryColor;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.as_ref().draw(target)
    }

    fn draw_sub_image<D>(&self, target: &mut D, area: &Rectangle) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.as_ref().draw_sub_image(target, area)
    }
}

pub struct Pixels<'data> {
    x: u8,
    y: u8,
    bitmap: BitmapRef<'data>,
}

impl Iterator for Pixels<'_> {
    type Item = Pixel<BinaryColor>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let bitmap_pixel_color = self.bitmap.get_pixel(self.x, self.y)?;
            let point = Point::new(self.x as i32, self.y as i32);

            if self.x == self.bitmap.width - 1 {
                // eventually this will cause self.y to go out of bounds and the get_pixel
                // call to return None every time, signalling the end of iteration.
                self.x = 0;
                self.y += 1;
            } else {
                self.x += 1;
            }

            // Transparent essentially means "don't draw over this pixel", so transparent pixels
            // make the iterator go to the next pixel, and repeat until an opaque pixel is found.
            let binary_color = match bitmap_pixel_color {
                PixelColor::Black => BinaryColor::On,
                PixelColor::White => BinaryColor::Off,
                PixelColor::Transparent => continue,
            };

            break Some(Pixel(point, binary_color));
        }
    }
}

impl FusedIterator for Pixels<'_> {}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Error)]
pub enum BitmapError {
    #[error("no width specified")]
    NoWidth,
    #[error("no height specified")]
    NoHeight,
    #[error("invalid dimensions ({width}x{height})")]
    InvalidDimensions { width: u8, height: u8 },
    #[error("length mismatch, expected {expected} got {actual}")]
    LengthMismatch { expected: usize, actual: usize },
    #[error("decompression error: {0:?}")]
    DecompressionFailed(TINFLStatus),
}

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum PixelColor {
    Black = BITMAP_COLOR_BLACK,
    White = BITMAP_COLOR_WHITE,
    Transparent = BITMAP_COLOR_TRANSPARENT,
}

// WASM compatibility
impl TryFrom<u32> for PixelColor {
    type Error = InvalidPixelColorError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match u8::try_from(value) {
            Ok(BITMAP_COLOR_BLACK) => Ok(Self::Black),
            Ok(BITMAP_COLOR_WHITE) => Ok(Self::White),
            Ok(BITMAP_COLOR_TRANSPARENT) => Ok(Self::Transparent),
            _ => Err(InvalidPixelColorError),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Error)]
#[error("invalid pixel color")]
pub struct InvalidPixelColorError;

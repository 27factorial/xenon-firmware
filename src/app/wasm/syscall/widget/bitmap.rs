use crate::app::wasm::types::{Env, Error};
use crate::driver::lcd;
use crate::macros::syscalls;
use crate::widget::bitmap::{
    self, BitmapError, BitmapRef, BitmapRefMut, CompressedBitmapRef, PixelColor,
};
use core::any::type_name;
use embassy_executor::task;
use embedded_graphics::image::Image;
use embedded_graphics::prelude::Point;

fn bitmap_error_to_wasm(err: BitmapError) -> (i32, u32, u32) {
    match err {
        BitmapError::NoWidth => (-1, 0, 0),
        BitmapError::NoHeight => (-2, 0, 0),
        BitmapError::InvalidDimensions { width, height } => (-3, width as u32, height as u32),
        BitmapError::LengthMismatch { expected, actual } => (-4, expected as u32, actual as u32),
        BitmapError::DecompressionFailed(_) => (-5, 0, 0),
    }
}

fn pixel_to_wasm(pixel: Option<PixelColor>) -> u32 {
    match pixel {
        None => 0,
        Some(PixelColor::Black) => 1,
        Some(PixelColor::White) => 2,
        Some(PixelColor::Transparent) => 3,
    }
}

fn wasm_to_pixel(pixel: u32) -> Option<PixelColor> {
    match pixel {
        1 => Some(PixelColor::Black),
        2 => Some(PixelColor::White),
        3 => Some(PixelColor::Transparent),
        _ => None,
    }
}

#[task]
async fn draw_compressed_bitmap_internal(
    env: Env,
    width: u8,
    height: u8,
    index: usize,
    position: Point,
) {
    let env_data = env.lock_async().await;
    let data = env_data.get_binary_data(index).unwrap();

    let bitmap = CompressedBitmapRef::new(width, height, data);

    lcd::draw(Image::new(&bitmap, position)).await;
}

#[task]
async fn draw_bitmap_internal(env: Env, width: u8, height: u8, index: usize, position: Point) {
    let env = env.lock_async().await;
    let data = env.get_binary_data(index).unwrap();

    let bitmap = BitmapRef::new_prechecked(width, height, data);

    lcd::draw(Image::new(&bitmap, position)).await;
}

syscalls! {
    pub extern "wasm" fn load_compressed_bitmap(
        caller,
        ptr: usize,
        len: usize,
    ) -> Result<i32, wasmi::Error> {
        let memory = caller.data().lock().memory();
        let end = ptr + len;

        let bytes = memory.data(&caller)
            .get(ptr..end)
            .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

        let idx = caller.data().lock().push_binary_data(bytes);

        Ok(idx as i32)
    }

    pub extern "wasm" fn load_bitmap(
        caller,
        width: u8,
        height: u8,
        ptr: usize,
        e1_ptr: usize,
        e2_ptr: usize,
    ) -> Result<i32, wasmi::Error> {
        let memory = caller.data().lock().memory();

        let expected_len = bitmap::expected_data_len(width, height);
        let end = ptr + expected_len;

        let bytes = memory.data(&caller)
            .get(ptr..end)
            .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

        let bitmap = match BitmapRef::new(width, height, bytes).map_err(bitmap_error_to_wasm) {
            Ok(b) => b,
            Err((code, e1, e2)) => {
                memory.write(&mut caller, e1_ptr, &e1.to_le_bytes())?;
                memory.write(&mut caller, e2_ptr, &e2.to_le_bytes())?;

                return Ok(code)
            }
        };

        let idx = caller.data().lock().push_binary_data(bitmap.data());

        Ok(idx as i32)
    }

    pub extern "wasm" fn decompress_bitmap(
        caller,
        id: i32,
        width: u8,
        height: u8,
        e1_ptr: usize,
        e2_ptr: usize,
    ) -> Result<i32, wasmi::Error> {
        let mut env = caller.data().lock();
        let memory = env.memory();

        let data = usize::try_from(id)
            .map_err(|_| Error::InvalidId(id))
            .and_then(|index| env.get_binary_data_mut(index).ok_or(Error::InvalidId(id)))?;

        let compressed = CompressedBitmapRef::new(width, height, data);
        let mut buf = bitmap::bitmap_buffer();
        let decompressed = match compressed.decompress_to_ref(&mut buf).map_err(bitmap_error_to_wasm) {
            Ok(bitmap) => bitmap,
            Err((code, e1, e2)) => {
                // explicitly end lifetime of `env` so `caller` can be borrowed mutably.
                drop(env);
                memory.write(&mut caller, e1_ptr, &e1.to_le_bytes())?;
                memory.write(&mut caller, e2_ptr, &e2.to_le_bytes())?;

                return Ok(code)
            }
        };

        data.clear();
        data.extend_from_slice(decompressed.data());

        Ok(0)
    }

    pub extern "wasm" fn draw_compressed_bitmap(
        caller,
        id: i32,
        width: u8,
        height: u8,
        x: i32,
        y: i32,
    ) -> Result<(), wasmi::Error> {
        let env = caller.data();
        let env_data = env.lock();

        let index = usize::try_from(id).map_err(|_| Error::InvalidId(id))?;

        match env_data.get_binary_data(index) {
            Some(_) => {
                env_data.spawn(draw_bitmap_internal(
                    env.clone(),
                    width,
                    height,
                    index,
                    Point::new(x, y)
                ))?;

                Ok(())
            }
            None => Err(Error::InvalidId(id).into()),
        }
    }

    pub extern "wasm" fn draw_bitmap(
        caller,
        id: i32,
        width: u8,
        height: u8,
        x: i32,
        y: i32,
    ) -> Result<(), wasmi::Error> {
        let env = caller.data();
        let env_data = env.lock();

        let index = usize::try_from(id).map_err(|_| Error::InvalidId(id))?;
        let data = env_data.get_binary_data(index).ok_or(Error::InvalidId(id))?;

        if BitmapRef::new(width, height, data).is_ok() {
            env_data.spawn(draw_bitmap_internal(
                env.clone(),
                width,
                height,
                index,
                Point::new(x, y)
            ))?;
        }

        Ok(())
    }

    pub extern "wasm" fn get_bitmap_pixel(
        caller,
        id: i32,
        width: u8,
        height: u8,
        x: u8,
        y: u8,
    ) -> Result<u32, wasmi::Error> {
        let env = caller.data().lock();

        let index = usize::try_from(id).map_err(|_| Error::InvalidId(id))?;
        let data = env.get_binary_data(index).ok_or(Error::InvalidId(id))?;

        match BitmapRef::new(width, height, data) {
            Ok(bitmap) => {
                Ok(pixel_to_wasm(bitmap.get_pixel(x, y)))
            }
            Err(_) => {
                Ok(pixel_to_wasm(None))
            }
        }
    }

    pub extern "wasm" fn set_bitmap_pixel(
        caller,
        id: i32,
        width: u8,
        height: u8,
        x: u8,
        y: u8,
        pixel_color: u32,
    ) -> Result<(), wasmi::Error> {
        let mut env = caller.data().lock();

        let index = usize::try_from(id).map_err(|_| Error::InvalidId(id))?;
        let data = env.get_binary_data_mut(index).ok_or(Error::InvalidId(id))?;

        if let Ok(mut bitmap) = BitmapRefMut::new(width, height, data) {
            let pixel = wasm_to_pixel(pixel_color).ok_or(Error::InvalidValue(type_name::<PixelColor>()))?;

            bitmap.set_pixel(x, y, pixel);
        }

        Ok(())
    }
}

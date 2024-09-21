use crate::driver::lcd::LcdBuffer;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::Drawable;

pub mod bitmap;
pub mod button;
pub mod collections;
pub mod misc;
pub mod text;

pub trait Widget {
    fn render(&self, buffer: &mut LcdBuffer);
}

impl<T, O> Widget for T
where
    T: Drawable<Color = BinaryColor, Output = O>,
{
    fn render(&self, buffer: &mut LcdBuffer) {
        let _ = self.draw(buffer);
    }
}

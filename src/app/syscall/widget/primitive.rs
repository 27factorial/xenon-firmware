use crate::app::types::{Env, Error};
use crate::driver::lcd;
use crate::macros::{syscall, task};
use core::any::type_name;
use core::mem::size_of;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{Angle, Point, Size};
use embedded_graphics::primitives::{
    Arc, Circle, CornerRadii, CornerRadiiBuilder, Ellipse, Line, PrimitiveStyle,
    PrimitiveStyleBuilder, Rectangle, RoundedRectangle, Sector, StrokeAlignment, Styled, Triangle,
};
use wasmi::Caller;

macro_rules! draw {
    ($t:ty, $shape:expr, $style:expr) => {{
        let styled = Styled::new($shape, $style);

        task! {
            (styled: Styled<$t, PrimitiveStyle<BinaryColor>>) {
                lcd::draw(styled).await;
            }
        }
    }};
}

fn style(
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<PrimitiveStyle<BinaryColor>, Error> {
    let mut style_builder = PrimitiveStyleBuilder::new()
        .stroke_width(stroke_width)
        .stroke_alignment(wasm_to_stroke_align(stroke_alignment)?);

    if let Some(color) = wasm_to_color(fill_color)? {
        style_builder = style_builder.fill_color(color);
    }

    if let Some(color) = wasm_to_color(stroke_color)? {
        style_builder = style_builder.stroke_color(color);
    }

    Ok(style_builder.build())
}

#[inline]
#[track_caller]
fn corner_radii<'a>(iter: &mut impl Iterator<Item = &'a [u8]>) -> CornerRadii {
    let [tl, tr, br, bl] = core::array::from_fn(|_| {
        let Some(&[wa, wb, wc, wd]) = iter.next() else {
            panic!("invalid corner radius iterator");
        };

        let width = u32::from_le_bytes([wa, wb, wc, wd]);

        let Some(&[ha, hb, hc, hd]) = iter.next() else {
            panic!("invalid corner radius iterator");
        };

        let height = u32::from_le_bytes([ha, hb, hc, hd]);

        Size::new(width, height)
    });

    CornerRadiiBuilder::new()
        .top_left(tl)
        .top_right(tr)
        .bottom_right(br)
        .bottom_left(bl)
        .build()
}

fn wasm_to_color(v: u32) -> Result<Option<BinaryColor>, Error> {
    match v {
        0 => Ok(None),
        1 => Ok(Some(BinaryColor::Off)),
        2 => Ok(Some(BinaryColor::On)),
        _ => Err(Error::InvalidValue(type_name::<Option<BinaryColor>>())),
    }
}

fn wasm_to_stroke_align(v: u32) -> Result<StrokeAlignment, Error> {
    match v {
        0 => Ok(StrokeAlignment::Inside),
        1 => Ok(StrokeAlignment::Center),
        2 => Ok(StrokeAlignment::Outside),
        _ => Err(Error::InvalidValue(type_name::<StrokeAlignment>())),
    }
}

#[syscall]
pub extern "wasm" fn draw_arc(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    diameter: u32,
    angle_start: f32,
    angle_sweep: f32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let top_left = Point::new(top_left_x, top_left_y);

    let arc = Arc::new(
        top_left,
        diameter,
        Angle::from_radians(angle_start),
        Angle::from_radians(angle_sweep),
    );

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Arc, arc, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_circle(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    diameter: u32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let top_left = Point::new(top_left_x, top_left_y);

    let circle = Circle::new(top_left, diameter);

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Circle, circle, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_ellipse(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    width: u32,
    height: u32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let top_left = Point::new(top_left_x, top_left_y);
    let size = Size::new(width, height);

    let ellipse = Ellipse::new(top_left, size);

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Ellipse, ellipse, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_line(
    caller: Caller<'_, Env>,
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let start = Point::new(start_x, start_y);
    let end = Point::new(end_x, end_y);
    let line = Line { start, end };

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Line, line, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_rectangle(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    width: u32,
    height: u32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let top_left = Point::new(top_left_x, top_left_y);
    let size = Size::new(width, height);

    let rectangle = Rectangle::new(top_left, size);

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Rectangle, rectangle, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_rounded_rectangle(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    width: u32,
    height: u32,
    corners_ptr: usize,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    // this isn't for calculating bits, it just looks like it because 4 (width, height) pairs
    // means 8 u32s.
    #[allow(clippy::manual_bits)]
    const CORNER_ELEMS: usize = size_of::<u32>() * 8;

    let memory = caller.data().lock_data_blocking().memory();

    let corners_end = corners_ptr + CORNER_ELEMS;
    let corners =
        memory
            .data(&caller)
            .get(corners_ptr..corners_end)
            .ok_or(Error::InvalidMemoryRange {
                start: corners_ptr,
                end: corners_end,
            })?;

    let mut corner_iter = corners.chunks_exact(size_of::<u32>());

    let top_left = Point::new(top_left_x, top_left_y);
    let size = Size::new(width, height);
    let corners = corner_radii(&mut corner_iter);

    let rounded_rectangle = RoundedRectangle::new(Rectangle::new(top_left, size), corners);

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller
        .data()
        .spawn(draw!(RoundedRectangle, rounded_rectangle, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_sector(
    caller: Caller<'_, Env>,
    top_left_x: i32,
    top_left_y: i32,
    diameter: u32,
    angle_start: f32,
    angle_sweep: f32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let top_left = Point::new(top_left_x, top_left_y);

    let sector = Sector::new(
        top_left,
        diameter,
        Angle::from_radians(angle_start),
        Angle::from_radians(angle_sweep),
    );

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Sector, sector, style))?;

    Ok(())
}

#[syscall]
pub extern "wasm" fn draw_triangle(
    caller: Caller<'_, Env>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    fill_color: u32,
    stroke_color: u32,
    stroke_width: u32,
    stroke_alignment: u32,
) -> Result<(), wasmi::Error> {
    let v0 = Point::new(x0, y0);
    let v1 = Point::new(x1, y1);
    let v2 = Point::new(x2, y2);

    let triangle = Triangle::new(v0, v1, v2);

    let style = style(fill_color, stroke_color, stroke_width, stroke_alignment)?;

    caller.data().spawn(draw!(Triangle, triangle, style))?;

    Ok(())
}

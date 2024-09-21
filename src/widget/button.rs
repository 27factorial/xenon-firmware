use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{Dimensions, DrawTarget, Point, Size};
use embedded_graphics::primitives::{
    Circle, Line, PrimitiveStyleBuilder, Rectangle, StrokeAlignment, Styled,
};
use embedded_graphics::Drawable;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct RadioButton {
    top_left: Point,
    diameter: u8,
    border_width: u8,
    selected: bool,
    color: BinaryColor,
}

impl RadioButton {
    pub fn new(top_left: Point, diameter: u8, border_width: u8, color: BinaryColor) -> Self {
        Self {
            top_left,
            diameter,
            border_width,
            selected: false,
            color,
        }
    }

    pub fn select(&mut self) {
        self.selected = true;
    }

    pub fn deselect(&mut self) {
        self.selected = false;
    }
}

impl Dimensions for RadioButton {
    fn bounding_box(&self) -> Rectangle {
        Circle::new(self.top_left, self.diameter as u32).bounding_box()
    }
}

impl Drawable for RadioButton {
    type Color = BinaryColor;

    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let style = PrimitiveStyleBuilder::new()
            .stroke_color(self.color)
            .stroke_width(self.border_width as u32)
            .stroke_alignment(StrokeAlignment::Inside)
            .fill_color(if self.selected {
                self.color
            } else {
                self.color.invert()
            })
            .build();

        let circle = Styled::new(Circle::new(self.top_left, self.diameter as u32), style);

        circle.draw(target)?;

        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Checkbox {
    top_left: Point,
    size: u8,
    border_width: u8,
    line_width: u8,
    selected: bool,
    color: BinaryColor,
}

impl Checkbox {
    pub fn new(
        top_left: Point,
        size: u8,
        border_width: u8,
        line_width: u8,
        color: BinaryColor,
    ) -> Self {
        Self {
            top_left,
            size,
            border_width,
            line_width,
            selected: false,
            color,
        }
    }

    pub fn select(&mut self) {
        self.selected = true;
    }

    pub fn deselect(&mut self) {
        self.selected = false;
    }
}

impl Dimensions for Checkbox {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(self.top_left, Size::new_equal(self.size as u32)).bounding_box()
    }
}

impl Drawable for Checkbox {
    type Color = BinaryColor;

    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let style = PrimitiveStyleBuilder::new()
            .stroke_color(self.color)
            .stroke_width(self.border_width as u32)
            .stroke_alignment(StrokeAlignment::Inside)
            .fill_color(self.color.invert())
            .build();

        let square = Styled::new(
            Rectangle::new(self.top_left, Size::new_equal(self.size as u32)),
            style,
        );

        square.draw(target)?;

        // Draw the "X" to mark it as selected
        if let Some(bottom_right) = square.primitive.bottom_right() {
            if self.selected {
                let top_left = square.primitive.top_left;
                let top_right = Point::new(bottom_right.x, top_left.y);
                let bottom_left = Point::new(top_left.x, bottom_right.y);

                let style = PrimitiveStyleBuilder::new()
                    .stroke_color(self.color)
                    .stroke_width(self.line_width as u32)
                    .build();

                let tl_to_br = Styled::new(Line::new(top_left, bottom_right), style);
                let bl_to_tr = Styled::new(Line::new(bottom_left, top_right), style);

                tl_to_br.draw(target)?;
                bl_to_tr.draw(target)?;
            }
        }

        Ok(())
    }
}

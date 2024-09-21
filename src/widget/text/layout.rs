use super::font::{Font, FontMetrics, GlyphId, GlyphMetrics};
use crate::widget::bitmap::BitmapRef;
use alloc::vec::Vec;
use core::num::Wrapping;
use embedded_graphics::image::Image;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{DrawTarget, Point};
use embedded_graphics::Drawable;

fn whitespace_wrap(
    layout: &mut Layout<'_>,
    index: usize,
    c: char,
    line: &str,
    max_width: i32,
    line_spacing: i32,
) -> bool {
    let next_index = index + c.len_utf8();

    if let Some(rest) = line.get(next_index..) {
        let h_advance = match rest.split_once(|c: char| c.is_whitespace()) {
            Some((next_word, _)) => layout.str_h_advance(next_word),
            None => layout.str_h_advance(rest),
        };

        if layout.current.x + h_advance >= max_width {
            layout.new_line(line_spacing);
            return true;
        }
    }

    false
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub enum WrapMode {
    /// Wrap only on whitespace characters when the max width would be reached by the next word.
    Whitespace,

    /// Wrap on any character when the max width would be reached.
    Character,

    /// Attempt to wrap on whitespace characters first, but wrap anyway if the max width is
    /// reached and no whitespace character is available.
    #[default]
    Both,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Config {
    pub max_width: Option<i32>,
    pub max_height: Option<i32>,
    pub wrap_mode: WrapMode,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct PositionedGlyph<'font> {
    pub position: Point,
    pub bitmap: BitmapRef<'font>,
}

pub struct Layout<'font> {
    start: Point,
    current: Point,
    font: &'font Font,
    config: Config,
    glyphs: Vec<PositionedGlyph<'font>>,
}

impl<'font> Layout<'font> {
    pub fn new(position: Point, font: &'font Font, config: Config) -> Self {
        Self {
            start: position,
            current: position,
            font,
            config,
            glyphs: Vec::new(),
        }
    }

    pub fn with_text<S: AsRef<str>>(&mut self, s: S, color: BinaryColor) -> &mut Self {
        let s = s.as_ref();

        match self.config.wrap_mode {
            WrapMode::Whitespace => self.with_text_whitespace_wrap(s, color),
            WrapMode::Character => self.with_text_char_wrap(s, color),
            WrapMode::Both => self.with_text_both_wrap(s, color),
        }
    }

    pub fn clear(&mut self) {
        self.current = self.start;
        self.glyphs.clear();
    }

    pub fn glyphs(&self) -> &[PositionedGlyph<'font>] {
        &self.glyphs
    }

    fn with_text_char_wrap(&mut self, s: &str, color: BinaryColor) -> &mut Self {
        self.with_text_internal(s, color, |data| {
            let WrapData {
                layout,
                line: _,
                index: _,
                c: _,
                glyph_metrics,
                line_spacing,
            } = data;

            if let Some(max_width) = layout.config.max_width {
                if layout.current.x + glyph_metrics.h_advance >= max_width {
                    layout.new_line(line_spacing);
                }
            }

            false
        })
    }

    fn with_text_whitespace_wrap(&mut self, s: &str, color: BinaryColor) -> &mut Self {
        self.with_text_internal(s, color, |data| {
            let WrapData {
                layout,
                line,
                index,
                c,
                glyph_metrics: _,
                line_spacing,
            } = data;

            if let Some(max_width) = layout.config.max_width {
                if c.is_whitespace()
                    && whitespace_wrap(layout, index, c, line, max_width, line_spacing)
                {
                    return true;
                }
            }

            false
        })
    }

    fn with_text_both_wrap(&mut self, s: &str, color: BinaryColor) -> &mut Self {
        self.with_text_internal(s, color, |data| {
            let WrapData {
                layout,
                line,
                index,
                c,
                glyph_metrics,
                line_spacing,
            } = data;

            if let Some(max_width) = layout.config.max_width {
                if c.is_whitespace()
                    && whitespace_wrap(layout, index, c, line, max_width, line_spacing)
                {
                    return true;
                } else if layout.current.x + glyph_metrics.h_advance >= max_width {
                    layout.new_line(line_spacing);
                }
            }

            false
        })
    }

    fn with_text_internal(
        &mut self,
        s: &str,
        color: BinaryColor,
        wrap: impl Fn(WrapData<'_, '_, '_>) -> bool,
    ) -> &mut Self {
        let font_metrics = self.font.font_metrics();
        let line_spacing = font_metrics.ascent - font_metrics.descent + font_metrics.line_gap;

        let get_glyph: fn(&'_ Font, GlyphId) -> (GlyphMetrics, BitmapRef<'_>) = match color {
            BinaryColor::On => |font: &Font, id| font.black_glyph(id),
            BinaryColor::Off => |font: &Font, id| font.white_glyph(id),
        };

        for mut line in s.lines() {
            if let Some(stripped) = line.strip_suffix(|c: char| c.is_whitespace()) {
                line = stripped;
            }

            for (index, c) in line.char_indices() {
                let Some(glyph) = self.font.id(c) else {
                    continue;
                };

                let (metrics, bitmap) = get_glyph(self.font, glyph);

                let wrap_data = WrapData {
                    layout: self,
                    line,
                    index,
                    c,
                    glyph_metrics: metrics,
                    line_spacing,
                };

                // the condition here indicates whether this character should skip rendering, which
                // happens when whitespace is turned into a linebreak.
                if wrap(wrap_data) {
                    continue;
                }

                self.push_positioned_glyph(font_metrics, metrics, bitmap);
            }

            self.new_line(line_spacing);
        }

        self
    }

    fn push_positioned_glyph(
        &mut self,
        font_metrics: FontMetrics,
        metrics: GlyphMetrics,
        bitmap: BitmapRef<'font>,
    ) {
        let mut position = self.current;
        let y_offset = font_metrics.ascent.wrapping_sub_unsigned(metrics.height) + metrics.y_offset;
        position.y += y_offset;

        let positioned = PositionedGlyph { position, bitmap };

        self.current.x += metrics.h_advance;

        self.glyphs.push(positioned);
    }

    fn str_h_advance(&self, s: &str) -> i32 {
        s.chars()
            .filter_map(|c| self.font.id(c))
            .map(|id| Wrapping(self.font.glyph_metrics(id).h_advance))
            .sum::<Wrapping<i32>>()
            .0
    }

    fn new_line(&mut self, line_spacing: i32) {
        self.current.x = self.start.x;
        self.current.y += line_spacing;
    }
}

impl Drawable for Layout<'_> {
    type Color = BinaryColor;

    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        for positioned in self.glyphs() {
            let image = Image::new(&positioned.bitmap, positioned.position);
            image.draw(target)?;
        }

        Ok(())
    }
}

impl<'f> Drawable for &Layout<'f> {
    type Color = <Layout<'f> as Drawable>::Color;

    type Output = <Layout<'f> as Drawable>::Output;

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        (**self).draw(target)
    }
}

struct WrapData<'l, 's, 'font> {
    layout: &'l mut Layout<'font>,
    line: &'s str,
    index: usize,
    c: char,
    glyph_metrics: GlyphMetrics,
    line_spacing: i32,
}

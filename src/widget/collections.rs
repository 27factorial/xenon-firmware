use super::Widget;
use crate::driver::lcd::LcdBuffer;
use alloc::boxed::Box;
use alloc::vec::Vec;
use heapless::Vec as ConstVec;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct ConstTypedContainer<W, const N: usize>(pub ConstVec<W, N>);

impl<W, const N: usize> ConstTypedContainer<W, N> {
    pub const fn new() -> Self {
        Self(ConstVec::new())
    }

    pub fn with_widget(mut self, widget: W) -> Result<Self, CapacityError<Self, W, N>> {
        match self.0.push(widget) {
            Ok(_) => Ok(self),
            Err(w) => Err(CapacityError {
                container: self,
                widget: w,
            }),
        }
    }
}

impl<W: Widget, const N: usize> Widget for ConstTypedContainer<W, N> {
    fn render(&self, buffer: &mut LcdBuffer) {
        for widget in self.0.iter() {
            widget.render(buffer);
        }
    }
}

impl<W, const N: usize> FromIterator<W> for ConstTypedContainer<W, N> {
    fn from_iter<T: IntoIterator<Item = W>>(iter: T) -> Self {
        Self(ConstVec::from_iter(iter))
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct TypedContainer<W>(pub Vec<W>);

impl<W> TypedContainer<W> {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_widget(mut self, widget: W) -> Self {
        self.0.push(widget);
        self
    }
}

impl<W: Widget> Widget for TypedContainer<W> {
    fn render(&self, buffer: &mut LcdBuffer) {
        for widget in self.0.iter() {
            widget.render(buffer);
        }
    }
}

impl<W> FromIterator<W> for TypedContainer<W> {
    fn from_iter<T: IntoIterator<Item = W>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

#[derive(Default)]
pub struct ConstContainer<const N: usize>(pub ConstVec<Box<dyn Widget>, N>);

impl<const N: usize> ConstContainer<N> {
    pub const fn new() -> Self {
        Self(ConstVec::new())
    }

    pub fn with_widget<W: Widget + 'static>(
        mut self,
        widget: W,
    ) -> Result<Self, CapacityError<Self, Box<dyn Widget>, N>> {
        match self.0.push(Box::new(widget)) {
            Ok(_) => Ok(self),
            Err(w) => Err(CapacityError {
                container: self,
                widget: w,
            }),
        }
    }
}

impl<const N: usize> Widget for ConstContainer<N> {
    fn render(&self, buffer: &mut LcdBuffer) {
        for widget in self.0.iter() {
            widget.render(buffer);
        }
    }
}

impl<const N: usize> FromIterator<Box<dyn Widget>> for ConstContainer<N> {
    fn from_iter<T: IntoIterator<Item = Box<dyn Widget>>>(iter: T) -> Self {
        Self(ConstVec::from_iter(iter))
    }
}

#[derive(Default)]
pub struct Container(pub Vec<Box<dyn Widget>>);

impl Container {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_widget<W: Widget + 'static>(mut self, widget: W) -> Self {
        self.0.push(Box::new(widget));
        self
    }
}

impl Widget for Container {
    fn render(&self, buffer: &mut LcdBuffer) {
        for widget in self.0.iter() {
            widget.render(buffer);
        }
    }
}

impl FromIterator<Box<dyn Widget>> for Container {
    fn from_iter<T: IntoIterator<Item = Box<dyn Widget>>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct CapacityError<C, W, const N: usize> {
    container: C,
    widget: W,
}

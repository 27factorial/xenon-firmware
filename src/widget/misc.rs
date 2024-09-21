use core::cell::{Ref, RefCell, RefMut};
use core::marker::PhantomData;
use embedded_graphics::prelude::{DrawTarget, PixelColor, PixelIteratorExt};
use embedded_graphics::Drawable;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct Dynamic<D, F, S> {
    mode: UpdateMode,
    widget: RefCell<D>,
    state: RefCell<S>,
    f: RefCell<F>,
}

impl<D, F, S> Dynamic<D, F, S>
where
    F: FnMut(&mut D, &mut S),
{
    pub fn new(mode: UpdateMode, widget: D, state: S, f: F) -> Self {
        Self {
            mode,
            widget: RefCell::new(widget),
            state: RefCell::new(state),
            f: RefCell::new(f),
        }
    }

    pub fn state(&self) -> Ref<'_, S> {
        self.state.borrow()
    }

    pub fn state_mut(&self) -> RefMut<'_, S> {
        self.state.borrow_mut()
    }

    pub fn with_state(&self, f: impl FnOnce(&mut S)) {
        let mut state = self.state.borrow_mut();
        f(&mut state)
    }
}

impl<T, F, S, C, O> Drawable for Dynamic<T, F, S>
where
    T: Drawable<Color = C, Output = O>,
    C: PixelColor,
    F: FnMut(&mut T, &mut S),
{
    type Color = <T as Drawable>::Color;

    type Output = <T as Drawable>::Output;

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut drawable = self.widget.borrow_mut();
        let mut f = self.f.borrow_mut();
        let mut state = self.state.borrow_mut();

        match self.mode {
            UpdateMode::Before => {
                f(&mut drawable, &mut state);
                drawable.draw(target)
            }
            UpdateMode::After => {
                let ret = drawable.draw(target);
                f(&mut drawable, &mut state);
                ret
            }
        }
    }
}

pub struct DrawableIter<I, C>(pub I, PhantomData<C>);

impl<I, C> DrawableIter<I, C> {
    pub const fn new(iter: I) -> Self {
        Self(iter, PhantomData)
    }
}

impl<I, C> Drawable for DrawableIter<I, C>
where
    I: PixelIteratorExt<C> + Clone,
    C: PixelColor,
{
    type Color = C;

    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.0.clone().draw(target)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum UpdateMode {
    Before,
    After,
}

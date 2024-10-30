use alloc::{collections::vec_deque::VecDeque, sync::Arc};
use bitflags::bitflags;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex as CsRawMutex, mutex::Mutex, signal::Signal,
};
use embassy_time::Instant;
use wasmi::{Store, StoreContextMut, TypedFunc};

use super::Env;

pub type WakerFunc = TypedFunc<u32, ()>;

#[derive(Clone, Default)]
pub struct RegistrationQueue(Arc<Inner>);

impl RegistrationQueue {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            queue: Mutex::new(VecDeque::new()),
            barrier: Signal::new(),
        }))
    }

    pub async fn push(&self, registration: Registration) {
        self.0.queue.lock().await.push_back(registration);
        self.0.barrier.signal(());
    }

    pub async fn try_pop(&self) -> Option<Registration> {
        self.0.queue.lock().await.pop_front()
    }

    pub async fn wait(&self) {
        self.0.barrier.wait().await;
    }
}

#[derive(Default)]
struct Inner {
    queue: Mutex<CsRawMutex, VecDeque<Registration>>,
    barrier: Signal<CsRawMutex, ()>,
}

#[derive(Copy, Clone, Debug)]
pub struct Registration {
    kind: RegistrationKind,
    pub(crate) data: u32,
    pub(crate) wake: WakerFunc,
}

impl Registration {
    pub fn wake<'a>(&self, ctx: impl Into<StoreContextMut<'a, Env>>) -> Result<(), wasmi::Error> {
        self.wake.call(ctx.into(), self.data)
    }

    pub fn new_timer(deadline: Instant, data: u32, wake: WakerFunc) -> Self {
        Self::new_internal(RegistrationKind::Timer(deadline), data, wake)
    }

    pub fn new_io(id: i32, interest: Interest, data: u32, wake: WakerFunc) -> Self {
        Self::new_internal(RegistrationKind::Io { id, interest }, data, wake)
    }

    fn new_internal(kind: RegistrationKind, data: u32, wake: WakerFunc) -> Self {
        Self { kind, data, wake }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub enum RegistrationKind {
    Timer(Instant),
    Io { id: i32, interest: Interest },
}

bitflags! {
    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
    pub struct Interest: u8 {
        const READ = 0x1;
        const WRITE = 0x2;
    }
}

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::RangeBounds;
use core::sync::atomic::{self, Ordering};
use critical_section as cs;
use embassy_executor::{SendSpawner, SpawnToken};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as CsRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use embassy_time::Timer;
use esp_hal::rng::Trng;
use esp_hal::Cpu;
use wasmi::core::ValType;
use wasmi::{
    Config, Engine, Extern, FuncRef, Instance, Linker, Memory, Module, Store, StoreContext,
    StoreContextMut, StoreLimits, StoreLimitsBuilder, Table, TypedResumableCall,
};

use super::error::{Error, Result};
use super::{PollRequest, Registration, RegistrationQueue, WakerFunc};

macro_rules! link_syscalls {
    (
        $(($f:path, $name:literal)),* $(,)? ; $linker:expr
    ) => {
        $(
            {
                let link_result = $linker.func_wrap(SYSCALL_NAMESPACE, $name, $f);

                if let Err(e) = link_result {
                    return Err(wasmi::Error::from(e))
                }
            }
        )*
    }
}

const SYSCALL_NAMESPACE: &str = "__xenon_syscall";
const ENTRY_POINT: &str = "__xenon_start";
const MEMORY_NAME: &str = "memory";
const FUNCTION_TABLE_NAME: &str = "__indirect_function_table";
const WASM_MEMORY_LIMIT: usize = 1 << 20; // 1 MiB

fn link_syscalls(linker: &mut Linker<Env>) -> Result<()> {
    use crate::app::syscall::*;

    link_syscalls![
        (stdio::print, "print"),
        (stdio::print, "eprint"),
        (stdio::log, "log"),
        (time::get_time, "get_time"),
        (widget::draw_arc, "draw_arc"),
        (widget::draw_circle, "draw_circle"),
        (widget::draw_ellipse, "draw_ellipse"),
        (widget::draw_line, "draw_line"),
        (widget::draw_rectangle, "draw_rectangle"),
        (widget::draw_rounded_rectangle, "draw_rounded_rectangle"),
        (widget::draw_sector, "draw_sector"),
        (widget::draw_triangle, "draw_triangle"),
        (widget::load_compressed_bitmap, "load_compressed_bitmap"),
        (widget::load_bitmap, "load_bitmap"),
        (widget::decompress_bitmap, "decompress_bitmap"),
        (widget::draw_compressed_bitmap, "draw_compressed_bitmap"),
        (widget::draw_bitmap, "draw_bitmap"),
        (widget::get_bitmap_pixel, "get_bitmap_pixel"),
        (widget::set_bitmap_pixel, "set_bitmap_pixel"),
        (misc::clear_buffer, "clear_buffer"),
        (misc::clone_binary_data, "clone_binary_data"),
        (misc::drop_binary_data, "drop_binary_data"),
        (asynch::wait, "wait"),
        (asynch::poll, "poll"),
        (io::schedule_timer, "schedule_timer"),
        (io::schedule_io, "schedule_io"),
        (panic::panic, "panic");
        linker
    ];

    Ok(())
}

pub struct Executor {
    instance: Instance,
    store: Store<Env>,
}

impl Executor {
    pub fn new(rng: Trng<'static>, spawner: SendSpawner, module: &[u8]) -> Result<Self> {
        let mut config = Config::default();
        config.wasm_multi_value(false);

        let engine = Engine::new(&config);
        let module = Module::new(&engine, module)?;

        let limits = Limits {
            store: StoreLimitsBuilder::new()
                .memories(1)
                .memory_size(WASM_MEMORY_LIMIT)
                .build(),
        };

        let mut store = Store::new(&engine, Env::new(rng, spawner, limits));
        store.limiter(|env| &mut env.limits.store);

        let mut linker = Linker::new(&engine);
        link_syscalls(&mut linker)?;

        let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

        let memory = instance
            .get_memory(&store, MEMORY_NAME)
            .ok_or(Error::NoMemory)?;

        let function_table = instance
            .get_export(&store, FUNCTION_TABLE_NAME)
            .and_then(Extern::into_table)
            .ok_or(Error::NoFunctionTable)?;

        {
            let mut env_data = store.data().lock_data_blocking();
            env_data.set_memory(memory);
            env_data.set_funcs(&store, function_table)?;
        }

        Ok(Self { instance, store })
    }

    pub async fn run(&mut self) -> Result<()> {
        let env = self.store.data().clone();

        let entry = self
            .instance
            .get_typed_func::<(), ()>(&self.store, ENTRY_POINT)?;

        let mut entry_handle = entry.call_resumable(&mut self.store, ())?;

        while let TypedResumableCall::Resumable(resumable) = entry_handle {
            let Some(&request) = resumable.host_error().downcast_ref::<PollRequest>() else {
                // Since wasmi guarantees that resumable.host_error() will never be a Wasm trap, and
                // the only other error type returned by host calls is `Error`, the downcast should
                // unconditionally return Some(_).
                let &host_error = resumable.host_error().downcast_ref::<Error>().unwrap();
                return Err(host_error.into());
            };

            match request {
                PollRequest::Wait => {
                    log::trace!(target: "Wasm executor", "waiting for a task to wake up");
                    self.poll_wakers(&env).await?;
                }
                PollRequest::Poll => {
                    self.poll_wakers(&env).await?;
                }
            }

            entry_handle = resumable.resume(&mut self.store, &[])?;
        }

        Ok(())
    }

    async fn poll_wakers(&mut self, env: &Env) -> Result<()> {
        log::trace!(target: "Wasm executor", "polling wakers");
        while let Some(registration) = env.registrations.try_pop().await {
            registration.wake(&mut self.store)?;
            log::trace!(target: "Wasm executor", "woke up task at wasm address {:#x}", registration.data)
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct Env {
    data: Arc<Mutex<CsRawMutex, EnvData>>,
    registrations: RegistrationQueue,
    spawner: SendSpawner,
    limits: Limits,
}

impl Env {
    pub fn new(rng: Trng<'static>, spawner: SendSpawner, limits: Limits) -> Self {
        Self {
            data: Arc::new(Mutex::new(EnvData::new(rng))),
            registrations: RegistrationQueue::new(),
            spawner,
            limits,
        }
    }

    pub fn lock_data_blocking(&self) -> MutexGuard<'_, CsRawMutex, EnvData> {
        loop {
            match self.data.try_lock() {
                Ok(guard) => break guard,
                Err(_) => core::hint::spin_loop(),
            }
        }
    }

    pub async fn lock_data(&self) -> MutexGuard<'_, CsRawMutex, EnvData> {
        self.data.lock().await
    }

    pub async fn push_registration(&self, registration: Registration) {
        self.registrations.push(registration).await;
    }

    pub fn spawn<S: Send>(&self, token: SpawnToken<S>) -> Result<()> {
        self.spawner
            .spawn(token)
            .map_err(|_| Error::TooManyTasks.into())
    }
}

pub struct EnvData {
    rng: Trng<'static>,
    binary_data: BinaryData,
    funcs: Option<Table>,
    memory: Option<Memory>,
    notified: bool,
}

impl EnvData {
    fn new(rng: Trng<'static>) -> Self {
        Self {
            rng,
            binary_data: BinaryData::new(),
            funcs: None,
            memory: None,
            notified: false,
        }
    }

    pub fn set_memory(&mut self, memory: Memory) {
        self.memory = Some(memory)
    }

    pub fn memory(&self) -> Memory {
        self.memory.expect("env memory was not set")
    }

    pub fn set_funcs<'a>(
        &mut self,
        ctx: impl Into<StoreContext<'a, Env>>,
        table: Table,
    ) -> Result<()> {
        if matches!(table.ty(ctx.into()).element(), ValType::FuncRef) {
            self.funcs = Some(table);
            Ok(())
        } else {
            Err(Error::NoFunctionTable.into())
        }
    }

    pub fn get_func<'a>(&self, ctx: impl Into<StoreContext<'a, Env>>, index: u32) -> FuncRef {
        // Self::set_funcs verifies that the self.funcs is a funcref table, so calling val.funcref()
        // .unwrap() should never panic unless self.funcs is set through other means.
        self.funcs
            .expect("env function table was not set")
            .get(ctx.into(), index)
            .map(|val| *val.funcref().unwrap())
            .unwrap_or_else(FuncRef::null)
    }

    pub fn memory_range<'a, T>(
        &self,
        ctx: impl Into<StoreContext<'a, Env>>,
        range: impl RangeBounds<usize>,
    ) -> Option<&'a [u8]> {
        let ctx = ctx.into();
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();

        self.memory().data(ctx).get((start, end))
    }

    pub fn memory_range_mut<'a>(
        &self,
        ctx: impl Into<StoreContextMut<'a, Env>>,
        range: impl RangeBounds<usize>,
    ) -> Option<&'a mut [u8]> {
        let ctx = ctx.into();
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();

        self.memory().data_mut(ctx).get_mut((start, end))
    }

    pub fn push_binary_data(&mut self, data: impl AsRef<[u8]>) -> usize {
        self.binary_data.push(data)
    }

    pub fn remove_binary_data(&mut self, index: usize) -> Option<Vec<u8>> {
        self.binary_data.remove(index)
    }

    pub fn get_binary_data(&self, index: usize) -> Option<&[u8]> {
        self.binary_data.get(index)
    }

    pub fn get_binary_data_mut(&mut self, index: usize) -> Option<&mut Vec<u8>> {
        self.binary_data.get_mut(index)
    }

    pub fn random_32(&mut self) -> u32 {
        self.rng.random()
    }

    pub fn random_64(&mut self) -> u64 {
        let mut buf = [0u8; size_of::<u64>()];
        self.rng.read(&mut buf);
        u64::from_ne_bytes(buf)
    }

    pub fn notified(&self) -> bool {
        self.notified
    }

    pub fn set_notified(&mut self, notified: bool) {
        self.notified = notified;
    }

    pub fn random_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.read(bytes)
    }
}

// TODO: implement some sort of "generation" system (as is commonly used in ECSs) to have an extra
// check against accidentally freeing data twice if something goes wrong in wasm-land (e.g. a
// double-free bug in the wasm binary).
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct BinaryData {
    free_indices: Vec<usize>,
    data: Vec<Option<Vec<u8>>>,
}

impl BinaryData {
    const fn new() -> Self {
        Self {
            free_indices: Vec::new(),
            data: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn size(&self) -> usize {
        self.data
            .iter()
            .filter_map(|opt| opt.as_ref())
            .map(|vec| vec.len())
            .sum()
    }

    fn get(&self, index: usize) -> Option<&[u8]> {
        self.data.get(index).and_then(|vec| vec.as_deref())
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut Vec<u8>> {
        self.data.get_mut(index).and_then(|vec| vec.as_mut())
    }

    fn push(&mut self, data: impl AsRef<[u8]>) -> usize {
        self.push_internal(data.as_ref())
    }

    fn remove(&mut self, index: usize) -> Option<Vec<u8>> {
        self.data.get_mut(index).and_then(|slot| {
            let vec = slot.take();

            if vec.is_some() {
                self.free_indices.push(index);
            }

            vec
        })
    }

    fn push_internal(&mut self, bytes: &[u8]) -> usize {
        let bytes = bytes.to_vec();

        match self.free_indices.pop() {
            Some(index) => {
                // This shouldn't panic, because the only time popping from free_indices is
                // Some(index) is when that index has previously been used and has been freed.
                let slot = &mut self.data[index];
                *slot = Some(bytes);
                index
            }
            None => {
                let index = self.data.len();
                self.data.push(Some(bytes));
                index
            }
        }
    }
}

#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct Limits {
    pub store: StoreLimits,
}

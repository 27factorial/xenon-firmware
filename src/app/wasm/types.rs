use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::RangeBounds;
use core::result;
use embassy_executor::{SendSpawner, SpawnToken};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as CsRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use esp_hal::rng::Trng;
use thiserror::Error;
use wasmi::core::HostError;
use wasmi::{
    Config, Engine, Instance, Linker, Memory, Module, Store, StoreContext, StoreContextMut,
    StoreLimits, StoreLimitsBuilder,
};

const SYSCALL_NAMESPACE: &str = "__xenon_syscall";
const ENTRY_POINT: &str = "__xenon_start";
const WASM_MEMORY_LIMIT: usize = 1024 * 1024; // 1 MiB

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

fn link_syscalls(linker: &mut Linker<Env>) -> Result<()> {
    use crate::app::wasm::syscall::*;

    link_syscalls![
        (stdio::print, "print"),
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
        (misc::drop_binary_data, "drop_binary_data");
        linker
    ];

    Ok(())
}

pub type Result<T> = result::Result<T, wasmi::Error>;

pub type MutexEnv = Mutex<CsRawMutex, Env>;

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

        let env = Env::new(rng, spawner, limits);

        let mut store = Store::new(&engine, env);
        store.limiter(|env| &mut env.limits.store);

        let mut linker = Linker::new(&engine);
        link_syscalls(&mut linker)?;

        let instance = linker.instantiate(&mut store, &module)?.start(&mut store)?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or(Error::NoMemory)?;

        store.data().lock().set_memory(memory);

        Ok(Self { instance, store })
    }

    pub fn run(&mut self) -> Result<()> {
        let entry = self
            .instance
            .get_typed_func::<(), ()>(&self.store, ENTRY_POINT)?;

        entry.call(&mut self.store, ())?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct Env {
    data: Arc<Mutex<CsRawMutex, EnvData>>,
    limits: Limits,
}

impl Env {
    pub fn new(rng: Trng<'static>, spawner: SendSpawner, limits: Limits) -> Self {
        Self {
            data: Arc::new(Mutex::new(EnvData::new(rng, spawner))),
            limits,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, CsRawMutex, EnvData> {
        loop {
            match self.data.try_lock() {
                Ok(guard) => break guard,
                Err(_) => core::hint::spin_loop(),
            }
        }
    }

    pub async fn lock_async(&self) -> MutexGuard<'_, CsRawMutex, EnvData> {
        self.data.lock().await
    }
}

pub struct EnvData {
    rng: Trng<'static>,
    spawner: SendSpawner,
    binary_data: BinaryData,
    memory: Option<Memory>,
}

impl EnvData {
    pub fn new(rng: Trng<'static>, spawner: SendSpawner) -> Self {
        Self {
            spawner,
            rng,
            binary_data: BinaryData::new(),
            memory: None,
        }
    }

    pub fn set_memory(&mut self, memory: Memory) {
        self.memory = Some(memory)
    }

    pub fn memory(&self) -> Memory {
        self.memory.unwrap()
    }

    pub fn memory_range<'a>(
        &self,
        ctx: impl Into<StoreContext<'a, Self>>,
        range: impl RangeBounds<usize>,
    ) -> Option<&'a [u8]> {
        let ctx = ctx.into();
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();

        self.memory().data(ctx).get((start, end))
    }

    pub fn memory_range_mut<'a>(
        &self,
        ctx: impl Into<StoreContextMut<'a, Self>>,
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

    pub fn random_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.read(bytes)
    }

    pub fn spawn<S: Send>(&self, token: SpawnToken<S>) -> Result<()> {
        self.spawner
            .spawn(token)
            .map_err(|_| Error::TooManyTasks.into())
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

#[repr(u8)]
#[derive(Debug, Error)]
pub enum Error {
    #[error("wasm module did not export any linear memory")]
    NoMemory,
    #[error("invalid value for type {0}")]
    InvalidValue(&'static str),
    #[error(
        "memory range [{}, {}) is invalid UTF-8 (valid up to index {} in range)", 
        start, start + len, valid_up_to,
    )]
    InvalidUtf8 {
        start: usize,
        len: usize,
        valid_up_to: usize,
    },
    #[error("invalid memory range [{}, {})", start, start + end)]
    InvalidMemoryRange { start: usize, end: usize },
    #[error("wasm function `{0}` not found")]
    FunctionNotFound(String),
    #[error("invalid log level {0}")]
    InvalidLogLevel(u32),
    #[error("invalid data id {0}")]
    InvalidId(i32),
    #[error("attempted to spawn too many tasks")]
    TooManyTasks,
    #[error("module panicked")]
    Panicked,
}

impl From<Error> for wasmi::Error {
    fn from(value: Error) -> Self {
        wasmi::Error::host(value)
    }
}

impl HostError for Error {}

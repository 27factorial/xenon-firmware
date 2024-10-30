use crate::app::types::{Env, Error};
use crate::macros::syscall;
use wasmi::Caller;

#[syscall]
pub extern "wasm" fn random_32(caller: Caller<'_, Env>) -> Result<u32, wasmi::Error> {
    let mut env = caller.data().lock_data_blocking();
    Ok(env.random_32())
}

#[syscall]
pub extern "wasm" fn random_64(caller: Caller<'_, Env>) -> Result<u64, wasmi::Error> {
    let mut env = caller.data().lock_data_blocking();
    Ok(env.random_64())
}

#[syscall]
pub extern "wasm" fn random_bytes(
    mut caller: Caller<'_, Env>,
    ptr: usize,
    len: usize,
) -> Result<(), wasmi::Error> {
    let memory = caller.data().lock_data_blocking().memory();
    let end = ptr + len;
    let (memory_data, store) = memory.data_and_store_mut(&mut caller);
    let mut env = store.lock_data_blocking();

    let bytes = memory_data
        .get_mut(ptr..end)
        .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

    env.random_bytes(bytes);

    Ok(())
}

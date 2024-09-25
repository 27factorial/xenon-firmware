use crate::app::types::Error;
use crate::macros::syscalls;

syscalls! {
    pub extern "wasm" fn random_32(caller) -> Result<u32, wasmi::Error> {
        let mut env = caller.data().lock_sync();
        Ok(env.random_32())
    }

    pub extern "wasm" fn random_64(caller) -> Result<u64, wasmi::Error> {
        let mut env = caller.data().lock_sync();
        Ok(env.random_64())
    }

    pub extern "wasm" fn random_bytes(
        caller,
        ptr: usize,
        len: usize,
    ) -> Result<(), wasmi::Error> {
        let memory = caller.data().lock_sync().memory();
        let end = ptr + len;
        let (memory_data, store) = memory.data_and_store_mut(&mut caller);
        let mut env = store.lock_sync();

        let bytes = memory_data
            .get_mut(ptr..end)
            .ok_or(Error::InvalidMemoryRange { start: ptr, end })?;

        env.random_bytes(bytes);

        Ok(())
    }
}

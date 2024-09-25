use crate::app::wasm::types::Error;
use crate::driver::lcd::LCD_BUFFER;
use crate::macros::syscalls;
use embassy_executor::task;

#[task]
async fn clear_internal() {
    LCD_BUFFER.lock().await.clear();
}

syscalls! {
    pub extern "wasm" fn clear_buffer(
        caller
    ) -> Result<(), wasmi::Error> {
        caller.data().lock_sync().spawn(clear_internal())
    }

    pub extern "wasm" fn clone_binary_data(
        caller,
        id: i32,
    ) -> Result<i32, wasmi::Error> {
        let mut env = caller.data().lock_sync();
        let data = usize::try_from(id)
            .map_err(|_| Error::InvalidId(id))
            .and_then(|index| env.get_binary_data(index).ok_or(Error::InvalidId(id)))?
            .to_vec();

        let index = env.push_binary_data(data);

        Ok(index as i32)
    }

    pub extern "wasm" fn drop_binary_data(
        caller,
        id: i32,
    ) -> Result<(), wasmi::Error> {
        let mut env = caller.data().lock_sync();
        let index = usize::try_from(id)
            .map_err(|_| Error::InvalidId(id))?;

        env.remove_binary_data(index).ok_or(Error::InvalidId(id))?;

        Ok(())
    }
}

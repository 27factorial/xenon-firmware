use crate::{app::wasm::types::AsyncEvent, macros::syscalls};

syscalls! {
    pub extern "wasm" fn cs_acquire(caller) -> Result<(), wasmi::Error> {
        let mut env_data = caller.data().lock_sync();

        env_data.push_critical_section();

        Ok(())
    }

    pub extern "wasm" fn cs_release(caller) -> Result<(), wasmi::Error> {
        let mut env_data = caller.data().lock_sync();
        env_data.pop_critical_section()
    }

    pub extern "wasm" fn wait(caller) -> Result<(), wasmi::Error> {
        let mut env_data = caller.data().lock_sync();

        // After calling `resume` on the resumable Wasm function, wasmi will resume here. The 
        // `notified` flag is used to ensure we don't get into an infinite loop of returning 
        // `AsyncEvent::Wait`.
        if env_data.is_notified() {
            env_data.set_notified(false);
            Ok(())
        } else {
            Err(AsyncEvent::Wait.into())
        }        
    }
}

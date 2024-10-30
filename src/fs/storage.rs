use embedded_storage::nor_flash::{
    ErrorType as NorFlashErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash,
};
use embedded_storage_async::nor_flash::{
    MultiwriteNorFlash as AsyncMultiwriteNorFlash, NorFlash as AsyncNorFlash,
    ReadNorFlash as AsyncReadNorFlash,
};
use esp_storage::FlashStorage as EspFlashStorage;

// this type just implements the embedded_storage_async NOR flash traits by calling the blocking
// version. esp_storage doesn't really provide a better api and the async variant is required by
// sequential_storage.
#[derive(Debug, Default)]
pub struct Storage(EspFlashStorage);

impl NorFlashErrorType for Storage {
    type Error = <EspFlashStorage as NorFlashErrorType>::Error;
}

impl ReadNorFlash for Storage {
    const READ_SIZE: usize = <EspFlashStorage as ReadNorFlash>::READ_SIZE;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.0.read(offset, bytes)
    }

    fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl AsyncReadNorFlash for Storage {
    const READ_SIZE: usize = <Self as ReadNorFlash>::READ_SIZE;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        ReadNorFlash::read(self, offset, bytes)
    }

    fn capacity(&self) -> usize {
        ReadNorFlash::capacity(self)
    }
}

impl NorFlash for Storage {
    const WRITE_SIZE: usize = <EspFlashStorage as NorFlash>::WRITE_SIZE;

    const ERASE_SIZE: usize = <EspFlashStorage as NorFlash>::ERASE_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.0.erase(from, to)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0.write(offset, bytes)
    }
}

impl AsyncNorFlash for Storage {
    const WRITE_SIZE: usize = <Self as NorFlash>::WRITE_SIZE;

    const ERASE_SIZE: usize = <Self as NorFlash>::ERASE_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        NorFlash::erase(self, from, to)
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        NorFlash::write(self, offset, bytes)
    }
}

// Technically the internal FlashStorage doesn't guarantee the behavior required by this trait, but
// in practice and testing it does on my current board.
impl MultiwriteNorFlash for Storage {}

impl AsyncMultiwriteNorFlash for Storage {}

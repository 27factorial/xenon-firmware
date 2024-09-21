use alloc::sync::Arc;
use alloc::vec; // `vec!` macro, not the module. rust-analyzer gets this wrong.
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::convert::Infallible;
use core::fmt::{self, Debug};
use core::hint::spin_loop;
use core::ops::{Deref, Range};
use ekv::flash::{Flash, PageID};
use ekv::{CommitError, Config, Error as EkvError, FormatError, MountError, ReadError, WriteError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::once_lock::OnceLock;
use embedded_io::{
    Error as IoError, ErrorKind as IoErrorKind, ErrorType as IoErrorType, Read, Seek, SeekFrom,
};
use embedded_io_async::{Read as AsyncRead, Seek as AsyncSeek, Write as AsyncWrite};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_hal::rng::Rng;
use esp_hal::sha::{Sha, Sha256};
use esp_hal::Blocking;
use esp_storage::{FlashStorage as EspFlashStorage, FlashStorageError as EspFlashStorageError};
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FS_START: u32 = 0x00110000;
pub const FS_SIZE: u32 = 0x006f0000;
pub const FS_PAGE_SIZE: u32 = EspFlashStorage::SECTOR_SIZE;
pub const FS_PAGES: u32 = FS_SIZE / FS_PAGE_SIZE;
pub const FILE_KEY_SIZE: usize = ekv::config::MAX_KEY_SIZE;

pub static FILESYSTEM: GlobalFilesystem = GlobalFilesystem::new();

type Database = ekv::Database<Storage, CriticalSectionRawMutex>;
type Mutex<T> = embassy_sync::mutex::Mutex<CriticalSectionRawMutex, T>;

#[inline(always)]
const fn page_id_to_range(id: PageID) -> Range<u32> {
    let index = id.index() as u32;
    let start = FS_START + (index * FS_PAGE_SIZE);
    let end = start + FS_PAGE_SIZE;
    start..end
}

fn sha256(s: &str) -> [u8; FILE_KEY_SIZE] {
    #[inline(always)]
    fn wait(sha: &Sha256<Blocking>) {
        while sha.is_busy() {
            spin_loop();
        }
    }

    let mut buf = [0; FILE_KEY_SIZE];
    let mut sha = Sha256::new();

    wait(&sha);
    sha.write_data(s.as_bytes()).unwrap();

    wait(&sha);
    sha.process_buffer();

    wait(&sha);
    sha.finish(&mut buf).unwrap();

    buf
}

pub struct GlobalFilesystem(OnceLock<Filesystem>);

impl GlobalFilesystem {
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    pub fn init(&self, fs: Filesystem) {
        if self.0.init(fs).is_err() {
            panic!("attempted to initialize GlobalFilesystem twice.")
        };
    }
}

impl Deref for GlobalFilesystem {
    type Target = Filesystem;

    fn deref(&self) -> &Self::Target {
        match self.0.try_get() {
            Some(fs) => fs,
            None => panic!(
                "global filesystem was not initialized; \
                 call `GlobalFilesystem::init` to initialize it first before using it."
            ),
        }
    }
}

impl Default for GlobalFilesystem {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Filesystem {
    db: Arc<Database>,
    rng: Mutex<Rng>,
}

impl Filesystem {
    pub async fn new(storage: EspFlashStorage, mut rng: Rng) -> Result<Self, Error> {
        let storage = Storage(storage);
        let mut config = Config::default();
        config.random_seed = rng.random();

        let db = Database::new(storage, config);

        if let Err(mount_err) = db.mount().await {
            match mount_err {
                MountError::Corrupted => {
                    log::warn!(
                        "No filesystem found, formatting {FS_SIZE} bytes at address {FS_START:#x}",
                    );
                    db.format().await?;
                }
                MountError::Flash(e) => return Err(Error::Flash(e)),
            }
        }

        Ok(Self {
            db: Arc::new(db),
            rng: Mutex::new(rng),
        })
    }

    pub async fn open_file(&self, name: &str) -> Result<File, Error> {
        let key = sha256(name);
        let mut file_meta_bytes = [0u8; FileMeta::POSTCARD_MAX_SIZE];

        self.db
            .read_transaction()
            .await
            .read(&key, &mut file_meta_bytes)
            .await?;

        let file_meta = postcard::from_bytes::<FileMeta>(&file_meta_bytes)?;

        Ok(File {
            entry_key: key,
            data_key: file_meta.key,
            size: file_meta.size,
            cursor: 0,
            data: None,
            db: Arc::clone(&self.db),
        })
    }

    pub async fn create_file(&mut self, name: &str) -> Result<File, Error> {
        let entry_key = sha256(name);

        if !self.key_exists(entry_key).await? {
            let mut data_key = [0u8; FILE_KEY_SIZE];
            self.rng.lock().await.read(&mut data_key);

            Ok(File {
                entry_key,
                data_key,
                size: 0,
                cursor: 0,
                data: Some(Vec::new()),
                db: self.db.clone(),
            })
        } else {
            Err(Error::AlreadyExists)
        }
    }

    pub async fn format(&self) -> Result<(), Error> {
        self.db.format().await?;
        Ok(())
    }

    pub async fn key_exists(&self, key: [u8; FILE_KEY_SIZE]) -> Result<bool, Error> {
        // Used to check if the key already exists.
        // TODO: Change this abomination to some sort of .exists() function if that ever becomes
        // a thing.
        let read_result = self.db.read_transaction().await.read(&key, &mut []).await;

        match read_result {
            Ok(_) | Err(ReadError::BufferTooSmall) => Ok(true),
            Err(ReadError::KeyNotFound) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn exists(&self, name: &str) -> Result<bool, Error> {
        let entry_key = sha256(name);
        self.key_exists(entry_key).await
    }
}

impl Debug for Filesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Filesystem").finish_non_exhaustive()
    }
}

/// NOTE: If File::commit is not called, all changes will be lost. Files that were just created
/// will also not be able to be opened.
#[derive(Clone)]
pub struct File {
    entry_key: [u8; FILE_KEY_SIZE],
    data_key: [u8; FILE_KEY_SIZE],
    size: usize,
    cursor: usize,
    data: Option<Vec<u8>>,
    db: Arc<Database>,
}

impl File {
    pub async fn open(name: &str) -> Result<Self, Error> {
        FILESYSTEM.open_file(name).await
    }

    pub async fn load(&mut self) -> Result<&mut Vec<u8>, Error> {
        if self.data.is_none() {
            let mut data = vec![0; self.size];

            self.db
                .read_transaction()
                .await
                .read(&self.data_key, &mut data)
                .await?;

            data.shrink_to_fit();
            self.size = data.len();
            self.data = Some(data);
        }

        // It's fine to unwrap here because self.data is always Some(_) after the check above.
        Ok(self.data.as_mut().unwrap())
    }

    pub async fn commit(&self) -> Result<(), Error> {
        if let Some(ref data) = self.data {
            let meta = FileMeta {
                key: self.data_key,
                size: self.size,
            };

            let mut meta_bytes = [0; FileMeta::POSTCARD_MAX_SIZE];
            let meta_bytes = postcard::to_slice(&meta, &mut meta_bytes)
                .expect("slice to have an adequate length");

            // This little dance is required because ekv requires that the keys given to each write
            // are given in lexicographically ascending order. Something tells me that either:
            // A) ekv wasn't intended to be used this way.
            // B) I'm an idiot and there's a far better way to do this.
            // C) Both (<- most likely option).
            let (first_key, first_data, second_key, second_data) =
                match self.entry_key.cmp(&self.data_key) {
                    Ordering::Less => (
                        self.entry_key.as_slice(),
                        data.as_slice(),
                        self.data_key.as_slice(),
                        &*meta_bytes,
                    ),
                    Ordering::Greater => (
                        self.data_key.as_slice(),
                        &*meta_bytes,
                        self.entry_key.as_slice(),
                        data.as_slice(),
                    ),
                    Ordering::Equal => panic!(
                        "entry and data keys must not be identical.\n\
                         This is astronomically unlikely, which means something is probably wrong \
                         with the file data or the implementation of the filesystem."
                    ),
                };

            let mut transaction = self.db.write_transaction().await;

            transaction.write(first_key, first_data).await?;
            transaction.write(second_key, second_data).await?;
            transaction.commit().await?;
        }

        Ok(())
    }
}

impl Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("File")
            .field("entry_key", &self.entry_key)
            .field("data_key", &self.data_key)
            .field("size", &self.size)
            .field("cursor", &self.cursor)
            .field("data", &self.data)
            .finish()
    }
}

impl IoErrorType for File {
    type Error = Error;
}

impl AsyncRead for File {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let cursor = self.cursor;
        let data = self.load().await?;

        match data.get(cursor..) {
            Some(mut slice) => {
                let bytes_read = Read::read(&mut slice, buf)?;
                self.cursor += bytes_read;
                Ok(bytes_read)
            }
            None => Ok(0),
        }
    }
}

impl AsyncWrite for File {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if !buf.is_empty() {
            let data = self.load().await?;

            if data.len() + buf.len() <= ekv::config::MAX_VALUE_SIZE {
                data.extend_from_slice(buf);
                self.size = data.len();

                Ok(buf.len())
            } else {
                Err(Error::DataTooLarge)
            }
        } else {
            Err(Error::WriteZero)
        }
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.commit().await
    }
}

impl Seek for File {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        let max_len = match self.data {
            Some(ref data) => data.len(),
            None => self.size,
        };

        let (base, offset) = match pos {
            SeekFrom::Start(n) => {
                let n = n.min(usize::MAX as u64) as usize;
                self.cursor = n;

                return Ok(self.cursor as u64);
            }
            SeekFrom::End(n) => {
                let n = n.clamp(isize::MIN as i64, isize::MAX as i64) as isize;

                (max_len, n)
            }
            SeekFrom::Current(n) => {
                let n = n.clamp(isize::MIN as i64, isize::MAX as i64) as isize;

                (self.cursor, n)
            }
        };

        match base.checked_add_signed(offset) {
            Some(new_cursor) => {
                self.cursor = new_cursor;
                Ok(self.cursor as u64)
            }
            None => Err(Error::OutOfBounds),
        }
    }
}

impl AsyncSeek for File {
    async fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        Seek::seek(self, pos)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("filesystem corruption detected")]
    Corrupted,
    #[error("key was not found")]
    NotFound,
    #[error("file already exists")]
    AlreadyExists,
    #[error("data was too large")]
    DataTooLarge,
    #[error("operation was canceled")]
    Canceled,
    #[error("database is full")]
    Full,
    #[error("attempt to access out of bounds")]
    OutOfBounds,
    #[error("attempted to write zero bytes to filesystem")]
    WriteZero,
    #[error("deserialization error: {0}")]
    Deserialize(postcard::Error),
    #[error("flash storage error: {0:?}")]
    Flash(EspFlashStorageError),
}

impl IoError for Error {
    fn kind(&self) -> IoErrorKind {
        use EspFlashStorageError as EfsError;

        match self {
            Self::Corrupted => IoErrorKind::InvalidData,
            Self::NotFound => IoErrorKind::NotFound,
            Self::AlreadyExists => IoErrorKind::AlreadyExists,
            Self::DataTooLarge => IoErrorKind::InvalidInput,
            Self::Canceled => IoErrorKind::Interrupted,
            Self::Full => IoErrorKind::OutOfMemory,
            Self::OutOfBounds => IoErrorKind::InvalidInput,
            Self::WriteZero => IoErrorKind::WriteZero,
            Self::Deserialize(_) => IoErrorKind::Other,
            Self::Flash(e) => match e {
                EfsError::IoError => IoErrorKind::Other,
                EfsError::IoTimeout => IoErrorKind::TimedOut,
                EfsError::CantUnlock => IoErrorKind::PermissionDenied,
                EfsError::NotAligned => IoErrorKind::InvalidInput,
                EfsError::OutOfBounds => IoErrorKind::Other,
                EfsError::Other(_) => IoErrorKind::Other,
                _ => unreachable!("flash storage error has a new variant"),
            },
        }
    }
}

impl From<EkvError<EspFlashStorageError>> for Error {
    fn from(value: EkvError<EspFlashStorageError>) -> Self {
        match value {
            EkvError::Corrupted => Error::Corrupted,
            EkvError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<FormatError<EspFlashStorageError>> for Error {
    fn from(value: FormatError<EspFlashStorageError>) -> Self {
        match value {
            FormatError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<MountError<EspFlashStorageError>> for Error {
    fn from(value: MountError<EspFlashStorageError>) -> Self {
        match value {
            MountError::Corrupted => Error::Corrupted,
            MountError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<ReadError<EspFlashStorageError>> for Error {
    fn from(value: ReadError<EspFlashStorageError>) -> Self {
        match value {
            ReadError::KeyNotFound => Error::NotFound,
            ReadError::KeyTooBig => {
                unimplemented!(
                    "file operations shouldalways ensure database keys are correctly sized"
                )
            }
            ReadError::BufferTooSmall => unimplemented!(
                "file operations should always ensure database operations have a correctly sized \
                buffer"
            ),
            ReadError::Corrupted => Error::Corrupted,
            ReadError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<WriteError<EspFlashStorageError>> for Error {
    fn from(value: WriteError<EspFlashStorageError>) -> Self {
        match value {
            WriteError::NotSorted => todo!(),
            WriteError::KeyTooBig => {
                unimplemented!("file operations always ensure database keys are correctly sized")
            }
            WriteError::ValueTooBig => Error::DataTooLarge,
            WriteError::TransactionCanceled => Error::Canceled,
            WriteError::Full => Error::Full,
            WriteError::Corrupted => Error::Corrupted,
            WriteError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<postcard::Error> for Error {
    fn from(value: postcard::Error) -> Self {
        Self::Deserialize(value)
    }
}

impl From<CommitError<EspFlashStorageError>> for Error {
    fn from(value: CommitError<EspFlashStorageError>) -> Self {
        match value {
            CommitError::TransactionCanceled => Error::Canceled,
            CommitError::Corrupted => Error::Corrupted,
            CommitError::Flash(e) => Error::Flash(e),
        }
    }
}

impl From<Infallible> for Error {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, MaxSize)]
struct FileMeta {
    key: [u8; FILE_KEY_SIZE],
    size: usize,
}

struct Storage(EspFlashStorage);

impl Flash for Storage {
    type Error = EspFlashStorageError;

    fn page_count(&self) -> usize {
        FS_PAGES as usize
    }

    async fn erase(&mut self, page_id: PageID) -> Result<(), Self::Error> {
        let range = page_id_to_range(page_id);
        self.0.erase(range.start, range.end)
    }

    async fn read(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        let range = page_id_to_range(page_id);
        let address = range.start + offset as u32;
        self.0.read(address, data)
    }

    async fn write(
        &mut self,
        page_id: PageID,
        offset: usize,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        let range = page_id_to_range(page_id);
        let address = range.start + offset as u32;
        self.0.write(address, data)
    }
}

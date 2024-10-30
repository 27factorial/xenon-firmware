use alloc::collections::btree_set::BTreeSet;
use alloc::sync::Arc;
use alloc::vec; // `vec!` macro, not the module. rust-analyzer gets this wrong.
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::convert::Infallible;
use core::fmt::{self, Debug};
use core::hint::spin_loop;
use core::mem;
use core::ops::{Deref, Range};
use ekv::flash::{Flash, PageID};
use ekv::{CommitError, Config, Error as EkvError, FormatError, MountError, ReadError, WriteError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::once_lock::OnceLock;
use embedded_io::{
    Error as IoError, ErrorKind as IoErrorKind, ErrorType as IoErrorType, Read, Seek, SeekFrom,
};
use embedded_io_async::{Read as AsyncRead, Seek as AsyncSeek, Write as AsyncWrite};
use embedded_storage::nor_flash::{ErrorType, NorFlash, ReadNorFlash};
use esp_hal::rng::Rng;
use esp_hal::sha::{Sha, Sha256};
use esp_hal::Blocking;
use esp_storage::{FlashStorage as EspFlashStorage, FlashStorageError as EspFlashStorageError};
use hashbrown::HashSet;
use heapless::{String as ConstString, Vec as ConstVec};
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FS_START: u32 = 0x00110000;
pub const FS_SIZE: u32 = 0x006f0000;
pub const FS_PAGE_SIZE: u32 = EspFlashStorage::SECTOR_SIZE;
pub const FS_PAGES: u32 = FS_SIZE / FS_PAGE_SIZE;
pub const SHA256_SIZE: usize = 32;
pub const FILE_KEY_SIZE: usize = ekv::config::MAX_KEY_SIZE;
pub const FILE_CHUNK_SIZE: usize = ekv::config::MAX_VALUE_SIZE;
pub const MAX_DIRECTORY_ELEMENTS: usize = 0;
pub const PATH_SEPARATOR: char = '/';

const _: () = const {
    assert!(
        ekv::config::MAX_KEY_SIZE >= SHA256_SIZE,
        "keys in an ekv database must be able to store at least a SHA256 hash"
    );
};

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

fn sha256(s: &str) -> [u8; SHA256_SIZE] {
    #[inline(always)]
    fn wait(sha: &Sha256<Blocking>) {
        while sha.is_busy() {
            spin_loop();
        }
    }

    let mut buf = [0; SHA256_SIZE];
    let mut sha = Sha256::new();

    wait(&sha);
    sha.write_data(s.as_bytes()).unwrap();

    wait(&sha);
    sha.process_buffer();

    wait(&sha);
    sha.finish(&mut buf).unwrap();

    buf
}

fn canonicalize_path(path: &str) -> Result<&str, Error> {
    let bytes = path.as_bytes();

    let path_is_one_byte = bytes.len() == 1;
    let path_is_separator = path_is_one_byte && bytes[0] == PATH_SEPARATOR as u8;

    if bytes.is_empty() || path_is_separator {
        Err(Error::InvalidPath)
    } else if path_is_one_byte {
        // This is a file or directory at the root path with a one character name.
        Ok(path)
    } else {
        {
            // at this point we know the `bytes` slice must have at least 2 elements.
            let mut previous_byte = bytes[0];

            for &byte in &bytes[1..] {
                if previous_byte == PATH_SEPARATOR as u8 && byte == PATH_SEPARATOR as u8 {
                    // The path contained at least two separator characters one after another,
                    // meaning it's invalid.
                    return Err(Error::InvalidPath);
                }

                previous_byte = byte;
            }
        }

        // Turns strings of the form "/path/to/directory/or/file/" into
        // "path/to/directory/or/file". Since the filesystem doesn't have any concept of relative
        // paths, these are identical. The second form is the canonical version that gets used in
        // all file operations internally.
        Ok(path.trim_matches(PATH_SEPARATOR))
    }
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

    pub async fn create_dir(&self, path: &str) -> Result<(), Error> {
        let mut write_txn = self.db.write_transaction().await;
        let path = canonicalize_path(path)?;

        // path is now guaranteed to be a valid path name. Now it should be safe to parse without
        // checking for extraneous separators. Namely, the rsplit below cannot result in
        // `Some(("", ""))` because a single path separator is not a valid path, and would cause
        // an error above. It also cannot be `Some("<example>", "")` because leading and trailing
        // path separators are trimmed from the path in `canonicalize_path`.
        let (parent, current) = match path.rsplit_once(PATH_SEPARATOR) {
            Some((p, c)) => (Some(p), c),
            None => (None, path),
        };

        if let Some(parent) = parent {
            // We have to drop the write txn first because ekv doesn't allow reads from
            // write txns. It's reinitialized after the read.
            //
            // TODO: Remove this if/when ekv allows reads in write transactions.
            drop(write_txn);
            let mut node_bytes = [0; FsNode::POSTCARD_MAX_SIZE];

            let node: FsNode = {
                let read_txn = self.db.read_transaction().await;
                let hashed = sha256(parent);

                read_txn.read(&hashed, &mut node_bytes).await?;
                postcard::from_bytes(&node_bytes)?
            };

            write_txn = self.db.write_transaction().await;
        }

        Ok(())
    }

    pub async fn open_node(&self, path: &str) -> Result<File, Error> {
        let meta_hash = sha256(path);
        let mut file_meta_bytes = [0; FileMeta::POSTCARD_MAX_SIZE];
        let txn = self.db.read_transaction().await;

        txn.read(&meta_hash, &mut file_meta_bytes).await?;

        let file_meta = postcard::from_bytes::<FileMeta>(&file_meta_bytes)?;

        todo!()
    }

    // pub async fn create_file(&mut self, name: &str) -> Result<File, Error> {
    //     let entry_key = sha256(name);

    //     if !self.key_exists(entry_key).await? {
    //         let mut data_key = [0u8; FILE_KEY_SIZE];
    //         self.rng.lock().await.read(&mut data_key);

    //         Ok(File {
    //             entry_key,
    //             data_key,
    //             size: 0,
    //             cursor: 0,
    //             data: Some(Vec::new()),
    //             db: self.db.clone(),
    //         })
    //     } else {
    //         Err(Error::AlreadyExists)
    //     }
    // }

    // pub async fn format(&self) -> Result<(), Error> {
    //     self.db.format().await?;
    //     Ok(())
    // }

    // pub async fn key_exists(&self, key: [u8; FILE_KEY_SIZE]) -> Result<bool, Error> {
    //     // Used to check if the key already exists.
    //     // TODO: Change this abomination to some sort of .exists() function if that ever becomes
    //     // a thing.
    //     let read_result = self.db.read_transaction().await.read(&key, &mut []).await;

    //     match read_result {
    //         Ok(_) | Err(ReadError::BufferTooSmall) => Ok(true),
    //         Err(ReadError::KeyNotFound) => Ok(false),
    //         Err(e) => Err(e.into()),
    //     }
    // }

    // pub async fn exists(&self, name: &str) -> Result<bool, Error> {
    //     let entry_key = sha256(name);
    //     self.key_exists(entry_key).await
    // }
}

impl Debug for Filesystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Filesystem").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, MaxSize)]
enum FsNode {
    File(FileMeta),
    Dir(DirMeta),
}

impl FsNode {
    const _SIZE_CHECK: () = const {
        assert!(Self::POSTCARD_MAX_SIZE <= ekv::config::MAX_VALUE_SIZE);
    };
}

/// NOTE: If File::commit is not called, all changes will be lost. Files that were just created
/// will also not be able to be opened.
#[derive(Clone)]
pub struct File {
    meta_hash: [u8; SHA256_SIZE],
    key: [u8; FILE_KEY_SIZE],
    current_chunk: u16,
    max_chunk: u16,
    data: Vec<u8>,
    db: Arc<Database>,
}

impl File {}

#[derive(Clone, Debug, Serialize, Deserialize, MaxSize)]
struct FileMeta {
    chunks: u16,
    last_chunk_size: u16,
    key: [u8; FILE_KEY_SIZE],
    name: ConstString<255>,
}

pub struct Directory {
    meta_hash: [u8; SHA256_SIZE],
    keys: HashSet<[u8; SHA256_SIZE]>,
    db: Arc<Database>,
}

#[derive(Clone, Debug, Serialize, Deserialize, MaxSize)]
struct DirMeta {
    chunks: u16,
    last_chunk_elems: u16,
    name: ConstString<255>,
}

// impl IoErrorType for File {
//     type Error = Error;
// }

// impl AsyncRead for File {
//     async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
//         let cursor = self.cursor;
//         let data = self.load().await?;

//         match data.get(cursor..) {
//             Some(mut slice) => {
//                 let bytes_read = Read::read(&mut slice, buf)?;
//                 self.cursor += bytes_read;
//                 Ok(bytes_read)
//             }
//             None => Ok(0),
//         }
//     }
// }

// impl AsyncWrite for File {
//     async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
//         if !buf.is_empty() {
//             let data = self.load().await?;

//             if data.len() + buf.len() <= ekv::config::MAX_VALUE_SIZE {
//                 data.extend_from_slice(buf);
//                 self.size = data.len();

//                 Ok(buf.len())
//             } else {
//                 Err(Error::DataTooLarge)
//             }
//         } else {
//             Err(Error::WriteZero)
//         }
//     }

//     async fn flush(&mut self) -> Result<(), Self::Error> {
//         self.commit().await
//     }
// }

// impl Seek for File {
//     fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
//         let max_len = match self.data {
//             Some(ref data) => data.len(),
//             None => self.size,
//         };

//         let (base, offset) = match pos {
//             SeekFrom::Start(n) => {
//                 let n = n.min(usize::MAX as u64) as usize;
//                 self.cursor = n;

//                 return Ok(self.cursor as u64);
//             }
//             SeekFrom::End(n) => {
//                 let n = n.clamp(isize::MIN as i64, isize::MAX as i64) as isize;

//                 (max_len, n)
//             }
//             SeekFrom::Current(n) => {
//                 let n = n.clamp(isize::MIN as i64, isize::MAX as i64) as isize;

//                 (self.cursor, n)
//             }
//         };

//         match base.checked_add_signed(offset) {
//             Some(new_cursor) => {
//                 self.cursor = new_cursor;
//                 Ok(self.cursor as u64)
//             }
//             None => Err(Error::OutOfBounds),
//         }
//     }
// }

// impl AsyncSeek for File {
//     async fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
//         Seek::seek(self, pos)
//     }
// }

#[derive(Debug, Error)]
pub enum Error {
    #[error("filesystem corruption detected")]
    Corrupted,
    #[error("key was not found")]
    NotFound,
    #[error("file already exists")]
    AlreadyExists,
    #[error("path was invalid")]
    InvalidPath,
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
            Self::InvalidPath => IoErrorKind::InvalidInput,
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

use embedded_storage::nor_flash::{ErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash};
use esp_storage::FlashStorage as EspFlashStorage;

pub struct Storage(EspFlashStorage);

impl ErrorType for Storage {
    type Error = <EspFlashStorage as ErrorType>::Error;
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

// Technically the internal FlashStorage doesn't guarantee the behavior required by this trait, but
// in practice and testing it does on my current board.
impl MultiwriteNorFlash for Storage {

}

// pub mod file;
// pub(crate) mod node;
// mod storage;

// use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
// use core::{
//     fmt,
//     hint::spin_loop,
//     mem::size_of,
//     ops::{Deref, DerefMut, Range},
//     str::{self, FromStr},
// };
// use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
// use embedded_io::{
//     Error as IoError, ErrorKind as IoErrorKind, ErrorType as IoErrorType, Read, Seek, SeekFrom, Write
// };
// use embedded_io_async::{Read as AsyncRead, Seek as AsyncSeek, Write as AsyncWrite};
// use esp_hal::{
//     rng::Rng,
//     sha::{Sha, Sha256},
//     Blocking,
// };
// use esp_storage::{FlashStorage as EspFlashStorage, FlashStorageError as EspFlashStorageError};
// use heapless::String as FixedString;
// use heapless::Vec as FixedVec;
// use node::{Chunk, Metadata, Node, CHUNK_DATA_SIZE, MAX_NAME_BYTES};
// use postcard::experimental::max_size::MaxSize;
// use sequential_storage::map;
// use sequential_storage::Error as SeqStorageError;
// use sequential_storage::{
//     cache::KeyPointerCache,
//     map::{Key as MapKey, SerializationError as MapSerError},
// };
// use serde::{Deserialize, Serialize};
// use storage::Storage;
// use thiserror::Error;

// pub const FS_START: u32 = 0x00110000;
// pub const FS_SIZE: u32 = 0x006f0000;
// pub const FS_RANGE: Range<u32> = FS_START..FS_START + FS_SIZE;
// pub const FS_PAGES: usize = FS_SIZE as usize / EspFlashStorage::SECTOR_SIZE as usize;
// pub const FS_CACHE_KEYS: usize = 32;
// pub const KEY_BYTES: usize = 32;
// pub const CHUNK_ID_BYTES: usize = KEY_BYTES - size_of::<u16>();
// const U16_BYTES: usize = size_of::<u16>();

// type Cache = KeyPointerCache<FS_PAGES, Key, FS_CACHE_KEYS>;

// fn postcard_error_to_i32(error: postcard::Error) -> i32 {
//     use postcard::Error as PcError;

//     match error {
//         PcError::WontImplement => 0,
//         PcError::NotYetImplemented => 1,
//         PcError::SerializeBufferFull => 2,
//         PcError::SerializeSeqLengthUnknown => 3,
//         PcError::DeserializeUnexpectedEnd => 4,
//         PcError::DeserializeBadVarint => 5,
//         PcError::DeserializeBadBool => 6,
//         PcError::DeserializeBadChar => 7,
//         PcError::DeserializeBadUtf8 => 8,
//         PcError::DeserializeBadOption => 9,
//         PcError::DeserializeBadEnum => 10,
//         PcError::DeserializeBadEncoding => 11,
//         PcError::DeserializeBadCrc => 12,
//         PcError::SerdeSerCustom => 13,
//         PcError::SerdeDeCustom => 14,
//         PcError::CollectStrError => 15,
//         e => unimplemented!("unknown postcard error {e:?}"),
//     }
// }

// fn i32_to_postcard_error(error: i32) -> postcard::Error {
//     use postcard::Error as PcError;

//     match error {
//         0 => PcError::WontImplement,
//         1 => PcError::NotYetImplemented,
//         2 => PcError::SerializeBufferFull,
//         3 => PcError::SerializeSeqLengthUnknown,
//         4 => PcError::DeserializeUnexpectedEnd,
//         5 => PcError::DeserializeBadVarint,
//         6 => PcError::DeserializeBadBool,
//         7 => PcError::DeserializeBadChar,
//         8 => PcError::DeserializeBadUtf8,
//         9 => PcError::DeserializeBadOption,
//         10 => PcError::DeserializeBadEnum,
//         11 => PcError::DeserializeBadEncoding,
//         12 => PcError::DeserializeBadCrc,
//         13 => PcError::SerdeSerCustom,
//         14 => PcError::SerdeDeCustom,
//         15 => PcError::CollectStrError,
//         int => unimplemented!("invalid integer {int} for postcard error"),
//     }
// }

// fn check_name(name: &str) -> Result<(), Error> {
//     if name.len() <= MAX_NAME_BYTES {
//         Ok(())
//     } else {
//         Err(Error::FilenameTooLong)
//     }
// }

// fn sha256(bytes: impl AsRef<[u8]>) -> [u8; KEY_BYTES] {
//     fn sha256_internal(bytes: &[u8]) -> [u8; KEY_BYTES] {
//         #[inline(always)]
//         fn wait(sha: &Sha256<Blocking>) {
//             while sha.is_busy() {
//                 spin_loop();
//             }
//         }

//         let mut buf = [0; KEY_BYTES];
//         let mut sha = Sha256::new();

//         wait(&sha);
//         sha.write_data(bytes).unwrap();

//         wait(&sha);
//         sha.process_buffer();

//         wait(&sha);
//         sha.finish(&mut buf).unwrap();

//         buf
//     }

//     sha256_internal(bytes.as_ref())
// }

// #[derive(Clone)]
// pub struct Filesystem(Arc<Mutex<CriticalSectionRawMutex, Inner>>);

// impl Filesystem {
//     async fn open_file(&self, name: &str) -> Result<(), Error> {
//         check_name(name)?;

//         let mut fs = self.0.lock().await;
//         let meta_key = Key::from_name(name);
//         let meta = fs.fetch_metadata(meta_key).await?;

//         let file = File {
//             chunks: meta.chunks,
//             meta_key,
//             chunk_key: meta.first_chunk,
//             cursor: 0,
//             name: String::from(meta.name.as_str()).into_boxed_str(),
//             fs: self.clone(),
//         };

//         Ok(())
//     }

//     async fn fetch_chunk(&self, key: Key) -> Result<Chunk, Error> {
//         self.0.lock().await.fetch_chunk(key).await
//     }

//     async fn write_chunk(&self, key: Key, chunk: Chunk) -> Result<(), Error> {
//         let mut inner = self.0.lock().await;
//         let node = Node::Chunk(chunk);
//         inner.write_node(key, node).await
//     }

//     async fn fetch_metadata_by_name(&self, name: &str) -> Result<Metadata, Error> {
//         check_name(name)?;

//         self.0.lock().await.fetch_metadata_by_name(name).await
//     }

//     async fn fetch_metadata(&self, key: Key) -> Result<Metadata, Error> {
//         self.0.lock().await.fetch_metadata(key).await
//     }

//     async fn write_metadata(&self, metadata: Metadata) -> Result<(), Error> {
//         let mut inner = self.0.lock().await;
//         let key = Key::from_name(&metadata.name);
//         let node = Node::Metadata(metadata);

//         inner.write_node(key, node).await
//     }

//     async fn chunk_exists(&self, key: Key) -> Result<bool, Error> {
//         let mut fs = self.0.lock().await;
//         match fs.fetch_node(key).await {
//             Ok(Node::Chunk(_)) => Ok(true),
//             Ok(_) => Err(Error::IsNotChunk),
//             Err(Error::NotFound) => Ok(false),
//             Err(e) => Err(e),
//         }
//     }

//     async fn metadata_exists(&self, name: &str) -> Result<bool, Error> {
//         check_name(name)?;

//         let mut fs = self.0.lock().await;
//         match fs.fetch_node(Key::from_name(name)).await {
//             Ok(Node::Metadata(_)) => Ok(true),
//             Ok(_) => Err(Error::IsNotChunk),
//             Err(Error::NotFound) => Ok(false),
//             Err(e) => Err(e),
//         }
//     }

//     async fn create_file(&self, name: &str) -> Result<(), Error> {
//         check_name(name)?;

//         let mut fs = self.0.lock().await;
//         let mut id = [0; CHUNK_ID_BYTES];
//         fs.rng.read(&mut id);

//         // crude protection to make sure that the RNG writes to every byte in the id.
//         for byte in id.iter_mut() {
//             loop {
//                 if *byte == 0 {
//                     let rand = fs.rng.random();
//                     *byte = rand as u8;
//                 } else {
//                     break;
//                 }
//             }
//         }

//         let name = FixedString::from_str(name).unwrap();
//         let first_chunk = Key::from_first_chunk(id);

//         let meta = Metadata {
//             chunks: 0,
//             first_chunk,
//             name,
//         };

//         Ok(())
//     }
// }

// impl fmt::Debug for Filesystem {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         let guard = self.0.try_lock();

//         let cache: &dyn fmt::Debug;
//         let storage: &dyn fmt::Debug;
//         let rng: &dyn fmt::Debug;

//         match &guard {
//             Ok(inner) => {
//                 // there's no difference between Storage values.
//                 cache = &inner.cache;
//                 storage = &inner.storage;
//                 rng = &"Rng";
//             }
//             Err(_) => {
//                 cache = &"<locked>";
//                 storage = &"<locked>";
//                 rng = &"<locked>";
//             }
//         }

//         f.debug_struct("Filesystem")
//             .field("cache", cache)
//             .field("storage", storage)
//             .field("rng", rng)
//             .finish()
//     }
// }

// struct Inner {
//     cache: Cache,
//     storage: Storage,
//     rng: Rng,
// }

// impl Inner {
//     async fn fetch_node(&mut self, key: Key) -> Result<Node, Error> {
//         let res = {
//             let mut buf = AlignedArray([0; Node::POSTCARD_MAX_SIZE]);
//             map::fetch_item(&mut self.storage, FS_RANGE, &mut self.cache, &mut buf, &key).await
//         };

//         match res {
//             Ok(Some(node)) => Ok(node),
//             Ok(None) => Err(Error::NotFound),
//             Err(e) => Err(e.into()),
//         }
//     }

//     async fn write_node(&mut self, key: Key, node: Node) -> Result<(), Error> {
//         let mut buf = AlignedArray([0; Node::POSTCARD_MAX_SIZE]);

//         map::store_item(
//             &mut self.storage,
//             FS_RANGE,
//             &mut self.cache,
//             &mut buf,
//             &key,
//             &node,
//         )
//         .await?;
//         Ok(())
//     }

//     async fn fetch_metadata_by_name(&mut self, name: &str) -> Result<Metadata, Error> {
//         self.fetch_node(Key::from_name(name))
//             .await?
//             .into_metadata()
//             .ok_or(Error::IsNotMetadata)
//     }

//     async fn fetch_metadata(&mut self, key: Key) -> Result<Metadata, Error> {
//         self.fetch_node(key)
//             .await?
//             .into_metadata()
//             .ok_or(Error::IsNotMetadata)
//     }

//     async fn fetch_chunk(&mut self, key: Key) -> Result<Chunk, Error> {
//         self.fetch_node(key)
//             .await?
//             .into_chunk()
//             .ok_or(Error::IsNotMetadata)
//     }

//     // NOTE: This is incredibly fucking slow!!! It should be use extremely rarely.
//     async fn delete_file(&mut self, name: &str) -> Result<(), Error> {
//         check_name(name)?;

//         let meta_key = Key::from_name(name);
//         let meta = self.fetch_metadata(meta_key).await?;
//         let mut buf = AlignedArray([0; Node::POSTCARD_MAX_SIZE]);
//         let mut key = meta.first_chunk;

//         while key.chunk() <= meta.chunks {
//             map::remove_item(&mut self.storage, FS_RANGE, &mut self.cache, &mut buf, &key).await?;
//             key.make_next_chunk();
//         }

//         map::remove_item(
//             &mut self.storage,
//             FS_RANGE,
//             &mut self.cache,
//             &mut buf,
//             &meta_key,
//         )
//         .await?;

//         Ok(())
//     }
// }

// #[repr(transparent)]
// #[derive(
//     Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize, MaxSize,
// )]
// pub struct Key([u8; KEY_BYTES]);

// impl Key {
//     pub fn from_name(name: &str) -> Self {
//         Self(sha256(name))
//     }

//     pub fn from_first_chunk(id: [u8; CHUNK_ID_BYTES]) -> Self {
//         let mut buf = [0; KEY_BYTES];
//         buf[size_of::<u16>()..].copy_from_slice(&id);

//         Self(buf)
//     }

//     pub fn next_chunk(&self) -> Self {
//         let mut new = *self;
//         new.make_next_chunk();
//         new
//     }

//     pub fn prev_chunk(&self) -> Self {
//         let mut new = *self;
//         new.make_prev_chunk();
//         new
//     }

//     pub fn make_next_chunk(&mut self) {
//         let next = self.chunk().wrapping_add(1);
//         self.0[..U16_BYTES].copy_from_slice(&next.to_le_bytes());
//     }

//     pub fn make_prev_chunk(&mut self) {
//         let next = self.chunk().wrapping_sub(1);
//         self.0[..U16_BYTES].copy_from_slice(&next.to_le_bytes());
//     }

//     pub fn chunk(&self) -> u16 {
//         let mut buf = [0; U16_BYTES];
//         buf.copy_from_slice(&self.0[..U16_BYTES]);
//         u16::from_le_bytes(buf)
//     }
// }

// impl MapKey for Key {
//     fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, MapSerError> {
//         MapKey::serialize_into(&self.0, buffer)
//     }

//     fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), MapSerError> {
//         let (key, len) = <[u8; KEY_BYTES] as MapKey>::deserialize_from(buffer)?;

//         Ok((Self(key), len))
//     }
// }

// #[derive(Clone, Debug)]
// pub struct File {
//     chunks: u16,
//     meta_key: Key,
//     chunk_key: Key,
//     cursor: usize,
//     name: Box<str>,
//     fs: Filesystem,
// }

// impl File {
//     pub fn name(&self) -> &str {
//         &self.name
//     }
// }

// impl IoErrorType for File {
//     type Error = Error;
// }

// impl AsyncRead for File {
//     async fn read(&mut self, mut buf: &mut [u8]) -> Result<usize, Self::Error> {
//         let mut bytes_read = 0;

//         while !buf.is_empty() && self.chunk_key.chunk() < self.chunks {
//             let chunk = self.fs.fetch_chunk(self.chunk_key).await?;

//             let chunk_len = chunk.0.len();
//             let count = chunk_len.min(buf.len());
//             let chunk_range = self.cursor..self.cursor + count;

//             let (head, tail) = buf.split_at_mut(count);

//             head.copy_from_slice(&chunk.0[chunk_range]);

//             self.cursor += count;
//             bytes_read += count;

//             if self.cursor == chunk_len {
//                 self.cursor = 0;
//                 self.chunk_key.make_next_chunk();
//             }

//             buf = tail;
//         }

//         Ok(bytes_read)
//     }
// }

// impl AsyncWrite for File {
//     async fn write(&mut self, mut buf: &[u8]) -> Result<usize, Self::Error> {
//         let mut bytes_written = 0;
//         let mut metadata = self.fs.fetch_metadata(self.meta_key).await?;

//         while !buf.is_empty() {
//             let mut chunk = match self.fs.fetch_chunk(self.chunk_key).await {
//                 Ok(c) => c,
//                 Err(Error::NotFound) => {
//                     if self.chunk_key.chunk() < u16::MAX {
//                         metadata.chunks = metadata.chunks.wrapping_add(1);
//                         self.chunk_key.make_next_chunk();
//                         Chunk(FixedVec::new())
//                     } else {
//                         // This can't even happen on any ESP32 variant since it would require
//                         // writing 255.75 MiB of data to a single file.
//                         return Err(Error::DataTooLarge);
//                     }
//                 }
//                 Err(e) => return Err(e),
//             };

//             let chunk_len = chunk.0.len();
//             let count = buf.len().min(chunk.0.capacity() - chunk_len);

//             let (head, tail) = buf.split_at(count);
//             chunk.0[chunk_len..].copy_from_slice(head);
//             buf = tail;

//             self.cursor += count;
//             bytes_written += count;

//             if chunk.0.is_full() {
//                 self.chunk_key.make_next_chunk();
//                 self.cursor = 0;
//             }
//         }

//         // updating metadata last ensures that even though writes may be "lost", reading or writing
//         // to the file will not attempt to read or write those lost chunks.
//         self.fs.write_metadata(metadata).await?;
//         Ok(bytes_written)
//     }
// }

// impl Seek for File {
//     fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
//         todo!()
//     }
// }

// // impl AsyncSeek for File {
    
// // }

// #[derive(Debug, Error)]
// pub enum Error {
//     #[error("filesystem corruption detected")]
//     Corrupted,
//     #[error("key was not found")]
//     NotFound,
//     #[error("file already exists")]
//     AlreadyExists,
//     #[error("name was invalid")]
//     Invalidname,
//     #[error("buffer was too small")]
//     BufferTooSmall,
//     #[error("data was too large to be stored")]
//     DataTooLarge,
//     #[error("operation was canceled")]
//     Canceled,
//     #[error("filesystem is full")]
//     Full,
//     #[error("invalid filesystem format")]
//     InvalidFormat,
//     #[error("filename is too long")]
//     FilenameTooLong,
//     #[error("filesystem node is not metadata")]
//     IsNotMetadata,
//     #[error("filesystem node is not a chunk")]
//     IsNotChunk,
//     #[error("attempt to access out of bounds")]
//     OutOfBounds,
//     #[error("attempted to write zero bytes to filesystem")]
//     WriteZero,
//     #[error("postcard error: {0}")]
//     Postcard(postcard::Error),
//     #[error("flash storage error: {0:?}")]
//     Flash(EspFlashStorageError),
// }

// impl IoError for Error {
//     fn kind(&self) -> IoErrorKind {
//         use EspFlashStorageError as EfsError;

//         match self {
//             Self::Corrupted => IoErrorKind::InvalidData,
//             Self::NotFound => IoErrorKind::NotFound,
//             Self::AlreadyExists => IoErrorKind::AlreadyExists,
//             Self::Invalidname => IoErrorKind::InvalidInput,
//             Self::BufferTooSmall => IoErrorKind::InvalidInput,
//             Self::DataTooLarge => IoErrorKind::InvalidInput,
//             Self::Canceled => IoErrorKind::Interrupted,
//             Self::Full => IoErrorKind::OutOfMemory,
//             Self::InvalidFormat => IoErrorKind::InvalidData,
//             Self::FilenameTooLong => IoErrorKind::InvalidInput,
//             Self::IsNotMetadata => IoErrorKind::InvalidData,
//             Self::IsNotChunk => IoErrorKind::InvalidData,
//             Self::OutOfBounds => IoErrorKind::InvalidInput,
//             Self::WriteZero => IoErrorKind::WriteZero,
//             Self::Postcard(_) => IoErrorKind::Other,
//             Self::Flash(e) => match e {
//                 EfsError::IoError => IoErrorKind::Other,
//                 EfsError::IoTimeout => IoErrorKind::TimedOut,
//                 EfsError::CantUnlock => IoErrorKind::PermissionDenied,
//                 EfsError::NotAligned => IoErrorKind::InvalidInput,
//                 EfsError::OutOfBounds => IoErrorKind::Other,
//                 EfsError::Other(_) => IoErrorKind::Other,
//                 _ => unreachable!("flash storage error has a new variant"),
//             },
//         }
//     }
// }

// impl From<postcard::Error> for Error {
//     fn from(value: postcard::Error) -> Self {
//         Self::Postcard(value)
//     }
// }

// impl From<SeqStorageError<EspFlashStorageError>> for Error {
//     fn from(value: SeqStorageError<EspFlashStorageError>) -> Self {
//         match value {
//             SeqStorageError::Storage { value } => Self::Flash(value),
//             SeqStorageError::FullStorage => Self::Full,
//             SeqStorageError::Corrupted {} => Self::Corrupted,
//             SeqStorageError::BufferTooBig => Self::DataTooLarge,
//             SeqStorageError::BufferTooSmall(_) => Self::BufferTooSmall,
//             SeqStorageError::SerializationError(serialization_error) => {
//                 Self::from(serialization_error)
//             }
//             SeqStorageError::ItemTooBig => Self::DataTooLarge,
//             e => unimplemented!("unknown SeqStorageError {e:?}"),
//         }
//     }
// }

// impl From<MapSerError> for Error {
//     fn from(value: MapSerError) -> Self {
//         match value {
//             MapSerError::BufferTooSmall => Self::BufferTooSmall,
//             MapSerError::InvalidData => Self::DataTooLarge,
//             MapSerError::InvalidFormat => Self::InvalidFormat,
//             MapSerError::Custom(e) => Self::Postcard(i32_to_postcard_error(e)),
//             e => unimplemented!("unknown MapSerError {e:?}"),
//         }
//     }
// }

// #[repr(C, align(4))]
// struct AlignedArray<T, const N: usize>([T; N]);

// impl<T, const N: usize> Deref for AlignedArray<T, N> {
//     type Target = [T];

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl<T, const N: usize> DerefMut for AlignedArray<T, N> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

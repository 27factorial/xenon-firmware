use super::{postcard_error_to_i32, Key};
use core::mem;
use heapless::{String as FixedString, Vec as FixedVec};
use postcard::experimental::max_size::MaxSize;
use sequential_storage::map::{SerializationError as MapSerError, Value};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_NAME_BYTES: usize = 255;
pub(crate) const CHUNK_MAX_SIZE: usize = 4096;
pub(crate) const CHUNK_DATA_SIZE: usize = CHUNK_MAX_SIZE - size_of::<FixedVec<u8, 0>>(); // 4096

#[allow()]
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize, MaxSize)]
pub enum Node {
    Metadata(Metadata),
    Chunk(Chunk),
}

impl Node {
    pub fn metadata(&self) -> Option<&Metadata> {
        match self {
            Node::Metadata(metadata) => Some(metadata),
            _ => None,
        }
    }

    pub fn chunk(&self) -> Option<&Chunk> {
        match self {
            Node::Chunk(chunk) => Some(chunk),
            _ => None,
        }
    }

    pub fn into_metadata(self) -> Option<Metadata> {
        match self {
            Node::Metadata(metadata) => Some(metadata),
            _ => None,
        }
    }

    pub fn into_chunk(self) -> Option<Chunk> {
        match self {
            Node::Chunk(chunk) => Some(chunk),
            _ => None,
        }
    }
}

impl<'a> Value<'a> for Node {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, MapSerError> {
        let slice = postcard::to_slice(self, buffer)
            .map_err(|e| MapSerError::Custom(postcard_error_to_i32(e)))?;

        Ok(slice.len())
    }

    fn deserialize_from(buffer: &'a [u8]) -> Result<Self, MapSerError>
    where
        Self: Sized,
    {
        postcard::from_bytes(buffer).map_err(|e| MapSerError::Custom(postcard_error_to_i32(e)))
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize, MaxSize)]
pub struct Metadata {
    pub(crate) chunks: u16,
    pub(crate) first_chunk: Key,
    pub(crate) name: FixedString<MAX_NAME_BYTES>,
}

#[repr(transparent)]
#[derive(
    Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, Deserialize, MaxSize,
)]
pub struct Chunk(pub FixedVec<u8, CHUNK_DATA_SIZE>);

impl Chunk {
    const _SIZE_CHECK: () = const {
        assert!(mem::size_of::<Self>() <= CHUNK_MAX_SIZE);
    };
}

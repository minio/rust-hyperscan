use core::fmt;
use core::ops::Deref;
use core::ptr::{null_mut, NonNull};
use core::slice;
use std::ffi::CStr;

use failure::{AsFail, Error};
use foreign_types::{ForeignType, ForeignTypeRef};

use crate::common::{Database, DatabaseRef};
use crate::errors::AsResult;
use crate::ffi;

/// A type representing an owned, C-compatible buffer.
#[derive(Debug)]
pub struct CBuffer<T>(NonNull<T>, usize);

impl<T> Drop for CBuffer<T> {
    fn drop(&mut self) {
        unsafe { libc::free(self.0.as_ptr() as *mut _) }
    }
}

impl<T> Deref for CBuffer<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> AsRef<[T]> for CBuffer<T> {
    fn as_ref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.0.as_ptr(), self.1) }
    }
}

/// A serialized database
pub trait Serialized {
    /// The type of error if it fails in a normal fashion.
    type Error: fmt::Debug + AsFail;

    /// Reporting the size that would be required by a database if it were deserialized.
    fn size(&self) -> Result<usize, Self::Error>;

    /// Providing information about a serialized database.
    fn info(&self) -> Result<String, Self::Error>;

    /// Reconstruct a pattern database from a stream of bytes previously generated by `Database::serialize()`.
    fn deserialize<M>(&self) -> Result<Database<M>, Self::Error>;
}

impl<T: AsRef<[u8]>> Serialized for T {
    type Error = Error;

    fn size(&self) -> Result<usize, Error> {
        let buf = self.as_ref();
        let mut size = 0;

        unsafe { ffi::hs_serialized_database_size(buf.as_ptr() as *const _, buf.len(), &mut size).map(|_| size) }
    }

    fn info(&self) -> Result<String, Error> {
        let buf = self.as_ref();
        let mut p = null_mut();

        unsafe {
            ffi::hs_serialized_database_info(buf.as_ptr() as *const _, buf.len(), &mut p).and_then(|_| {
                let info = CStr::from_ptr(p).to_str()?.to_owned();

                if !p.is_null() {
                    libc::free(p as *mut _)
                }

                Ok(info)
            })
        }
    }

    fn deserialize<M>(&self) -> Result<Database<M>, Error> {
        let buf = self.as_ref();
        let mut db = null_mut();

        unsafe {
            ffi::hs_deserialize_database(buf.as_ptr() as *const i8, buf.len(), &mut db).map(|_| Database::from_ptr(db))
        }
    }
}

impl<T> DatabaseRef<T> {
    /// Serialize a pattern database to a stream of bytes.
    pub fn serialize(&self) -> Result<CBuffer<u8>, Error> {
        let mut ptr = null_mut();
        let mut size: usize = 0;

        unsafe {
            ffi::hs_serialize_database(self.as_ptr(), &mut ptr, &mut size)
                .map(|_| CBuffer(NonNull::new_unchecked(ptr).cast(), size))
        }
    }

    /// Reconstruct a pattern database from a stream of bytes
    /// previously generated by `Database::serialize()` at a given memory location.
    pub fn deserialize_at<B: AsRef<[u8]>>(&mut self, bytes: B) -> Result<(), Error> {
        let bytes = bytes.as_ref();

        unsafe { ffi::hs_deserialize_database_at(bytes.as_ptr() as *const i8, bytes.len(), self.as_ptr()).ok() }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::common::database::tests::*;
    use crate::prelude::*;

    use super::*;

    pub fn validate_serialized_database<S: Serialized>(data: &S) {
        assert!(data.size().unwrap() >= DATABASE_SIZE);

        validate_database_info(data.info().unwrap().as_str());
    }

    #[test]
    fn test_database_serialize() {
        let _ = pretty_env_logger::try_init();

        let db: StreamingDatabase = pattern! { "test" }.build().unwrap();

        let data = db.serialize().unwrap();

        validate_serialized_database(&data);

        assert!(!data.info().unwrap().is_empty());
    }

    #[test]
    fn test_database_deserialize() {
        let _ = pretty_env_logger::try_init();

        let db: VectoredDatabase = pattern! { "test" }.build().unwrap();

        let data = db.serialize().unwrap();

        validate_serialized_database(&data);

        let db: VectoredDatabase = data.deserialize().unwrap();

        validate_database(&db);
    }

    #[test]
    fn test_database_deserialize_at() {
        let _ = pretty_env_logger::try_init();

        let mut db: BlockDatabase = pattern! { "test" }.build().unwrap();

        let data = db.serialize().unwrap();

        validate_serialized_database(&data);

        db.deserialize_at(&data).unwrap();

        validate_database(&db);
    }
}

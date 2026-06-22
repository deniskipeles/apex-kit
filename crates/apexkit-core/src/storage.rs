pub mod local;
pub mod s3;
pub mod traits;

pub use local::LocalStorage;
pub use s3::S3Storage;
pub use traits::StorageBackend;

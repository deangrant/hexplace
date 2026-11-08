// Eval fixture for rust-coding-standard case 3. Do not edit.

use std::io;

/// Stand-in for an object-store client. Not a real SDK type.
pub struct ObjectStoreSink {
    bucket: String,
}

impl ObjectStoreSink {
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
        }
    }

    pub fn put(&self, name: &str, body: Vec<u8>) -> io::Result<()> {
        let _ = (&self.bucket, name, body);
        Ok(())
    }

    pub fn delete(&self, name: &str) -> io::Result<()> {
        let _ = (&self.bucket, name);
        Ok(())
    }

    pub fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        let _ = (&self.bucket, prefix);
        Ok(Vec::new())
    }
}

/// Writes one chunk to object storage.
pub fn process_chunk(bucket: &str, name: &str, body: Vec<u8>) -> io::Result<()> {
    let sink = ObjectStoreSink::new(bucket);
    sink.put(name, body)
}

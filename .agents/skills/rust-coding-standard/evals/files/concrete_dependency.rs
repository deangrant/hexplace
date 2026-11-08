// Eval fixture for rust-coding-standard case 3. Do not edit.

use std::io;

/// Stand-in for an Azure blob client. Not a real SDK type.
pub struct AzureBlobSink {
    account: String,
}

impl AzureBlobSink {
    pub fn new(account: impl Into<String>) -> Self {
        Self {
            account: account.into(),
        }
    }

    pub fn put(&self, name: &str, body: Vec<u8>) -> io::Result<()> {
        let _ = (&self.account, name, body);
        Ok(())
    }

    pub fn delete(&self, name: &str) -> io::Result<()> {
        let _ = (&self.account, name);
        Ok(())
    }

    pub fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        let _ = (&self.account, prefix);
        Ok(Vec::new())
    }
}

/// Writes one Parquet chunk.
pub fn process_chunk(account: &str, name: &str, body: Vec<u8>) -> io::Result<()> {
    let sink = AzureBlobSink::new(account);
    sink.put(name, body)
}

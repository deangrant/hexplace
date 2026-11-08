// Eval fixture for rust-coding-standard case 2. Do not edit.

use std::fmt;

mod exit {
    #[derive(Debug, Clone, Copy)]
    pub enum Exit {
        Success = 0,
        Config = 1,
    }
}

pub use exit::Exit;

#[derive(Debug)]
pub struct ChunkName(String);

#[derive(Debug)]
pub enum ParseError {
    Empty,
    Invalid(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "chunk name is empty"),
            Self::Invalid(s) => write!(f, "invalid chunk name: {s}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_chunk_name(raw: &str) -> Result<ChunkName, ParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(ParseError::Invalid(trimmed.to_owned()));
    }
    Ok(ChunkName(trimmed.to_owned()))
}

pub fn first_item(items: &[u32]) -> u32 {
    items[0]
}

fn digits_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

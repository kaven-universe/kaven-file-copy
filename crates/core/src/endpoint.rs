use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointCapabilities {
    pub atomic_commit: bool,
    pub random_read: bool,
    pub symbolic_links: bool,
    pub preserve_mtime: bool,
    pub server_checksum: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    SymbolicLink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointMetadata {
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointEntry {
    pub relative_path: PathBuf,
    pub metadata: EndpointMetadata,
}

pub trait PendingWrite: Write + Send {
    fn commit(self: Box<Self>) -> Result<()>;
}

pub trait Endpoint: Send + Sync {
    fn endpoint_type(&self) -> &'static str;
    fn capabilities(&self) -> EndpointCapabilities;
    fn stat(&self, relative_path: &Path) -> Result<Option<EndpointMetadata>>;
    fn list(&self, relative_directory: &Path) -> Result<Vec<EndpointEntry>>;
    fn open_read(&self, relative_path: &Path) -> Result<Box<dyn Read + Send>>;
    fn open_write_temp(&self, relative_path: &Path) -> Result<Box<dyn PendingWrite>>;
    fn create_directory(&self, relative_path: &Path) -> Result<()>;
    fn set_modified_time(&self, relative_path: &Path, modified: SystemTime) -> Result<()>;
}

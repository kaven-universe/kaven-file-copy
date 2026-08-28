use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use filetime::FileTime;
use kfc_core::{
    Endpoint, EndpointCapabilities, EndpointEntry, EndpointMetadata, EntryKind, IoResultExt,
    KfcError, PendingWrite, Result,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LocalEndpoint {
    root: PathBuf,
}

impl LocalEndpoint {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root.as_ref()).at(root.as_ref())?;
        if !root.is_dir() {
            return Err(KfcError::InvalidRule(format!(
                "endpoint root is not a directory: {}",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    fn resolve(&self, relative: &Path) -> Result<PathBuf> {
        if relative.is_absolute()
            || relative.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::Prefix(_) | Component::RootDir
                )
            })
        {
            return Err(KfcError::PathEscape(relative.display().to_string()));
        }
        Ok(self.root.join(relative))
    }
}

impl Endpoint for LocalEndpoint {
    fn endpoint_type(&self) -> &'static str {
        "local"
    }

    fn capabilities(&self) -> EndpointCapabilities {
        EndpointCapabilities {
            atomic_commit: true,
            random_read: true,
            symbolic_links: true,
            preserve_mtime: true,
            server_checksum: false,
        }
    }

    fn stat(&self, relative_path: &Path) -> Result<Option<EndpointMetadata>> {
        let path = self.resolve(relative_path)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => Ok(Some(to_metadata(&metadata))),
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(KfcError::Io { path, source }),
        }
    }

    fn list(&self, relative_directory: &Path) -> Result<Vec<EndpointEntry>> {
        let directory = self.resolve(relative_directory)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory).at(&directory)? {
            let entry = entry.at(&directory)?;
            let relative_path = relative_directory.join(entry.file_name());
            let metadata = entry.metadata().at(entry.path())?;
            entries.push(EndpointEntry {
                relative_path,
                metadata: to_metadata(&metadata),
            });
        }
        Ok(entries)
    }

    fn open_read(&self, relative_path: &Path) -> Result<Box<dyn Read + Send>> {
        let path = self.resolve(relative_path)?;
        Ok(Box::new(fs::File::open(&path).at(&path)?))
    }

    fn open_write_temp(&self, relative_path: &Path) -> Result<Box<dyn PendingWrite>> {
        let target = self.resolve(relative_path)?;
        let parent = target
            .parent()
            .ok_or_else(|| KfcError::PathEscape(target.display().to_string()))?;
        fs::create_dir_all(parent).at(parent)?;
        let name = target
            .file_name()
            .ok_or_else(|| KfcError::PathEscape(target.display().to_string()))?
            .to_string_lossy();
        let temporary = parent.join(format!(".{name}.kfc-part-{}", Uuid::new_v4()));
        let file = fs::File::create(&temporary).at(&temporary)?;
        Ok(Box::new(LocalPendingWrite {
            file: Some(file),
            temporary,
            target,
        }))
    }

    fn create_directory(&self, relative_path: &Path) -> Result<()> {
        let path = self.resolve(relative_path)?;
        fs::create_dir_all(&path).at(path)
    }

    fn set_modified_time(&self, relative_path: &Path, modified: SystemTime) -> Result<()> {
        let path = self.resolve(relative_path)?;
        filetime::set_file_mtime(&path, FileTime::from_system_time(modified)).at(path)
    }
}

struct LocalPendingWrite {
    file: Option<fs::File>,
    temporary: PathBuf,
    target: PathBuf,
}

impl Write for LocalPendingWrite {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.file
            .as_mut()
            .expect("pending file is open")
            .write(buffer)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.as_mut().expect("pending file is open").flush()
    }
}

impl PendingWrite for LocalPendingWrite {
    fn commit(mut self: Box<Self>) -> Result<()> {
        let mut file = self.file.take().expect("pending file is open");
        file.flush().at(&self.temporary)?;
        file.sync_all().at(&self.temporary)?;
        drop(file);
        let backup = self
            .target
            .with_extension(format!("kfc-backup-{}", Uuid::new_v4()));
        let had_target = self.target.exists();
        if had_target {
            fs::rename(&self.target, &backup).at(&self.target)?;
        }
        if let Err(source) = fs::rename(&self.temporary, &self.target) {
            if had_target {
                let _ = fs::rename(&backup, &self.target);
            }
            return Err(KfcError::Io {
                path: self.target.clone(),
                source,
            });
        }
        if had_target {
            fs::remove_file(&backup).at(backup)?;
        }
        Ok(())
    }
}

impl Drop for LocalPendingWrite {
    fn drop(&mut self) {
        if self.temporary.exists() {
            let _ = fs::remove_file(&self.temporary);
        }
    }
}

fn to_metadata(metadata: &fs::Metadata) -> EndpointMetadata {
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        EntryKind::SymbolicLink
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    EndpointMetadata {
        kind,
        size: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_write_is_invisible_until_commit() {
        let root = std::env::temp_dir().join(format!("kfc-endpoint-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let endpoint = LocalEndpoint::new(&root).unwrap();
        let path = Path::new("nested/file.txt");
        let mut pending = endpoint.open_write_temp(path).unwrap();
        pending.write_all(b"endpoint data").unwrap();
        assert!(endpoint.stat(path).unwrap().is_none());
        pending.commit().unwrap();
        assert_eq!(endpoint.stat(path).unwrap().unwrap().size, 13);
        assert_eq!(fs::read(root.join(path)).unwrap(), b"endpoint data");
        fs::remove_dir_all(root).unwrap();
    }
}

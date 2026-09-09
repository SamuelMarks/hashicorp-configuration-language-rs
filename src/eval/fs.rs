//! Pluggable virtual filesystem abstraction for evaluation functions.
//!
//! Provides the [`FileSystem`] trait along with an operating system implementation
//! ([`OsFileSystem`]) and an in-memory implementation ([`MemFileSystem`]) for isolated testing.

#![deny(missing_docs)]

use crate::error::HclError;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

/// Pluggable filesystem abstraction trait.
///
/// Enables HCL functions (like `file`, `fileset`, and `templatefile`) to operate over
/// custom, in-memory, or mocked filesystems rather than directly binding to the operating system.
pub trait FileSystem: Send + Sync + std::fmt::Debug {
    /// Reads the raw byte contents of a file.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if the file cannot be read or does not exist.
    fn read(&self, path: &Path) -> Result<Vec<u8>, HclError>;

    /// Reads the entire contents of a file as a UTF-8 string.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if the file cannot be read or contains invalid UTF-8.
    fn read_to_string(&self, path: &Path) -> Result<String, HclError>;

    /// Returns `true` if the path exists (as either a file or a directory).
    fn exists(&self, path: &Path) -> bool;

    /// Returns `true` if the path exists and is a regular file.
    fn is_file(&self, path: &Path) -> bool;

    /// Returns `true` if the path exists and is a directory.
    fn is_dir(&self, path: &Path) -> bool;

    /// Returns the canonical, absolute path.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if canonicalization fails.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HclError>;

    /// Reads the directory at `path`, returning a list of entries.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if reading the directory fails.
    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, HclError>;
}

/// Standard filesystem implementation backed by `std::fs` operations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OsFileSystem;

impl OsFileSystem {
    /// Creates a new `OsFileSystem`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl FileSystem for OsFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, HclError> {
        std::fs::read(path).map_err(|e| HclError::FileSystem(e.to_string()))
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HclError> {
        std::fs::read_to_string(path).map_err(|e| HclError::FileSystem(e.to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HclError> {
        std::fs::canonicalize(path).map_err(|e| HclError::FileSystem(e.to_string()))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, HclError> {
        let entries = std::fs::read_dir(path).map_err(|e| HclError::FileSystem(e.to_string()))?;
        let mut paths = Vec::new();
        for entry in entries.flatten() {
            paths.push(entry.path());
        }
        paths.sort();
        Ok(paths)
    }
}

/// In-memory filesystem implementation backed by a map of paths to byte contents.
///
/// Thread-safe via internal synchronization (`RwLock`).
///
/// # Examples
/// ```rust
/// use hashicorp_configuration_language_rs::eval::fs::{FileSystem, MemFileSystem};
/// use std::path::Path;
///
/// let mut mem_fs = MemFileSystem::new();
/// mem_fs.insert_file("configs/app.hcl", "port = 8080");
/// assert!(mem_fs.exists(Path::new("configs/app.hcl")));
/// assert_eq!(
///     mem_fs.read_to_string(Path::new("configs/app.hcl")).expect("read file"),
///     "port = 8080"
/// );
/// ```
#[derive(Debug, Default)]
pub struct MemFileSystem {
    files: RwLock<BTreeMap<PathBuf, Vec<u8>>>,
}

impl MemFileSystem {
    /// Creates a new, empty in-memory filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: RwLock::new(BTreeMap::new()),
        }
    }

    /// Inserts or overwrites a file with the given contents.
    ///
    /// # Arguments
    /// * `path` - The file path to store.
    /// * `contents` - The file data as bytes or string.
    pub fn insert_file(&mut self, path: impl AsRef<Path>, contents: impl Into<Vec<u8>>) {
        self.add_file(path, contents);
    }

    /// Inserts or overwrites a file through a shared reference.
    ///
    /// # Arguments
    /// * `path` - The file path to store.
    /// * `contents` - The file data as bytes or string.
    pub fn add_file(&self, path: impl AsRef<Path>, contents: impl Into<Vec<u8>>) {
        self.add_file_bytes(path.as_ref(), contents.into());
    }

    fn add_file_bytes(&self, path: &Path, contents: Vec<u8>) {
        let norm = normalize_path(path);
        if let Ok(mut files) = self.files.write() {
            files.insert(norm, contents);
        }
    }
}

/// Normalizes a path by removing redundant `.` and resolving `..` components.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = components.last() {
                    if *last == Component::RootDir {
                        continue;
                    }
                    if *last != Component::ParentDir {
                        components.pop();
                        continue;
                    }
                }
                components.push(Component::ParentDir);
            }
            Component::Normal(c) => components.push(Component::Normal(c)),
            Component::RootDir | Component::Prefix(_) => components.push(comp),
        }
    }

    if components.is_empty() {
        PathBuf::from(".")
    } else {
        components.iter().collect()
    }
}

impl FileSystem for MemFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, HclError> {
        let norm = normalize_path(path);
        let files = self
            .files
            .read()
            .map_err(|e| HclError::FileSystem(format!("Lock poisoned: {e}")))?;

        files
            .get(&norm)
            .cloned()
            .ok_or_else(|| HclError::FileSystem(format!("File not found: '{}'", path.display())))
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HclError> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| HclError::FileSystem(e.to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        let norm = normalize_path(path);
        if let Ok(files) = self.files.read() {
            files.contains_key(&norm)
        } else {
            false
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        let norm = normalize_path(path);
        if let Ok(files) = self.files.read() {
            if files.contains_key(&norm) {
                return false;
            }
            // A path is a directory if it is a prefix of any registered file
            for file_path in files.keys() {
                if file_path.starts_with(&norm) {
                    return true;
                }
            }
            false
        } else {
            false
        }
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HclError> {
        let norm = normalize_path(path);
        if self.exists(&norm) {
            Ok(norm)
        } else {
            Err(HclError::FileSystem(format!(
                "Path does not exist: '{}'",
                path.display()
            )))
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, HclError> {
        let norm = normalize_path(path);
        let files = self
            .files
            .read()
            .map_err(|e| HclError::FileSystem(format!("Lock poisoned: {e}")))?;

        let mut children = std::collections::BTreeSet::new();
        let mut dir_found = false;
        let is_root = norm == Path::new(".");

        for file_path in files.keys() {
            let rel_opt = if is_root {
                Some(file_path.as_path())
            } else {
                file_path.strip_prefix(&norm).ok()
            };

            if let Some(rel) = rel_opt {
                for first in rel.components().take(1) {
                    dir_found = true;
                    if is_root {
                        children.insert(PathBuf::from(first.as_os_str()));
                    } else {
                        children.insert(norm.join(first.as_os_str()));
                    }
                }
            }
        }

        if !dir_found && !is_root {
            return Err(HclError::FileSystem(format!(
                "Directory not found: '{}'",
                path.display()
            )));
        }

        Ok(children.into_iter().collect())
    }
}

/// Virtual read-only filesystem operating directly over `.zip`, `.tar`, and `.tar.gz` compressed archives.
#[derive(Debug, Default)]
pub struct ArchiveFileSystem {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: std::collections::BTreeSet<PathBuf>,
}

impl ArchiveFileSystem {
    /// Loads an in-memory `ArchiveFileSystem` from a ZIP archive stream.
    ///
    /// # Arguments
    /// * `reader` - Seekable reader containing the ZIP archive data.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if reading or decoding the ZIP archive fails.
    pub fn from_zip<R: std::io::Read + std::io::Seek>(reader: R) -> Result<Self, HclError> {
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|e| HclError::FileSystem(format!("Failed to open ZIP archive: {e}")))?;

        let mut files = BTreeMap::new();
        let mut dirs = std::collections::BTreeSet::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| HclError::FileSystem(format!("Failed to read ZIP entry: {e}")))?;

            let raw_path = file.mangled_name();
            let norm = normalize_path(&raw_path);

            if file.is_dir() {
                dirs.insert(norm);
            } else {
                let mut content = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut content)
                    .map_err(|e| HclError::FileSystem(format!("Failed to read ZIP file: {e}")))?;
                // Register parent directories
                let mut curr = norm.parent();
                while let Some(parent) = curr {
                    if parent != Path::new("") {
                        dirs.insert(parent.to_path_buf());
                    }
                    curr = parent.parent();
                }
                files.insert(norm, content);
            }
        }

        Ok(Self { files, dirs })
    }

    /// Loads an `ArchiveFileSystem` from raw ZIP archive bytes.
    ///
    /// # Arguments
    /// * `bytes` - Slice of raw ZIP archive data.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if decoding fails.
    pub fn from_zip_bytes(bytes: &[u8]) -> Result<Self, HclError> {
        Self::from_zip(std::io::Cursor::new(bytes))
    }

    /// Loads an `ArchiveFileSystem` from a TAR archive stream.
    ///
    /// # Arguments
    /// * `reader` - Reader containing uncompressed TAR archive data.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if reading or decoding the TAR archive fails.
    pub fn from_tar(mut reader: impl std::io::Read) -> Result<Self, HclError> {
        Self::from_tar_reader(&mut reader)
    }

    fn from_tar_reader(reader: &mut dyn std::io::Read) -> Result<Self, HclError> {
        let mut archive = tar::Archive::new(reader);
        let mut files = BTreeMap::new();
        let mut dirs = std::collections::BTreeSet::new();

        for entry_res in archive.entries().into_iter().flatten() {
            let mut entry = entry_res
                .map_err(|e| HclError::FileSystem(format!("Failed to read TAR entry: {e}")))?;

            #[cfg(unix)]
            let path = {
                use std::os::unix::ffi::OsStrExt;
                PathBuf::from(std::ffi::OsStr::from_bytes(&entry.path_bytes()))
            };
            #[cfg(not(unix))]
            let path = entry
                .path()
                .map_err(|e| HclError::FileSystem(format!("Invalid path in TAR: {e}")))?
                .to_path_buf();
            let norm = normalize_path(&path);

            if entry.header().entry_type().is_dir() {
                dirs.insert(norm);
            } else if entry.header().entry_type().is_file() {
                let mut content = Vec::new();
                entry.read_to_end(&mut content).map_err(|e| {
                    HclError::FileSystem(format!("Failed to read TAR content: {e}"))
                })?;

                let mut curr = norm.parent();
                while let Some(parent) = curr {
                    if parent != Path::new("") {
                        dirs.insert(parent.to_path_buf());
                    }
                    curr = parent.parent();
                }
                files.insert(norm, content);
            }
        }

        Ok(Self { files, dirs })
    }

    /// Loads an `ArchiveFileSystem` from raw uncompressed TAR archive bytes.
    ///
    /// # Arguments
    /// * `bytes` - Slice of TAR archive data.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if decoding fails.
    pub fn from_tar_bytes(bytes: &[u8]) -> Result<Self, HclError> {
        Self::from_tar(bytes)
    }

    /// Loads an `ArchiveFileSystem` from a gzip-compressed TAR (`.tar.gz` / `.tgz`) stream.
    ///
    /// # Arguments
    /// * `reader` - Reader containing gzip-compressed TAR data.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if decompressing or decoding fails.
    pub fn from_tar_gz(reader: impl std::io::Read) -> Result<Self, HclError> {
        let mut gz = flate2::read::GzDecoder::new(reader);
        Self::from_tar_reader(&mut gz)
    }

    /// Loads an `ArchiveFileSystem` from raw gzip-compressed TAR archive bytes.
    ///
    /// # Arguments
    /// * `bytes` - Slice of gzip-compressed TAR archive bytes.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if decompressing or decoding fails.
    pub fn from_tar_gz_bytes(bytes: &[u8]) -> Result<Self, HclError> {
        Self::from_tar_gz(bytes)
    }
}

impl FileSystem for ArchiveFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, HclError> {
        let norm = normalize_path(path);
        self.files.get(&norm).cloned().ok_or_else(|| {
            HclError::FileSystem(format!("File not found in archive: '{}'", path.display()))
        })
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HclError> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| HclError::FileSystem(e.to_string()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    fn is_file(&self, path: &Path) -> bool {
        let norm = normalize_path(path);
        self.files.contains_key(&norm)
    }

    fn is_dir(&self, path: &Path) -> bool {
        let norm = normalize_path(path);
        if norm == Path::new(".") {
            return !self.files.is_empty() || !self.dirs.is_empty();
        }
        self.dirs.contains(&norm)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HclError> {
        let norm = normalize_path(path);
        if self.exists(&norm) {
            Ok(norm)
        } else {
            Err(HclError::FileSystem(format!(
                "Path does not exist in archive: '{}'",
                path.display()
            )))
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, HclError> {
        let norm = normalize_path(path);
        let mut children = std::collections::BTreeSet::new();
        let is_root = norm == Path::new(".");
        let mut found = is_root;

        for p in self.files.keys().chain(self.dirs.iter()) {
            let rel_opt = if is_root {
                Some(p.as_path())
            } else {
                p.strip_prefix(&norm).ok()
            };

            if let Some(rel) = rel_opt {
                found = true;
                if let Some(first) = rel.components().next() {
                    if is_root {
                        children.insert(PathBuf::from(first.as_os_str()));
                    } else {
                        children.insert(norm.join(first.as_os_str()));
                    }
                }
            }
        }

        if !found {
            return Err(HclError::FileSystem(format!(
                "Directory not found in archive: '{}'",
                path.display()
            )));
        }

        Ok(children.into_iter().collect())
    }
}

/// Sandboxed chroot filesystem wrapper enforcing strict root directory containment.
#[derive(Debug, Clone)]
pub struct SandboxedFileSystem {
    root: PathBuf,
    inner: std::sync::Arc<dyn FileSystem>,
}

impl SandboxedFileSystem {
    /// Creates a new `SandboxedFileSystem` jailed to `root`.
    ///
    /// # Arguments
    /// * `root` - The root boundary directory.
    /// * `inner` - The underlying filesystem implementation.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if the root does not exist or canonicalization fails.
    pub fn new(
        root: impl Into<PathBuf>,
        inner: std::sync::Arc<dyn FileSystem>,
    ) -> Result<Self, HclError> {
        let root_buf = root.into();
        let canonical_root = inner.canonicalize(&root_buf)?;
        Ok(Self {
            root: canonical_root,
            inner,
        })
    }

    /// Resolves and validates that a path stays strictly confined inside the sandbox root.
    ///
    /// # Arguments
    /// * `path` - The requested path.
    ///
    /// # Errors
    /// Returns [`HclError::FileSystem`] if a path traversal attempt is detected.
    pub fn resolve_sandboxed_path(&self, path: &Path) -> Result<PathBuf, HclError> {
        let mut target = self.root.clone();

        for comp in path.components() {
            match comp {
                Component::Normal(c) => target.push(c),
                Component::ParentDir => {
                    if target == self.root {
                        return Err(HclError::FileSystem(format!(
                            "Path traversal attack detected: access outside sandbox root '{}' is forbidden",
                            self.root.display()
                        )));
                    }
                    target.pop();
                }
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }

        Ok(target)
    }
}

impl FileSystem for SandboxedFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, HclError> {
        let resolved = self.resolve_sandboxed_path(path)?;
        self.inner.read(&resolved)
    }

    fn read_to_string(&self, path: &Path) -> Result<String, HclError> {
        let resolved = self.resolve_sandboxed_path(path)?;
        self.inner.read_to_string(&resolved)
    }

    fn exists(&self, path: &Path) -> bool {
        if let Ok(resolved) = self.resolve_sandboxed_path(path) {
            self.inner.exists(&resolved)
        } else {
            false
        }
    }

    fn is_file(&self, path: &Path) -> bool {
        if let Ok(resolved) = self.resolve_sandboxed_path(path) {
            self.inner.is_file(&resolved)
        } else {
            false
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        if let Ok(resolved) = self.resolve_sandboxed_path(path) {
            self.inner.is_dir(&resolved)
        } else {
            false
        }
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, HclError> {
        let resolved = self.resolve_sandboxed_path(path)?;
        let canonical = self.inner.canonicalize(&resolved)?;
        if !canonical.starts_with(&self.root) {
            return Err(HclError::FileSystem(format!(
                "Path traversal attack detected: canonical path '{}' escapes sandbox root '{}'",
                canonical.display(),
                self.root.display()
            )));
        }
        Ok(canonical)
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, HclError> {
        let resolved = self.resolve_sandboxed_path(path)?;
        let entries = self.inner.read_dir(&resolved)?;
        let mut safe_entries = Vec::new();
        for entry in entries {
            if entry.starts_with(&self.root) {
                safe_entries.push(entry);
            }
        }
        Ok(safe_entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_os_filesystem_basic() {
        let fs = OsFileSystem::new();
        let fs_def = OsFileSystem;
        assert_eq!(fs, fs_def);
        let fs_clone = fs;
        assert_eq!(fs, fs_clone);
        assert_eq!(format!("{fs:?}"), "OsFileSystem");

        assert!(fs.exists(Path::new("Cargo.toml")));
        assert!(fs.is_file(Path::new("Cargo.toml")));
        assert!(fs.is_dir(Path::new("src")));
        assert!(!fs.is_file(Path::new("src")));

        let content = fs.read_to_string(Path::new("Cargo.toml"));
        assert!(
            content
                .as_ref()
                .is_ok_and(|c| c.contains("hashicorp-configuration-language-rs"))
        );

        let bytes = fs.read(Path::new("Cargo.toml"));
        assert_eq!(
            bytes.ok().as_deref(),
            content.ok().as_deref().map(str::as_bytes)
        );

        let canon = fs.canonicalize(Path::new("Cargo.toml"));
        assert!(canon.is_ok_and(|c| c.is_absolute()));

        let entries = fs.read_dir(Path::new("src"));
        assert!(entries.is_ok_and(|e| !e.is_empty()));

        assert!(
            fs.read(Path::new("definitely_nonexistent_file_xyz.txt"))
                .is_err()
        );
        assert!(
            fs.read_to_string(Path::new("definitely_nonexistent_file_xyz.txt"))
                .is_err()
        );
        assert!(
            fs.read_dir(Path::new("definitely_nonexistent_dir_xyz"))
                .is_err()
        );
        assert!(
            fs.canonicalize(Path::new("definitely_nonexistent_file_xyz.txt"))
                .is_err()
        );
    }

    #[test]
    fn test_mem_filesystem_operations() {
        let mut fs = MemFileSystem::new();
        let fs_def = MemFileSystem::default();
        assert!(format!("{fs_def:?}").contains("MemFileSystem"));

        fs.insert_file("a/b/c.txt", "hello c");
        fs.add_file("a/d.txt", "hello d");

        assert!(fs.exists(Path::new("a/b/c.txt")));
        assert!(fs.is_file(Path::new("a/b/c.txt")));
        assert!(!fs.is_dir(Path::new("a/b/c.txt")));

        assert!(fs.exists(Path::new("a")));
        assert!(fs.is_dir(Path::new("a")));
        assert!(!fs.is_file(Path::new("a")));

        assert!(fs.exists(Path::new("a/b")));
        assert!(fs.is_dir(Path::new("a/b")));

        let c_content = fs.read_to_string(Path::new("a/b/c.txt"));
        assert_eq!(c_content.ok().as_deref(), Some("hello c"));

        let c_bytes = fs.read(Path::new("a/b/c.txt"));
        assert_eq!(c_bytes.ok().as_deref(), Some(b"hello c".as_slice()));

        let entries_a = fs.read_dir(Path::new("a"));
        assert_eq!(entries_a.ok().as_ref().map(Vec::len), Some(2)); // a/b and a/d.txt

        let entries_root = fs.read_dir(Path::new("."));
        assert_eq!(entries_root.ok().as_ref().map(Vec::len), Some(1)); // a

        let canon = fs.canonicalize(Path::new("a/b/c.txt"));
        assert_eq!(canon.ok(), Some(PathBuf::from("a/b/c.txt")));

        assert!(fs.read(Path::new("nonexistent.txt")).is_err());
        assert!(fs.read_to_string(Path::new("nonexistent.txt")).is_err());
        assert!(fs.read_dir(Path::new("nonexistent_dir")).is_err());
        assert!(fs.canonicalize(Path::new("nonexistent.txt")).is_err());
    }

    #[test]
    fn test_mem_filesystem_edge_cases() {
        let mut fs = MemFileSystem::new();

        // Invalid UTF-8 handling
        fs.insert_file("invalid.bin", vec![0xFF, 0xFE, 0xFD]);
        assert!(fs.read_to_string(Path::new("invalid.bin")).is_err());
        assert_eq!(
            fs.read(Path::new("invalid.bin")).ok(),
            Some(vec![0xFF, 0xFE, 0xFD])
        );

        // Path normalization branches
        fs.insert_file("./x/./y/../z.txt", "normalized");
        assert!(fs.is_file(Path::new("x/z.txt")));
        assert_eq!(
            fs.read_to_string(Path::new("x/z.txt")).ok().as_deref(),
            Some("normalized")
        );

        // Relative parent dirs
        fs.insert_file("../outside.txt", "parent relative");
        assert!(fs.is_file(Path::new("../outside.txt")));

        // Multiple parent dirs
        fs.insert_file("../../deep.txt", "deep parent");
        assert!(fs.is_file(Path::new("../../deep.txt")));

        // Absolute path
        fs.insert_file("/root/file.txt", "abs");
        assert!(fs.is_file(Path::new("/root/file.txt")));

        // Absolute path with parent dir beyond root
        fs.insert_file("/../root2/file.txt", "abs root");
        assert!(fs.is_file(Path::new("/root2/file.txt")));

        // Empty path normalization
        assert_eq!(normalize_path(Path::new("")), PathBuf::from("."));
        assert_eq!(normalize_path(Path::new("a/..")), PathBuf::from("."));

        // read_dir on empty/root/dot
        let empty_fs = MemFileSystem::new();
        assert_eq!(empty_fs.read_dir(Path::new(".")), Ok(vec![]));
    }

    #[test]
    fn test_mem_filesystem_poisoning() {
        let fs = MemFileSystem::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = fs.files.write();
            panic!("force rwlock poison for coverage testing")
        }));

        assert!(!fs.is_file(Path::new("any.txt")));
        assert!(!fs.is_dir(Path::new("any_dir")));
        assert!(fs.read(Path::new("any.txt")).is_err());
        assert!(fs.read_dir(Path::new("any_dir")).is_err());

        fs.add_file("any.txt", "data");
        let mut fs_mut = fs;
        fs_mut.insert_file("any.txt", "data");
    }

    #[test]
    fn test_archive_filesystem_zip() {
        use std::io::Write as IoWrite;

        // Build in-memory ZIP
        let mut zip_buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut zip_buf);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("templates/hello.tftpl", options)
                .expect("start file");
            writer.write_all(b"Hello, ${name}!").expect("write file");
            writer
                .start_file("configs/app.hcl", options)
                .expect("start file");
            writer.write_all(b"port = 8080\n").expect("write file");
            writer
                .start_file("binary.bin", options)
                .expect("start file");
            writer.write_all(&[0xFF, 0xFE, 0xFD]).expect("write bin");
            writer.add_directory("empty_dir", options).expect("add dir");
            writer.finish().expect("finish zip");
        }

        let zip_bytes = zip_buf.into_inner();
        let archive_fs = ArchiveFileSystem::from_zip_bytes(&zip_bytes).expect("load zip");
        let archive_fs_stream =
            ArchiveFileSystem::from_zip(std::io::Cursor::new(zip_bytes.as_slice()))
                .expect("load zip stream");
        assert_eq!(archive_fs.files.len(), archive_fs_stream.files.len());

        let archive_def = ArchiveFileSystem::default();
        assert!(format!("{archive_def:?}").contains("ArchiveFileSystem"));
        assert!(!archive_def.is_dir(Path::new(".")));

        // Invalid zip bytes error
        assert!(ArchiveFileSystem::from_zip_bytes(b"not a valid zip").is_err());

        // Corrupted zip entry data with valid central directory
        let mut corrupt_buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut corrupt_buf);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer
                .start_file("corrupt.txt", options)
                .expect("start corrupt");
            writer
                .write_all(b"content to compress and corrupt for decompression error testing")
                .expect("write corrupt");
            writer.finish().expect("finish corrupt");
        }
        let mut corrupt_bytes = corrupt_buf.into_inner();
        for b in corrupt_bytes.iter_mut().skip(45).take(20) {
            *b ^= 0xFF;
        }
        assert!(
            ArchiveFileSystem::from_zip(std::io::Cursor::new(corrupt_bytes.as_slice())).is_err()
        );

        // Unsupported compression method triggers archive.by_index error path
        let mut unsupp_buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut unsupp_buf);
            let options = zip::write::SimpleFileOptions::default();
            writer
                .start_file("unsupported.txt", options)
                .expect("start stored");
            writer.write_all(b"plain data").expect("write stored");
            writer.finish().expect("finish stored");
        }
        let mut unsupp_bytes = unsupp_buf.into_inner();
        let cdfh_pos = unsupp_bytes
            .windows(4)
            .position(|w| w == b"PK\x01\x02")
            .expect("find cdfh");
        unsupp_bytes[8] = 12;
        unsupp_bytes[9] = 0;
        unsupp_bytes[cdfh_pos + 10] = 12;
        unsupp_bytes[cdfh_pos + 11] = 0;
        let err_res = ArchiveFileSystem::from_zip(std::io::Cursor::new(unsupp_bytes.as_slice()));
        assert!(err_res.is_err());

        // File and directory existence checks
        assert!(archive_fs.exists(Path::new("configs/app.hcl")));
        assert!(archive_fs.is_file(Path::new("configs/app.hcl")));
        assert!(!archive_fs.is_dir(Path::new("configs/app.hcl")));

        assert!(archive_fs.exists(Path::new(".")));
        assert!(archive_fs.is_dir(Path::new(".")));
        assert!(!archive_fs.is_file(Path::new(".")));

        assert!(archive_fs.exists(Path::new("configs")));
        assert!(archive_fs.is_dir(Path::new("configs")));
        assert!(!archive_fs.is_file(Path::new("configs")));

        assert!(archive_fs.is_dir(Path::new("empty_dir")));
        assert!(!archive_fs.exists(Path::new("nonexistent")));
        assert!(!archive_fs.is_file(Path::new("nonexistent")));
        assert!(!archive_fs.is_dir(Path::new("nonexistent")));

        // Reading files
        assert_eq!(
            archive_fs
                .read_to_string(Path::new("configs/app.hcl"))
                .as_deref(),
            Ok("port = 8080\n")
        );
        assert!(archive_fs.read_to_string(Path::new("binary.bin")).is_err());
        assert!(archive_fs.read_to_string(Path::new("nonexistent")).is_err());

        // Canonicalization
        assert!(
            archive_fs
                .canonicalize(Path::new("configs/app.hcl"))
                .is_ok()
        );
        assert!(archive_fs.canonicalize(Path::new("nonexistent")).is_err());

        // read_dir
        assert_eq!(
            archive_fs.read_dir(Path::new("configs")).map(|v| v.len()),
            Ok(1)
        );
        assert!(
            archive_fs
                .read_dir(Path::new("."))
                .map(|v| v.len())
                .is_ok_and(|len| len >= 3)
        );
        assert!(archive_fs.read_dir(Path::new("nonexistent")).is_err());

        // Integration with standard library functions
        let ctx = crate::eval::context::Context::with_stdlib()
            .with_filesystem(std::sync::Arc::new(archive_fs));

        let tmpl_hcl = r#"res = templatefile("templates/hello.tftpl", { "name" = "World" })"#;
        let body = crate::api::parse(tmpl_hcl).expect("parse template");
        let eval = crate::eval::evaluator::Evaluator::new(&ctx);
        let (val, diags) = eval.evaluate(&body.attributes["res"].expr).expect("eval");
        assert!(!diags.has_errors());
        assert_eq!(val.to_string(), "\"Hello, World!\"");

        let file_hcl = r#"res = file("configs/app.hcl")"#;
        let body_f = crate::api::parse(file_hcl).expect("parse file");
        let (val_f, _) = crate::eval::evaluator::Evaluator::new(&ctx)
            .evaluate(&body_f.attributes["res"].expr)
            .expect("eval file");
        assert_eq!(val_f.to_string(), "\"port = 8080\n\"");
    }

    #[test]
    fn test_archive_filesystem_tar_and_tar_gz() {
        // 1. Build TAR with directory and nested files
        let mut tar_builder = tar::Builder::new(Vec::new());

        // Directory entry
        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_size(0);
        dir_header.set_mode(0o755);
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_cksum();
        tar_builder
            .append_data(&mut dir_header, "tar_dir", std::io::empty())
            .expect("append dir");

        // Top level file
        let mut header = tar::Header::new_gnu();
        header.set_size(11);
        header.set_mode(0o644);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, "hello.txt", "hello world".as_bytes())
            .expect("append file");

        // Nested file
        let mut nested_header = tar::Header::new_gnu();
        nested_header.set_size(4);
        nested_header.set_mode(0o644);
        nested_header.set_cksum();
        tar_builder
            .append_data(&mut nested_header, "nested/sub/data.txt", &b"data"[..])
            .expect("append nested");

        // Invalid UTF-8 file
        let mut bad_header = tar::Header::new_gnu();
        bad_header.set_size(3);
        bad_header.set_mode(0o644);
        bad_header.set_cksum();
        tar_builder
            .append_data(&mut bad_header, "bad.bin", &[0xFF, 0xFE, 0xFD][..])
            .expect("append bad");

        // Symlink entry (neither dir nor regular file)
        let mut symlink_header = tar::Header::new_gnu();
        symlink_header.set_size(0);
        symlink_header.set_mode(0o777);
        symlink_header.set_entry_type(tar::EntryType::Symlink);
        symlink_header.set_cksum();
        tar_builder
            .append_data(&mut symlink_header, "symlink", std::io::empty())
            .expect("append symlink");

        let tar_bytes = tar_builder.into_inner().expect("into_inner tar");

        let tar_fs = ArchiveFileSystem::from_tar_bytes(&tar_bytes).expect("load tar");
        let tar_fs_stream =
            ArchiveFileSystem::from_tar(tar_bytes.as_slice()).expect("load tar stream");
        assert_eq!(tar_fs.files.len(), tar_fs_stream.files.len());

        assert!(ArchiveFileSystem::from_tar_bytes(b"invalid tar data").is_err());
        let truncated_tar = &tar_bytes[..tar_bytes.len().saturating_sub(600)];
        assert!(ArchiveFileSystem::from_tar(truncated_tar).is_err());

        // Reader that fails when reading content after header
        struct FailingContentReader {
            header: Vec<u8>,
            pos: usize,
        }

        impl std::io::Read for FailingContentReader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.pos < self.header.len() {
                    let n = std::cmp::min(buf.len(), self.header.len() - self.pos);
                    buf[..n].copy_from_slice(&self.header[self.pos..self.pos + n]);
                    self.pos += n;
                    Ok(n)
                } else {
                    Err(std::io::Error::other("failing content read"))
                }
            }
        }

        let mut truncated_header = tar::Header::new_gnu();
        truncated_header.set_size(100);
        truncated_header.set_mode(0o644);
        truncated_header.set_cksum();
        let mut fail_reader = FailingContentReader {
            header: truncated_header.as_bytes().to_vec(),
            pos: 0,
        };
        assert!(ArchiveFileSystem::from_tar(&mut fail_reader).is_err());

        assert_eq!(
            tar_fs.read_to_string(Path::new("hello.txt")).as_deref(),
            Ok("hello world")
        );
        assert!(tar_fs.read_to_string(Path::new("bad.bin")).is_err());
        assert!(tar_fs.is_dir(Path::new("tar_dir")));
        assert!(tar_fs.is_dir(Path::new("nested")));
        assert!(tar_fs.is_dir(Path::new("nested/sub")));
        assert!(tar_fs.is_file(Path::new("nested/sub/data.txt")));

        // 2. Build TAR.GZ
        let mut gz_buf = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::default());
            let mut gz_builder = tar::Builder::new(encoder);
            let mut gz_header = tar::Header::new_gnu();
            gz_header.set_size(9);
            gz_header.set_mode(0o644);
            gz_header.set_cksum();
            gz_builder
                .append_data(&mut gz_header, "test.txt", "gzip test".as_bytes())
                .expect("append gz");
            let encoder = gz_builder.into_inner().expect("into_inner gz");
            encoder.finish().expect("finish gz");
        }

        let gz_fs = ArchiveFileSystem::from_tar_gz_bytes(&gz_buf).expect("load tar.gz");
        let gz_fs_stream =
            ArchiveFileSystem::from_tar_gz(gz_buf.as_slice()).expect("load tar.gz stream");
        assert_eq!(gz_fs.files.len(), gz_fs_stream.files.len());

        assert!(ArchiveFileSystem::from_tar_gz_bytes(b"invalid gz data").is_err());
        assert_eq!(
            gz_fs.read_to_string(Path::new("test.txt")).as_deref(),
            Ok("gzip test")
        );
    }

    /// Mock filesystem for simulating canonicalization and `read_dir` escape attempts.
    #[derive(Debug)]
    struct MockEscapingFs {
        inner: MemFileSystem,
    }

    impl FileSystem for MockEscapingFs {
        fn read(&self, p: &Path) -> Result<Vec<u8>, HclError> {
            self.inner.read(p)
        }

        fn read_to_string(&self, p: &Path) -> Result<String, HclError> {
            self.inner.read_to_string(p)
        }

        fn exists(&self, p: &Path) -> bool {
            self.inner.exists(p)
        }

        fn is_file(&self, p: &Path) -> bool {
            self.inner.is_file(p)
        }

        fn is_dir(&self, p: &Path) -> bool {
            self.inner.is_dir(p)
        }

        fn canonicalize(&self, _p: &Path) -> Result<PathBuf, HclError> {
            Ok(PathBuf::from("/escaped/outside/path"))
        }

        fn read_dir(&self, p: &Path) -> Result<Vec<PathBuf>, HclError> {
            let mut entries = self.inner.read_dir(p)?;
            entries.push(PathBuf::from("/escaped/outside/entry"));
            Ok(entries)
        }
    }

    #[test]
    fn test_sandboxed_filesystem_confinement_and_traversal_rejection() {
        let mut mem_fs = MemFileSystem::new();
        mem_fs.insert_file("jail/allowed.txt", "inside jail");
        mem_fs.insert_file("jail/sub/child.txt", "child in jail");
        mem_fs.insert_file("secret.txt", "outside jail");

        let inner = std::sync::Arc::new(mem_fs);

        // Fail to init with nonexistent root
        assert!(SandboxedFileSystem::new("nonexistent_root_dir", inner.clone()).is_err());

        let sandbox = SandboxedFileSystem::new("jail", inner.clone()).expect("sandbox init");
        let sandbox_clone = sandbox.clone();
        assert!(format!("{sandbox_clone:?}").contains("SandboxedFileSystem"));

        // Allowed accesses
        assert!(sandbox.exists(Path::new("allowed.txt")));
        assert!(sandbox.is_file(Path::new("allowed.txt")));
        assert_eq!(
            sandbox.read_to_string(Path::new("allowed.txt")).as_deref(),
            Ok("inside jail")
        );
        assert_eq!(
            sandbox.read(Path::new("allowed.txt")).as_deref(),
            Ok(b"inside jail".as_slice())
        );
        assert!(sandbox.is_dir(Path::new("sub")));
        assert_eq!(sandbox.read_dir(Path::new("sub")).map(|v| v.len()), Ok(1));

        // CurDir, RootDir, ParentDir within bounds
        assert_eq!(
            sandbox
                .read_to_string(Path::new("./sub/../allowed.txt"))
                .as_deref(),
            Ok("inside jail")
        );
        assert_eq!(
            sandbox.read_to_string(Path::new("/allowed.txt")).as_deref(),
            Ok("inside jail")
        );

        // Canonicalize allowed
        assert!(sandbox.canonicalize(Path::new("allowed.txt")).is_ok());
        assert!(
            sandbox
                .canonicalize(Path::new("nonexistent_file.txt"))
                .is_err()
        );

        // Rejected path traversal accesses
        assert!(sandbox.read(Path::new("../secret.txt")).is_err());
        assert!(sandbox.read_to_string(Path::new("../secret.txt")).is_err());
        assert!(!sandbox.exists(Path::new("../secret.txt")));
        assert!(!sandbox.is_file(Path::new("../secret.txt")));
        assert!(!sandbox.is_dir(Path::new("../secret.txt")));
        assert!(sandbox.canonicalize(Path::new("../secret.txt")).is_err());
        assert!(sandbox.read_dir(Path::new("../")).is_err());
        assert!(sandbox.read_dir(Path::new("nonexistent_sub")).is_err());

        // Escaping filesystem test
        let mut escaping_mem = MemFileSystem::new();
        escaping_mem.insert_file("jail/file.txt", "content");
        let escaping_fs = MockEscapingFs {
            inner: escaping_mem,
        };
        let escaping_sandbox = SandboxedFileSystem {
            root: PathBuf::from("jail"),
            inner: std::sync::Arc::new(escaping_fs),
        };

        // Escaping canonicalize fails
        assert!(
            escaping_sandbox
                .canonicalize(Path::new("file.txt"))
                .is_err()
        );

        // Escaping read_dir error propagation
        assert!(
            escaping_sandbox
                .read_dir(Path::new("nonexistent_sub"))
                .is_err()
        );

        // Escaping entries in read_dir are filtered out
        let safe = escaping_sandbox.read_dir(Path::new(".")).expect("read dir");
        assert_eq!(safe, vec![PathBuf::from("jail/file.txt")]);

        // Exercise remaining pass-through methods on MockEscapingFs
        assert!(escaping_sandbox.exists(Path::new("file.txt")));
        assert!(escaping_sandbox.is_file(Path::new("file.txt")));
        assert!(escaping_sandbox.is_dir(Path::new(".")));
        assert_eq!(
            escaping_sandbox
                .read_to_string(Path::new("file.txt"))
                .as_deref(),
            Ok("content")
        );
        assert_eq!(
            escaping_sandbox.read(Path::new("file.txt")).as_deref(),
            Ok(b"content".as_slice())
        );
    }
}

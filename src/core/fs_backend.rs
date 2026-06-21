use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::core::config::ServerConfig;
use crate::core::http::percent_decode_path;

#[derive(Debug, Clone)]
pub struct FileSystemBackend {
    root: PathBuf,
    hide_dotfiles: bool,
    follow_symlinks: bool,
}

#[derive(Debug, Clone)]
pub struct FsEntry {
    pub name: String,
    pub metadata: fs::Metadata,
}

impl FileSystemBackend {
    pub fn new(root: PathBuf, config: &ServerConfig) -> Self {
        Self {
            root,
            hide_dotfiles: config.hide_dotfiles,
            follow_symlinks: config.follow_symlinks,
        }
    }

    pub fn assert_root(&self) -> io::Result<()> {
        let metadata = fs::metadata(&self.root)?;
        if metadata.is_dir() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Not a directory: {}", self.root.display()),
            ))
        }
    }

    pub fn resolve(&self, request_path: &str) -> io::Result<PathBuf> {
        let decoded = percent_decode_path(request_path)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let relative = sanitize_relative_path(&decoded)?;

        if self.hide_dotfiles && relative.components().any(is_dot_component) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Hidden files are not exposed",
            ));
        }

        let candidate = self.root.join(&relative);
        let root = self.root.canonicalize()?;
        let parent = candidate.parent().unwrap_or(&self.root);
        let canonical_parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());

        if canonical_parent != root && !canonical_parent.starts_with(&root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Path escapes shared root",
            ));
        }

        if !self.follow_symlinks {
            self.reject_symlink_segments(&relative)?;
        }
        Ok(candidate)
    }

    pub fn metadata(&self, request_path: &str) -> io::Result<(PathBuf, fs::Metadata)> {
        let path = self.resolve(request_path)?;
        let metadata = fs::metadata(&path)?;
        Ok((path, metadata))
    }

    pub fn list(&self, request_path: &str) -> io::Result<Vec<FsEntry>> {
        let (path, _) = self.metadata(request_path)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if self.hide_dotfiles && name.starts_with('.') {
                continue;
            }
            entries.push(FsEntry {
                name,
                metadata: entry.metadata()?,
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    pub fn remove(&self, request_path: &str) -> io::Result<()> {
        let path = self.resolve(request_path)?;
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }

    fn reject_symlink_segments(&self, relative: &Path) -> io::Result<()> {
        let mut current = self.root.clone();
        for component in relative.components() {
            current.push(component.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Symlinks are disabled",
                    ));
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
}

fn sanitize_relative_path(input: &str) -> io::Result<PathBuf> {
    let mut out = PathBuf::new();
    for component in Path::new(input).components() {
        match component {
            Component::Normal(value) => out.push(value),
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Path escapes shared root",
                ));
            }
        }
    }
    Ok(out)
}

fn is_dot_component(component: Component<'_>) -> bool {
    matches!(component, Component::Normal(value) if value.to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::core::config::ServerConfig;

    use super::FileSystemBackend;

    #[test]
    fn rejects_parent_traversal() {
        let root = std::env::temp_dir().join(format!("davbox-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let backend = FileSystemBackend::new(root.clone(), &ServerConfig::default());
        assert!(backend.resolve("/../secret").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hides_dotfiles() {
        let root = std::env::temp_dir().join(format!("davbox-dot-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".env"), "secret").unwrap();
        let backend = FileSystemBackend::new(root.clone(), &ServerConfig::default());
        assert!(backend.resolve("/.env").is_err());
        let _ = fs::remove_dir_all(root);
    }
}

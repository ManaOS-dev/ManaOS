//! Virtual filesystem namespace state.

use super::directory::DirectoryNode;
use super::mount::{MountFlags, MountInfo, MountSource};
use super::node::{
    normalize_path, DirectoryEntry, FileMetadata, FileNode, FileSystemError, FileSystemResult,
    FileType,
};
use super::{device, RamFile};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

pub(super) struct VirtualFileSystem {
    nodes: BTreeMap<String, Arc<dyn FileNode>>,
    mounts: Vec<MountInfo>,
    initialized: bool,
}

impl VirtualFileSystem {
    pub(super) fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            mounts: Vec::new(),
            initialized: false,
        }
    }

    pub(super) fn initialize(&mut self) -> FileSystemResult<()> {
        if self.initialized {
            return Err(FileSystemError::AlreadyInitialized);
        }

        self.mount_directory("/", MountSource::Ram, MountFlags::read_write());
        self.mount_directory("/dev", MountSource::Device, MountFlags::read_write());
        self.mount_node(
            "/dev/console",
            Arc::new(device::ConsoleDevice::new()),
            MountSource::Device,
            MountFlags::read_write(),
        );
        self.mount_node(
            "/dev/keyboard",
            Arc::new(device::KeyboardInputDevice::new()),
            MountSource::Device,
            MountFlags::read_write(),
        );
        self.mount_node(
            "/dev/null",
            Arc::new(device::NullDevice::new()),
            MountSource::Device,
            MountFlags::read_write(),
        );
        self.mount_node(
            "/README",
            Arc::new(RamFile::from_bytes(b"ManaOS ramfs is initialized.\n")),
            MountSource::Ram,
            MountFlags::read_write(),
        );
        self.refresh_directories();
        self.initialized = true;
        Ok(())
    }

    pub(super) fn mount_node(
        &mut self,
        path: &str,
        node: Arc<dyn FileNode>,
        source: MountSource,
        flags: MountFlags,
    ) {
        let path = normalize_path(path);
        self.ensure_parent_directories(&path, source, flags);
        self.nodes.insert(path.clone(), node);
        self.upsert_mount(path, source, flags);
        self.refresh_directories();
    }

    pub(super) fn get_node(&self, path: &str) -> FileSystemResult<Arc<dyn FileNode>> {
        if path.as_bytes().contains(&0) {
            return Err(FileSystemError::InvalidPath);
        }

        self.nodes
            .get(&normalize_path(path))
            .cloned()
            .ok_or(FileSystemError::NotFound)
    }

    pub(super) fn metadata(&self, path: &str) -> FileSystemResult<FileMetadata> {
        Ok(self.get_node(path)?.metadata())
    }

    pub(super) fn list_directory(&self, path: &str) -> FileSystemResult<Vec<DirectoryEntry>> {
        self.get_node(path)?.list_entries()
    }

    pub(super) fn list_mounts(&self) -> Vec<MountInfo> {
        self.mounts.clone()
    }

    fn mount_directory(&mut self, path: &str, source: MountSource, flags: MountFlags) {
        self.mount_node(path, Arc::new(DirectoryNode::empty()), source, flags);
    }

    fn upsert_mount(&mut self, path: String, source: MountSource, flags: MountFlags) {
        if let Some(mount) = self.mounts.iter_mut().find(|mount| mount.path == path) {
            mount.source = source;
            mount.flags = flags;
            return;
        }

        self.mounts.push(MountInfo {
            path,
            source,
            flags,
        });
    }

    fn ensure_parent_directories(&mut self, path: &str, source: MountSource, flags: MountFlags) {
        let mut current = String::new();
        let mut parents = Vec::new();
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            let next = if current.is_empty() {
                format!("/{segment}")
            } else {
                format!("{current}/{segment}")
            };
            parents.push(current_path_parent(&next));
            current = next;
        }

        for parent in parents {
            if !self.nodes.contains_key(&parent) {
                self.nodes
                    .insert(parent.clone(), Arc::new(DirectoryNode::empty()));
                self.upsert_mount(parent, source, flags);
            }
        }
    }

    fn refresh_directories(&mut self) {
        let directory_paths: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|(path, node)| {
                if node.metadata().file_type == FileType::Directory {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect();

        for directory_path in directory_paths {
            let entries = self.collect_directory_entries(&directory_path);
            let directory_node = DirectoryNode::empty();
            directory_node.set_entries(entries);
            self.nodes.insert(directory_path, Arc::new(directory_node));
        }
    }

    fn collect_directory_entries(&self, directory_path: &str) -> Vec<DirectoryEntry> {
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        for (path, node) in &self.nodes {
            if path == directory_path {
                continue;
            }
            if let Some(name) = direct_child_name(directory_path, path) {
                if seen.insert(name.clone()) {
                    entries.push(DirectoryEntry {
                        name,
                        metadata: node.metadata(),
                    });
                }
            }
        }
        entries
    }
}

fn current_path_parent(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }

    match path.rsplit_once('/') {
        Some(("", _)) | None => String::from("/"),
        Some((parent, _)) => String::from(parent),
    }
}

fn direct_child_name(directory_path: &str, path: &str) -> Option<String> {
    let directory_path = normalize_path(directory_path);
    let path = normalize_path(path);
    let remainder = if directory_path == "/" {
        path.strip_prefix('/')?
    } else {
        path.strip_prefix(&format!("{directory_path}/"))?
    };
    if remainder.is_empty() || remainder.contains('/') {
        return None;
    }

    Some(remainder.to_string())
}

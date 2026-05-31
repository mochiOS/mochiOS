//! InitFs（フォールバック用の簡易ファイルシステム）
//!
//! ext2 が利用できない環境でも最低限の動作確認ができるように、
//! メモリ上に小さなツリーを持つだけの簡易FSを実装する。

use std::collections::BTreeMap;
use std::string::String;
use std::vec::Vec;

use crate::common::FileSystem;
use crate::common::vfs::{DirEntry, FileAttr, FileType, VfsError, VfsResult};

#[derive(Clone)]
struct Node {
    inode: u64,
    file_type: FileType,
    mode: u16,
    data: Vec<u8>,
    children: BTreeMap<String, u64>,
}

/// メモリ上の簡易 InitFs
pub struct InitFs {
    nodes: BTreeMap<u64, Node>,
    next_inode: u64,
}

impl InitFs {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            1,
            Node {
                inode: 1,
                file_type: FileType::Directory,
                mode: 0o755,
                data: Vec::new(),
                children: BTreeMap::new(),
            },
        );
        Self {
            nodes,
            next_inode: 2,
        }
    }

    /// デモ用のサンプルファイルを作成する
    pub fn create_sample_files(&mut self) -> VfsResult<()> {
        let root = self.root_inode();
        let hello_inode = self.create(root, "hello.txt", 0o644)?;
        let msg = b"mochiOS InitFs fallback\n";
        let _ = self.write(hello_inode, 0, msg)?;
        Ok(())
    }

    fn alloc_inode(&mut self) -> u64 {
        let inode = self.next_inode;
        self.next_inode += 1;
        inode
    }

    fn get_node(&self, inode: u64) -> VfsResult<&Node> {
        self.nodes.get(&inode).ok_or(VfsError::NotFound)
    }

    fn get_node_mut(&mut self, inode: u64) -> VfsResult<&mut Node> {
        self.nodes.get_mut(&inode).ok_or(VfsError::NotFound)
    }
}

impl FileSystem for InitFs {
    fn name(&self) -> &str {
        "initfs"
    }

    fn root_inode(&self) -> u64 {
        1
    }

    fn stat(&self, inode: u64) -> VfsResult<FileAttr> {
        let n = self.get_node(inode)?;
        Ok(FileAttr {
            file_type: n.file_type,
            size: n.data.len() as u64,
            blocks: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
            mode: n.mode,
            uid: 0,
            gid: 0,
            nlink: 1,
        })
    }

    fn lookup(&self, parent_inode: u64, name: &str) -> VfsResult<u64> {
        let parent = self.get_node(parent_inode)?;
        if parent.file_type != FileType::Directory {
            return Err(VfsError::NotDirectory);
        }
        parent
            .children
            .get(name)
            .copied()
            .ok_or(VfsError::NotFound)
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let n = self.get_node(inode)?;
        if n.file_type != FileType::RegularFile {
            return Err(VfsError::InvalidArgument);
        }
        let off = offset as usize;
        if off >= n.data.len() {
            return Ok(0);
        }
        let nread = std::cmp::min(buf.len(), n.data.len() - off);
        buf[..nread].copy_from_slice(&n.data[off..off + nread]);
        Ok(nread)
    }

    fn write(&mut self, inode: u64, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        let n = self.get_node_mut(inode)?;
        if n.file_type != FileType::RegularFile {
            return Err(VfsError::InvalidArgument);
        }
        let off = offset as usize;
        let end = off.saturating_add(buf.len());
        if end > n.data.len() {
            n.data.resize(end, 0);
        }
        n.data[off..end].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn readdir(&self, inode: u64) -> VfsResult<Vec<DirEntry>> {
        let n = self.get_node(inode)?;
        if n.file_type != FileType::Directory {
            return Err(VfsError::NotDirectory);
        }
        let mut out = Vec::new();
        for (name, child) in &n.children {
            let child_node = self.get_node(*child)?;
            out.push(DirEntry {
                name: name.clone(),
                inode: *child,
                file_type: child_node.file_type,
            });
        }
        Ok(out)
    }

    fn create(&mut self, parent_inode: u64, name: &str, mode: u16) -> VfsResult<u64> {
        let inode = self.alloc_inode();
        self.nodes.insert(
            inode,
            Node {
                inode,
                file_type: FileType::RegularFile,
                mode,
                data: Vec::new(),
                children: BTreeMap::new(),
            },
        );
        {
            let parent = self.get_node_mut(parent_inode)?;
            if parent.file_type != FileType::Directory {
                return Err(VfsError::NotDirectory);
            }
            if parent.children.contains_key(name) {
                return Err(VfsError::AlreadyExists);
            }
            parent.children.insert(name.to_string(), inode);
        }
        Ok(inode)
    }

    fn mkdir(&mut self, parent_inode: u64, name: &str, mode: u16) -> VfsResult<u64> {
        let inode = self.alloc_inode();
        self.nodes.insert(
            inode,
            Node {
                inode,
                file_type: FileType::Directory,
                mode,
                data: Vec::new(),
                children: BTreeMap::new(),
            },
        );
        {
            let parent = self.get_node_mut(parent_inode)?;
            if parent.file_type != FileType::Directory {
                return Err(VfsError::NotDirectory);
            }
            if parent.children.contains_key(name) {
                return Err(VfsError::AlreadyExists);
            }
            parent.children.insert(name.to_string(), inode);
        }
        Ok(inode)
    }

    fn unlink(&mut self, parent_inode: u64, name: &str) -> VfsResult<()> {
        let parent = self.get_node_mut(parent_inode)?;
        if parent.file_type != FileType::Directory {
            return Err(VfsError::NotDirectory);
        }
        let inode = parent.children.remove(name).ok_or(VfsError::NotFound)?;
        self.nodes.remove(&inode);
        Ok(())
    }

    fn rmdir(&mut self, parent_inode: u64, name: &str) -> VfsResult<()> {
        self.unlink(parent_inode, name)
    }

    fn truncate(&mut self, inode: u64, size: u64) -> VfsResult<()> {
        let n = self.get_node_mut(inode)?;
        if n.file_type != FileType::RegularFile {
            return Err(VfsError::InvalidArgument);
        }
        n.data.resize(size as usize, 0);
        Ok(())
    }

    fn sync(&mut self) -> VfsResult<()> {
        Ok(())
    }
}

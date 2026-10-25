use std::collections::BTreeSet;

pub type Id = u32;
const GIB: u64 = 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Disk,
    Partition,
    Volume,
    Unallocated,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disk => "Disk",
            Self::Partition => "Partition",
            Self::Volume => "Volume",
            Self::Unallocated => "Unallocated space",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Node {
    pub id: Id,
    pub parent: Option<Id>,
    pub name: String,
    pub kind: Kind,
    pub bytes: u64,
    pub used: Option<u64>,
    pub format: Option<&'static str>,
    pub mount: Option<&'static str>,
    pub read_only: bool,
    pub removable: bool,
}
impl Node {
    pub fn free(&self) -> Option<u64> {
        self.bytes.checked_sub(self.used?)
    }
}
pub struct Model {
    pub nodes: Vec<Node>,
    pub selected: Option<Id>,
    pub expanded: BTreeSet<Id>,
    pub preview: bool,
    pub detail_open: bool,
}
impl Model {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        if std::env::var("MOCHIOS_PREVIEW_DISK_UTILITY_DEMO").as_deref() != Ok("0") {
            return Self::sample();
        }
        Self {
            nodes: vec![],
            selected: None,
            expanded: BTreeSet::new(),
            preview: false,
            detail_open: false,
        }
    }
    pub fn node(&self, id: Id) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
    pub fn children(&self, id: Id) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(move |n| n.parent == Some(id))
    }
    pub fn select(&mut self, id: Id) -> bool {
        if self.node(id).is_none() {
            return false;
        }
        self.selected = Some(id);
        let mut parent = self.node(id).and_then(|n| n.parent);
        for _ in 0..self.nodes.len() {
            let Some(id) = parent else {
                break;
            };
            self.expanded.insert(id);
            parent = self.node(id).and_then(|n| n.parent);
        }
        self.detail_open = true;
        true
    }
    pub fn toggle(&mut self, id: Id) {
        if self.children(id).next().is_none() {
            return;
        }
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }
    pub fn visible(&self) -> Vec<(Id, usize)> {
        fn append(m: &Model, parent: Option<Id>, depth: usize, out: &mut Vec<(Id, usize)>) {
            for n in m.nodes.iter().filter(|n| n.parent == parent) {
                out.push((n.id, depth));
                if m.expanded.contains(&n.id) {
                    append(m, Some(n.id), depth + 1, out);
                }
            }
        }
        let mut result = vec![];
        append(self, None, 0, &mut result);
        result
    }
    pub fn disk_for(&self, mut id: Id) -> Option<&Node> {
        for _ in 0..self.nodes.len() {
            let n = self.node(id)?;
            match n.parent {
                Some(parent) => id = parent,
                None => return Some(n),
            }
        }
        None
    }
    pub fn refresh(&mut self) {
        if !self.preview {
            return;
        }
        let selected = self.selected;
        let expanded = self.expanded.clone();
        let detail = self.detail_open;
        let mut updated = Self::sample();
        updated.selected = selected.filter(|id| updated.node(*id).is_some());
        updated.expanded = expanded
            .into_iter()
            .filter(|id| updated.node(*id).is_some())
            .collect();
        updated.detail_open = detail && updated.selected.is_some();
        *self = updated;
    }
    // Fixtures belong to the preview only. Native startup never calls this path.
    #[cfg(any(target_os = "linux", test))]
    pub fn sample() -> Self {
        let mut nodes = vec![];
        let mut add =
            |id, parent, name: &str, kind, bytes, used, format, mount, read_only, removable| {
                nodes.push(Node {
                    id,
                    parent,
                    name: name.into(),
                    kind,
                    bytes,
                    used,
                    format,
                    mount,
                    read_only,
                    removable,
                });
            };
        add(
            1,
            None,
            "Preview SSD",
            Kind::Disk,
            512 * GIB,
            None,
            None,
            None,
            false,
            false,
        );
        for (id, name, size, used, format, mount, read_only) in [
            (10, "EFI", GIB / 2, GIB / 8, "FAT32", None, true),
            (
                20,
                "System A",
                32 * GIB,
                12 * GIB,
                "ext4",
                Some("/system"),
                true,
            ),
            (30, "System B", 32 * GIB, 10 * GIB, "ext4", None, true),
            (
                40,
                "Data",
                383 * GIB + GIB / 2,
                128 * GIB,
                "ext4",
                Some("/"),
                false,
            ),
        ] {
            add(
                id,
                Some(1),
                name,
                Kind::Partition,
                size,
                None,
                None,
                None,
                read_only,
                false,
            );
            add(
                id + 1,
                Some(id),
                name,
                Kind::Volume,
                size,
                Some(used),
                Some(format),
                mount,
                read_only,
                false,
            );
        }
        add(
            50,
            Some(1),
            "Unallocated",
            Kind::Unallocated,
            64 * GIB,
            None,
            None,
            None,
            false,
            false,
        );
        add(
            100,
            None,
            "Preview USB Drive",
            Kind::Disk,
            64 * GIB,
            None,
            None,
            None,
            false,
            true,
        );
        add(
            110,
            Some(100),
            "Projects",
            Kind::Partition,
            64 * GIB,
            None,
            None,
            None,
            false,
            true,
        );
        add(
            111,
            Some(110),
            "Projects",
            Kind::Volume,
            64 * GIB,
            Some(21 * GIB),
            Some("ext4"),
            Some("/volumes/Projects"),
            false,
            true,
        );
        Self {
            nodes,
            selected: Some(1),
            expanded: [1, 100].into_iter().collect(),
            preview: true,
            detail_open: false,
        }
    }
    #[cfg(not(any(target_os = "linux", test)))]
    fn sample() -> Self {
        unreachable!("preview fixtures are not available")
    }
}
pub fn bytes(value: u64) -> String {
    if value >= GIB {
        format!("{:.1} GiB", value as f64 / GIB as f64)
    } else if value >= 1024 * 1024 {
        format!("{:.0} MiB", value as f64 / (1024.0 * 1024.0))
    } else {
        format!("{value} bytes")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disk_segments_cover_capacity_without_using_volume_free_space() {
        let m = Model::sample();
        for disk in m.nodes.iter().filter(|n| n.kind == Kind::Disk) {
            assert_eq!(
                m.children(disk.id).map(|n| n.bytes).sum::<u64>(),
                disk.bytes
            );
            assert!(disk.free().is_none());
        }
        let data = m.node(41).unwrap();
        assert_eq!(data.free(), Some(data.bytes - 128 * GIB));
        assert_eq!(m.node(50).unwrap().kind, Kind::Unallocated);
    }
    #[test]
    fn tree_disclosure_and_refresh_keep_identity_and_selection() {
        let mut m = Model::sample();
        assert!(!m.visible().iter().any(|(id, _)| *id == 41));
        m.toggle(40);
        assert!(m.visible().contains(&(41, 2)));
        assert!(m.select(41));
        assert!(!m.select(999));
        m.refresh();
        assert_eq!(m.selected, Some(41));
        assert!(m.expanded.contains(&40));
        assert_eq!(m.disk_for(41).unwrap().id, 1);
        m.toggle(1);
        assert!(!m.visible().iter().any(|(id, _)| *id == 41));
        assert_eq!(m.selected, Some(41));
    }
    #[test]
    fn unknown_and_invalid_usage_never_looks_like_zero_free() {
        let mut n = Model::sample().node(41).unwrap().clone();
        n.used = None;
        assert_eq!(n.free(), None);
        n.used = Some(n.bytes + 1);
        assert_eq!(n.free(), None);
        n.bytes = 0;
        n.used = Some(0);
        assert_eq!(n.free(), Some(0));
    }
}

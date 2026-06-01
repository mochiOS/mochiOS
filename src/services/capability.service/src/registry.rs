use std::collections::BTreeSet;

/// registry: capabilities.toml に定義されている capability 名集合
#[derive(Clone, Debug)]
pub struct CapabilityRegistry {
    names: BTreeSet<String>,
}

impl CapabilityRegistry {
    pub fn load() -> Self {
        let text = include_str!("../resources/capabilities.toml");
        let mut set = BTreeSet::new();

        // 依存クレートを増やさないため、TOML を完全には解析せず、
        // セクションヘッダの `[capabilities.<name>]` だけを拾う。
        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with('[') || !line.ends_with(']') {
                continue;
            }
            let inside = &line[1..line.len() - 1];
            let Some(rest) = inside.strip_prefix("capabilities.") else {
                continue;
            };
            if !rest.is_empty() {
                set.insert(rest.to_string());
            }
        }

        Self { names: set }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}


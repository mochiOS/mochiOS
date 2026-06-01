use std::collections::{BTreeMap, BTreeSet};

use crate::protocol::SubjectType;

/// 許可DB（subject_id ごとの許可された capability）
///
/// ファイル形式（簡易）:
/// - `service:<id>=cap1,cap2,...`
/// - `app:<id>=cap1,cap2,...`
///
/// 例:
/// `service:net.service=net.raw,device.net`
#[derive(Clone, Debug, Default)]
pub struct AllowDb {
    services: BTreeMap<String, BTreeSet<String>>,
    apps: BTreeMap<String, BTreeSet<String>>,
}

impl AllowDb {
    pub fn load_from_config() -> Self {
        let text = match std::fs::read_to_string("/config/capabilities.db") {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };

        let mut services = BTreeMap::new();
        let mut apps = BTreeMap::new();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((lhs, rhs)) = line.split_once('=') else {
                continue;
            };
            let lhs = lhs.trim();
            let rhs = rhs.trim();
            let (kind, id) = if let Some(rest) = lhs.strip_prefix("service:") {
                ("service", rest.trim())
            } else if let Some(rest) = lhs.strip_prefix("app:") {
                ("app", rest.trim())
            } else {
                continue;
            };
            if id.is_empty() {
                continue;
            }

            let mut set = BTreeSet::new();
            for cap in rhs.split(',') {
                let cap = cap.trim();
                if !cap.is_empty() {
                    set.insert(cap.to_string());
                }
            }

            if kind == "service" {
                services.insert(id.to_string(), set);
            } else {
                apps.insert(id.to_string(), set);
            }
        }

        Self { services, apps }
    }

    pub fn services_len(&self) -> usize {
        self.services.len()
    }

    pub fn apps_len(&self) -> usize {
        self.apps.len()
    }

    pub fn allows(&self, subject_type: SubjectType, subject_id: &str, cap: &str) -> bool {
        match subject_type {
            SubjectType::Service => self
                .services
                .get(subject_id)
                .map(|s| s.contains(cap))
                .unwrap_or(false),
            SubjectType::App => self
                .apps
                .get(subject_id)
                .map(|s| s.contains(cap))
                .unwrap_or(false),
        }
    }
}


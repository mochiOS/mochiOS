use std::collections::{BTreeMap, BTreeSet};

/// capability の危険度
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityRisk {
    Normal,
    Sensitive,
    Privileged,
    Dangerous,
}

impl CapabilityRisk {
    fn from_key(key: &str) -> Option<Self> {
        match key {
            "normal" => Some(Self::Normal),
            "sensitive" => Some(Self::Sensitive),
            "privileged" => Some(Self::Privileged),
            "dangerous" => Some(Self::Dangerous),
            _ => None,
        }
    }
}

fn split_csv(value: &str) -> impl Iterator<Item = String> + '_ {
    value.split(',').map(|part| part.trim()).filter_map(|part| {
        if part.is_empty() {
            None
        } else {
            Some(part.to_string())
        }
    })
}

/// registry: capabilities.toml に定義されている capability 名集合と policy
#[derive(Clone, Debug)]
pub struct CapabilityRegistry {
    names: BTreeSet<String>,
    risk_by_name: BTreeMap<String, CapabilityRisk>,
    trusted_services: BTreeSet<String>,
    default_risk: CapabilityRisk,
}

impl CapabilityRegistry {
    pub fn load() -> Self {
        let text = include_str!("../resources/capabilities.toml");
        let mut names = BTreeSet::new();
        let mut risk_by_name = BTreeMap::new();
        let mut trusted_services = BTreeSet::new();
        let mut default_risk = CapabilityRisk::Sensitive;
        let mut section = String::new();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section.clear();
                section.push_str(&line[1..line.len() - 1]);
                continue;
            }

            let Some((lhs, rhs)) = line.split_once('=') else {
                continue;
            };
            let key = lhs.trim();
            let value = rhs.trim().trim_matches('"');

            if let Some(cap_name) = section.strip_prefix("capabilities.") {
                names.insert(cap_name.to_string());
                if key == "risk" {
                    if let Some(risk) = CapabilityRisk::from_key(value) {
                        risk_by_name.insert(cap_name.to_string(), risk);
                    }
                } else if key == "trusted_service" && value.eq_ignore_ascii_case("true") {
                    trusted_services.insert(cap_name.to_string());
                }
                continue;
            }

            match section.as_str() {
                "policy" => {
                    if key == "default_risk" {
                        if let Some(risk) = CapabilityRisk::from_key(value) {
                            default_risk = risk;
                        }
                    }
                    if key == "trusted_services" {
                        for service in split_csv(value) {
                            trusted_services.insert(service);
                        }
                    }
                }
                "policy.risk" => {
                    if let Some(risk) = CapabilityRisk::from_key(key) {
                        for cap_name in split_csv(value) {
                            risk_by_name.insert(cap_name, risk);
                        }
                    }
                }
                _ => {}
            }
        }

        Self {
            names,
            risk_by_name,
            trusted_services,
            default_risk,
        }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub fn risk_of(&self, name: &str) -> CapabilityRisk {
        self.risk_by_name
            .get(name)
            .copied()
            .unwrap_or(self.default_risk)
    }

    pub fn is_bootstrap_trusted_service(&self, name: &str) -> bool {
        self.trusted_services.contains(name)
    }

    pub fn trusted_services_len(&self) -> usize {
        self.trusted_services.len()
    }
}

use std::fs;
use std::path::Path;

/// index.tomlを解析してサービス情報を取得
pub fn parse_service_index(index_path: &Path) -> Result<ServiceIndex, String> {
    let content = fs::read_to_string(index_path)
        .map_err(|e| format!("Failed to read index.toml: {}", e))?;
    
    let value: toml::Value = toml::from_str(&content)
        .map_err(|e| format!("Failed to parse index.toml: {}", e))?;
    
    let mut index = ServiceIndex {
        services: Vec::new(),
    };
    
    // サービスを解析
    if let Some(core) = value.get("core") {
        if let Some(service_table) = core.get("service") {
            if let Some(table) = service_table.as_table() {
                for (name, value) in table {
                    if let Some(service_data) = value.as_table() {
                        let dir = service_data.get("dir")
                            .and_then(|v| v.as_str())
                            .ok_or(format!("Missing 'dir' for service {}", name))?;
                        
                        let fs_type = service_data.get("fs")
                            .and_then(|v| v.as_str())
                            .ok_or(format!("Missing 'fs' for service {}", name))?;
                        
                        let description = service_data.get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        
                        index.services.push(ServiceEntry {
                            name: name.to_string(),
                            dir: dir.to_string(),
                            fs_type: fs_type.to_string(),
                            description: description.to_string(),
                        });
                    }
                }
            }
        }
    }
    
    Ok(index)
}

#[derive(Debug)]
pub struct ServiceIndex {
    pub services: Vec<ServiceEntry>,
}

#[derive(Debug, Clone)]
pub struct ServiceEntry {
    pub name: String,
    pub dir: String,
    pub fs_type: String,
    pub description: String,
}

impl ServiceIndex {
    /// initfsに含めるサービスのリストを取得
    pub fn get_initfs_services(&self) -> Vec<&ServiceEntry> {
        self.services.iter()
            .filter(|s| s.fs_type == "initfs")
            .collect()
    }
    
    /// ext2に含めるサービスのリストを取得
    pub fn get_ext2_services(&self) -> Vec<&ServiceEntry> {
        self.services.iter()
            .filter(|s| s.fs_type == "ext2")
            .collect()
    }
}

use crate::models::SystemConfig;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub struct ConfigService {
    config_path: PathBuf,
    config: RwLock<SystemConfig>,
}

impl ConfigService {
    pub fn new() -> Self {
        let config_path = Self::resolve_config_path();
        let loaded = Self::load_from_disk(&config_path);

        Self {
            config_path,
            config: RwLock::new(loaded),
        }
    }

    /// 优先级：可执行文件同目录 config.json > 当前工作目录 config.json > 默认在可执行文件同目录创建
    fn resolve_config_path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let exe_cfg = exe_dir.join("config.json");
                if exe_cfg.exists() {
                    return exe_cfg;
                }
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            let cwd_cfg = cwd.join("config.json");
            if cwd_cfg.exists() {
                return cwd_cfg;
            }
            let parent_cfg = cwd.join("..").join("config.json");
            if parent_cfg.exists() {
                return parent_cfg;
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                return exe_dir.join("config.json");
            }
        }

        PathBuf::from("config.json")
    }

    fn load_from_disk(path: &Path) -> SystemConfig {
        if let Ok(content) = fs::read_to_string(path) {
            let clean = content.trim_start_matches('\u{feff}');
            match serde_json::from_str::<SystemConfig>(clean) {
                Ok(mut cfg) => {
                    if cfg.web_port == 0 {
                        cfg.web_port = 8999;
                    }
                    if cfg.udp_port == 0 {
                        cfg.udp_port = 8998;
                    }
                    return cfg;
                }
                Err(err) => {
                    tracing::warn!("解析配置文件 {:?} 失败，使用默认配置: {}", path, err);
                }
            }
        }
        SystemConfig::default()
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn get_config(&self) -> SystemConfig {
        self.config
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn update_config<F>(&self, updater: F) -> Result<SystemConfig, String>
    where
        F: FnOnce(&mut SystemConfig),
    {
        let snapshot = {
            let mut guard = self
                .config
                .write()
                .map_err(|e| format!("配置写锁异常: {}", e))?;
            updater(&mut guard);
            guard.clone()
        };
        self.write_atomic(&snapshot)?;
        Ok(snapshot)
    }

    pub fn save(&self) -> Result<(), String> {
        let snapshot = self.get_config();
        self.write_atomic(&snapshot)
    }

    /// 原子写入配置文件，防止崩溃导致 config.json 损坏
    fn write_atomic(&self, cfg: &SystemConfig) -> Result<(), String> {
        if let Some(parent) = self.config_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {}", e))?;
            }
        }

        let json = serde_json::to_string_pretty(cfg)
            .map_err(|e| format!("序列化配置失败: {}", e))?;

        let tmp_path = self.config_path.with_extension("json.tmp");
        fs::write(&tmp_path, json.as_bytes()).map_err(|e| format!("写入临时配置失败: {}", e))?;

        if self.config_path.exists() {
            let bak_path = self.config_path.with_extension("json.bak");
            let _ = fs::copy(&self.config_path, &bak_path);
        }

        if let Err(_) = fs::rename(&tmp_path, &self.config_path) {
            // Windows 上 rename 覆盖已有文件时可能失败，先覆盖拷贝再删 tmp
            fs::copy(&tmp_path, &self.config_path)
                .map_err(|e| format!("更新配置文件失败: {}", e))?;
            let _ = fs::remove_file(&tmp_path);
        }

        Ok(())
    }

    pub fn get_alias(&self, sn: &str) -> String {
        let cfg = self.get_config();
        cfg.headset_aliases
            .get(sn)
            .filter(|v| !v.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| sn.to_string())
    }

    pub fn set_alias(&self, sn: &str, alias: &str) {
        let _ = self.update_config(|cfg| {
            cfg.headset_aliases.insert(sn.to_string(), alias.to_string());
        });
    }

    pub fn get_device_default_project(&self, key: &str) -> Option<String> {
        let cfg = self.get_config();
        cfg.device_default_projects
            .get(key)
            .or_else(|| {
                cfg.device_default_projects
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v)
            })
            .cloned()
    }

    pub fn set_device_default_project(&self, key: &str, pkg: &str) {
        let _ = self.update_config(|cfg| {
            if pkg.trim().is_empty() {
                cfg.device_default_projects.remove(key);
            } else {
                cfg.device_default_projects
                    .insert(key.to_string(), pkg.to_string());
            }
        });
    }
}

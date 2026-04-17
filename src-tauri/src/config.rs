use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub chrome: ChromeConfig,
    pub topic_task: TopicTaskConfig,
    pub post_task: PostTaskConfig,
    pub auto_close_chrome: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeConfig {
    pub executable_path: String,
    pub debug_port: u16,
    pub user_data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicTaskConfig {
    pub target_count: u32,
    pub entry_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostTaskConfig {
    pub target_count: u32,
    pub skip_top: u32,
    pub delay_ms: u64,
    pub entry_urls: Vec<String>,
    pub like_enabled: bool,
    pub like_min_chars: u32,
    pub like_max_count: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            chrome: ChromeConfig {
                executable_path: default_chrome_path(),
                debug_port: 9222,
                user_data_dir: default_user_data_dir(),
            },
            topic_task: TopicTaskConfig {
                target_count: 500,
                entry_urls: vec![
                    "https://linux.do/latest".to_string(),
                    "https://linux.do/c/develop/4".to_string(),
                    "https://linux.do/c/resource/14".to_string(),
                    "https://linux.do/c/welfare/36".to_string(),
                    "https://linux.do/c/news/34".to_string(),
                ],
            },
            post_task: PostTaskConfig {
                target_count: 10000,
                skip_top: 3,
                delay_ms: 200,
                entry_urls: vec![
                    "https://linux.do/unread".to_string(),
                ],
                like_enabled: true,
                like_min_chars: 50,
                like_max_count: 30,
            },
            auto_close_chrome: false,
        }
    }
}

fn default_chrome_path() -> String {
    #[cfg(target_os = "windows")]
    {
        r"C:\Program Files\Google\Chrome\Application\chrome.exe".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "/usr/bin/google-chrome".to_string()
    }
}

fn default_user_data_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        r"D:\temp\chrome_liudu".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/.liudu_chrome_data", home)
    }
}

/// 获取配置文件路径
pub fn config_file_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    std::fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

/// 从文件加载配置
pub fn load_config(app_handle: &tauri::AppHandle) -> AppConfig {
    let path = config_file_path(app_handle);
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => return config,
                Err(e) => {
                    log::warn!("配置文件解析失败，使用默认配置: {}", e);
                }
            },
            Err(e) => {
                log::warn!("配置文件读取失败: {}", e);
            }
        }
    }
    AppConfig::default()
}

/// 保存配置到文件
pub fn save_config_to_file(app_handle: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_file_path(app_handle);
    let content = serde_json::to_string_pretty(config).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {}", e))?;
    Ok(())
}

use tauri::Manager;

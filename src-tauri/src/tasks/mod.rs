pub mod common;
pub mod topic_reader;
pub mod post_reader;

use serde::{Deserialize, Serialize};

/// 任务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    #[serde(rename = "topic")]
    TopicReader,
    #[serde(rename = "post")]
    PostReader,
}

/// 发送到前端的进度信息
#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_type: String,
    pub current: u32,
    pub total: u32,
    pub likes: u32,
    pub finished: bool,
}

/// 发送到前端的日志
#[derive(Debug, Clone, Serialize)]
pub struct TaskLog {
    pub timestamp: String,
    pub message: String,
    pub level: String,
}

impl TaskLog {
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            timestamp: chrono_now(),
            message: msg.into(),
            level: "info".to_string(),
        }
    }
    pub fn warn(msg: impl Into<String>) -> Self {
        Self {
            timestamp: chrono_now(),
            message: msg.into(),
            level: "warn".to_string(),
        }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            timestamp: chrono_now(),
            message: msg.into(),
            level: "error".to_string(),
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24 + 8; // UTC+8 简化处理
    let mins = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", hours % 24, mins, s)
}

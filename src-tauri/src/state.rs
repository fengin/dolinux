use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cdp::client::CdpClient;
use crate::config::AppConfig;

/// 应用全局状态
pub struct AppState {
    /// CDP 客户端（连接后才有值）
    pub cdp: Arc<Mutex<Option<Arc<CdpClient>>>>,
    /// 当前配置
    pub config: Arc<Mutex<AppConfig>>,
    /// 任务是否正在运行
    pub task_running: Arc<AtomicBool>,
    /// 停止标志
    pub stop_flag: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            cdp: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(config)),
            task_running: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.task_running.load(Ordering::SeqCst)
    }

    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst)
    }

    pub fn start_task(&self) {
        self.stop_flag.store(false, Ordering::SeqCst);
        self.task_running.store(true, Ordering::SeqCst);
    }

    pub fn finish_task(&self) {
        self.task_running.store(false, Ordering::SeqCst);
    }

    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

use std::process::Command;
use crate::config::ChromeConfig;

/// 启动 Chrome 浏览器（开启远程调试端口）
pub fn launch_chrome(config: &ChromeConfig) -> Result<(), String> {
    let exe = &config.executable_path;

    if !std::path::Path::new(exe).exists() {
        return Err(format!("Chrome 未找到: {}\n请在设置中配置正确的 Chrome 路径", exe));
    }

    let args = vec![
        format!("--remote-debugging-port={}", config.debug_port),
        format!("--user-data-dir={}", config.user_data_dir),
        "--no-first-run".to_string(),
    ];

    log::info!("启动 Chrome: {} {:?}", exe, args);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        Command::new(exe)
            .args(&args)
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| format!("启动 Chrome 失败: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new(exe)
            .args(&args)
            .spawn()
            .map_err(|e| format!("启动 Chrome 失败: {}", e))?;
    }

    Ok(())
}

/// 检查 Chrome 调试端口是否可达
pub async fn check_chrome_alive(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    reqwest::get(&url).await.is_ok()
}

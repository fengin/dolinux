// 流读 - Linux.do 阅读助手
// https://aibook.ren (AI全书)

mod cdp;
mod chrome;
mod config;
mod state;
mod tasks;

use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::cdp::client::CdpClient;
use crate::config::AppConfig;
use crate::state::AppState;
use crate::tasks::TaskLog;

// ========== Tauri Commands ==========

/// 启动 Chrome 浏览器
#[tauri::command]
async fn launch_chrome(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let cfg = state.config.lock().await;
    chrome::launch_chrome(&cfg.chrome)?;
    Ok("Chrome 已启动，请在浏览器中登录 Linux.do 后点击「连接」".to_string())
}

/// 连接到 Chrome
#[tauri::command]
async fn connect_chrome(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let port = {
        let cfg = state.config.lock().await;
        cfg.chrome.debug_port
    };

    // 发现页面
    let pages = CdpClient::discover_pages(port).await?;
    let page = CdpClient::pick_page(&pages)
        .ok_or_else(|| "未找到可用页面，请确保 Chrome 中有打开的网页标签".to_string())?;

    let ws_url = page
        .web_socket_debugger_url
        .as_ref()
        .ok_or("页面无 WebSocket URL")?;

    let title = page.title.clone();

    // 连接 WebSocket
    let client = CdpClient::connect(ws_url).await?;
    *state.cdp.lock().await = Some(client);

    let _ = app.emit("chrome-status", true);

    let msg = format!("✅ 已连接到 Chrome，当前页面: {}", title);
    let _ = app.emit("task-log", TaskLog::info(&msg));
    Ok(msg)
}

/// 断开 Chrome 连接
#[tauri::command]
async fn disconnect_chrome(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *state.cdp.lock().await = None;
    let _ = app.emit("chrome-status", false);
    let _ = app.emit("task-log", TaskLog::info("已断开 Chrome 连接"));
    Ok(())
}

/// 检查 Chrome 连接状态
#[tauri::command]
async fn check_chrome_status(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let cdp = state.cdp.lock().await;
    Ok(cdp.as_ref().map_or(false, |c| c.is_connected()))
}

/// 开始任务
#[tauri::command]
async fn start_task(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    task_type: String,
) -> Result<(), String> {
    if state.is_running() {
        return Err("已有任务在运行中".to_string());
    }

    let client = {
        let cdp = state.cdp.lock().await;
        cdp.as_ref()
            .ok_or("请先连接 Chrome")?
            .clone()
    };

    if !client.is_connected() {
        return Err("Chrome 连接已断开，请重新连接".to_string());
    }

    state.start_task();

    let state_arc = state.inner().clone();
    let config = state.config.lock().await.clone();

    match task_type.as_str() {
        "topic" => {
            let topic_config = config.topic_task.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                tasks::topic_reader::run(app_clone, state_arc, client, topic_config).await;
            });
        }
        "post" => {
            let post_config = config.post_task.clone();
            let app_clone = app.clone();
            tokio::spawn(async move {
                tasks::post_reader::run(app_clone, state_arc, client, post_config).await;
            });
        }
        _ => {
            state.finish_task();
            return Err(format!("未知任务类型: {}", task_type));
        }
    }

    Ok(())
}

/// 停止任务
#[tauri::command]
async fn stop_task(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if !state.is_running() {
        return Err("没有正在运行的任务".to_string());
    }
    state.request_stop();
    let _ = app.emit("task-log", TaskLog::info("正在停止任务..."));
    Ok(())
}

/// 获取当前配置
#[tauri::command]
async fn get_config(state: State<'_, Arc<AppState>>) -> Result<AppConfig, String> {
    let cfg = state.config.lock().await;
    Ok(cfg.clone())
}

/// 保存配置
#[tauri::command]
async fn save_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    new_config: AppConfig,
) -> Result<(), String> {
    config::save_config_to_file(&app, &new_config)?;
    *state.config.lock().await = new_config;
    let _ = app.emit("task-log", TaskLog::info("配置已保存"));
    Ok(())
}

/// 检查任务运行状态
#[tauri::command]
async fn is_task_running(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(state.is_running())
}

// ========== 应用入口 ==========

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 加载配置
            let config = config::load_config(&app.handle());
            let state = Arc::new(AppState::new(config));

            app.manage(state);

            // 设置系统托盘
            setup_tray(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 点击关闭按钮时隐藏到托盘，而不是退出
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            launch_chrome,
            connect_chrome,
            disconnect_chrome,
            check_chrome_status,
            start_task,
            stop_task,
            get_config,
            save_config,
            is_task_running,
        ])
        .run(tauri::generate_context!())
        .expect("启动应用失败");
}

/// 设置系统托盘
fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("流读 - Linux.do 阅读助手")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

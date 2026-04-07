use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::cdp::client::CdpClient;
use crate::config::TopicTaskConfig;
use crate::state::AppState;
use crate::tasks::common::*;
use crate::tasks::{TaskLog, TaskProgress};

/// 运行刷主题阅读任务
pub async fn run(
    app: AppHandle,
    state: Arc<AppState>,
    client: Arc<CdpClient>,
    config: TopicTaskConfig,
) {
    let emit_log = |msg: String| {
        let _ = app.emit("task-log", TaskLog::info(&msg));
        log::info!("{}", msg);
    };
    let emit_warn = |msg: String| {
        let _ = app.emit("task-log", TaskLog::warn(&msg));
        log::warn!("{}", msg);
    };
    let emit_progress = |current: u32, total: u32| {
        let _ = app.emit(
            "task-progress",
            TaskProgress {
                task_type: "topic".to_string(),
                current,
                total,
                likes: 0,
                finished: false,
            },
        );
    };

    let target = config.target_count;
    let mut topics_read: u32 = 0;

    emit_log(format!(
        "开始刷主题阅读，目标: {} 个，入口: {} 个",
        target,
        config.entry_urls.len()
    ));

    for (entry_idx, entry_url) in config.entry_urls.iter().enumerate() {
        if topics_read >= target || state.should_stop() {
            break;
        }

        emit_log(format!(
            "[入口 {}/{}] 导航到: {}",
            entry_idx + 1,
            config.entry_urls.len(),
            entry_url
        ));

        if let Err(e) = client.navigate_and_wait(entry_url, 30000).await {
            emit_warn(format!("导航失败: {}", e));
            continue;
        }
        delay_ms(2000).await;

        // 在当前入口列表中寻找并浏览未读话题
        let mut visited_titles: HashSet<String> = HashSet::new();
        let mut no_new_rounds = 0;

        while topics_read < target && !state.should_stop() {
            // 检查列表是否存在
            let list_exists = client
                .evaluate(JS_CHECK_TOPIC_LIST)
                .await
                .unwrap_or(Value::Bool(false));
            if list_exists != Value::Bool(true) {
                emit_warn("未找到话题列表".to_string());
                break;
            }

            // 获取未读话题
            let topics_json = client
                .evaluate_as_string(JS_GET_UNREAD_TOPICS)
                .await
                .unwrap_or_else(|_| "[]".to_string());

            let topics: Vec<serde_json::Value> =
                serde_json::from_str(&topics_json).unwrap_or_default();

            let mut found_new = false;

            for topic in &topics {
                if topics_read >= target || state.should_stop() {
                    break;
                }

                let title = topic
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let url = topic
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();

                if title.is_empty() || url.is_empty() || visited_titles.contains(&title) {
                    continue;
                }

                visited_titles.insert(title.clone());
                found_new = true;
                no_new_rounds = 0;

                emit_log(format!(
                    "[{}/{}] 点击话题: {}",
                    topics_read + 1,
                    target,
                    &title
                ));
                emit_progress(topics_read, target);

                // 导航到话题
                if let Err(e) = client.navigate_and_wait(&url, 30000).await {
                    emit_warn(format!("导航话题失败: {}", e));
                    continue;
                }
                random_delay(1.5, 2.5).await;

                // 模拟人类阅读
                simulate_human_scroll(&client).await.ok();

                topics_read += 1;
                emit_progress(topics_read, target);

                if topics_read >= target || state.should_stop() {
                    break;
                }

                // 返回列表
                if let Err(e) = client.navigate_and_wait(entry_url, 30000).await {
                    emit_warn(format!("返回列表失败: {}", e));
                    break;
                }
                random_delay(1.5, 2.5).await;
            }

            if topics_read >= target || state.should_stop() {
                break;
            }

            if !found_new {
                no_new_rounds += 1;
                if no_new_rounds >= 3 {
                    emit_log(format!(
                        "连续 {} 次无新未读话题，切换下一个入口",
                        no_new_rounds
                    ));
                    break;
                }
                emit_log(format!(
                    "当前列表无新未读话题，下拉加载更多（第 {} 次）",
                    no_new_rounds
                ));
                client.evaluate(JS_SCROLL_BOTTOM).await.ok();
                delay_ms(2000).await;
            }
        }

        emit_log(format!(
            "入口 {} 完成，累计阅读: {}/{}",
            entry_idx + 1,
            topics_read,
            target
        ));
    }

    // 任务完成
    let msg = if state.should_stop() {
        format!("任务已停止！共阅读 {} 个主题", topics_read)
    } else {
        format!("✅ 任务完成！共阅读 {} 个主题", topics_read)
    };
    emit_log(msg);

    let _ = app.emit(
        "task-progress",
        TaskProgress {
            task_type: "topic".to_string(),
            current: topics_read,
            total: target,
            likes: 0,
            finished: true,
        },
    );

    state.finish_task();
}

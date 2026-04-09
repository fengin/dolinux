use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::cdp::client::CdpClient;
use crate::config::TopicTaskConfig;
use crate::state::AppState;
use crate::tasks::common::*;
use crate::tasks::{TaskLog, TaskProgress};

/// 最大轮次：所有入口遍历一遍算一轮，连续一整轮无新话题才停止
const MAX_ROUNDS: u32 = 10;

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
    // 全局已访问 URL 集合，跨入口、跨轮次去重
    let mut visited_urls: HashSet<String> = HashSet::new();

    emit_log(format!(
        "开始刷主题阅读，目标: {} 个，入口: {} 个",
        target,
        config.entry_urls.len()
    ));

    // 外层：多轮循环所有入口，直到目标达成或连续一整轮无新话题
    for round in 0..MAX_ROUNDS {
        if topics_read >= target || state.should_stop() {
            break;
        }

        let round_start_count = topics_read;

        if round > 0 {
            emit_log(format!(
                "--- 第 {} 轮扫描入口（累计已读: {}/{}）---",
                round + 1,
                topics_read,
                target
            ));
        }

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
                emit_warn(format!("导航失败: {}，跳过此入口", e));
                continue;
            }
            delay_ms(2000).await;

            // 在当前入口列表中寻找并浏览未读话题
            let mut no_new_rounds = 0;

            while topics_read < target && !state.should_stop() {
                // 检查列表是否存在（带重试）
                let mut list_found = false;
                for retry in 0..3 {
                    let list_exists = client
                        .evaluate(JS_CHECK_TOPIC_LIST)
                        .await
                        .unwrap_or(Value::Bool(false));
                    if list_exists == Value::Bool(true) {
                        list_found = true;
                        break;
                    }
                    if retry < 2 {
                        delay_ms(1000).await;
                    }
                }
                if !list_found {
                    emit_warn("未找到话题列表，跳过此入口".to_string());
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

                    if title.is_empty() || url.is_empty() || visited_urls.contains(&url) {
                        continue;
                    }

                    visited_urls.insert(url.clone());
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
                        emit_warn(format!("导航话题失败: {}，跳过", e));
                        // 尝试回到列表页继续
                        if let Err(e2) = client.navigate_and_wait(entry_url, 30000).await {
                            emit_warn(format!("恢复到列表页也失败: {}，跳过此入口", e2));
                            break;
                        }
                        delay_ms(1000).await;
                        continue;
                    }
                    random_delay(1.5, 2.5).await;

                    // 注入 track_visit 请求，触发服务端记录主题浏览数
                    client.evaluate(JS_TRACK_TOPIC_VIEW).await.ok();

                    // 模拟人类阅读
                    simulate_human_scroll(&client).await.ok();

                    topics_read += 1;
                    emit_progress(topics_read, target);

                    if topics_read >= target || state.should_stop() {
                        break;
                    }

                    // 返回列表（带重试）
                    let mut back_ok = false;
                    for retry in 0..3 {
                        match client.navigate_and_wait(entry_url, 30000).await {
                            Ok(_) => {
                                back_ok = true;
                                break;
                            }
                            Err(e) => {
                                if retry < 2 {
                                    emit_warn(format!(
                                        "返回列表失败（第{}次重试）: {}",
                                        retry + 1,
                                        e
                                    ));
                                    delay_ms(2000).await;
                                } else {
                                    emit_warn(format!("返回列表失败，跳过此入口: {}", e));
                                }
                            }
                        }
                    }
                    if !back_ok {
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

        // 一整轮下来没有新增任何阅读，说明所有入口都没有新话题了
        if topics_read == round_start_count {
            emit_log("所有入口均无新话题可读，任务结束".to_string());
            break;
        }
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

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::cdp::client::CdpClient;
use crate::config::PostTaskConfig;
use crate::state::AppState;
use crate::tasks::common::*;
use crate::tasks::{TaskLog, TaskProgress};

/// 最大轮次
const MAX_ROUNDS: u32 = 10;

/// 运行刷帖子+点赞任务
pub async fn run(
    app: AppHandle,
    state: Arc<AppState>,
    client: Arc<CdpClient>,
    config: PostTaskConfig,
) {
    let emit_log = |msg: String| {
        let _ = app.emit("task-log", TaskLog::info(&msg));
        log::info!("{}", msg);
    };
    let emit_warn = |msg: String| {
        let _ = app.emit("task-log", TaskLog::warn(&msg));
        log::warn!("{}", msg);
    };
    let emit_progress = |current: u32, total: u32, likes: u32, finished: bool| {
        let _ = app.emit(
            "task-progress",
            TaskProgress {
                task_type: "post".to_string(),
                current,
                total,
                likes,
                finished,
            },
        );
    };

    let target = config.target_count;
    let delay = config.delay_ms;

    emit_log(format!(
        "开始刷帖子，目标: {} 个，延时: {}ms，入口: {} 个",
        target,
        delay,
        config.entry_urls.len()
    ));
    if config.like_enabled {
        emit_log(format!(
            "点赞已启用：中文 ≥ {} 字点赞，上限 {} 个",
            config.like_min_chars, config.like_max_count
        ));
    }

    let mut total_read: u32 = 0;
    let mut total_likes: u32 = 0;
    // 全局已访问话题 URL 集合（去重用）
    let mut visited_topic_urls: HashSet<String> = HashSet::new();

    // 多轮循环所有入口
    for round in 0..MAX_ROUNDS {
        if total_read >= target || state.should_stop() {
            break;
        }

        let round_start_count = total_read;

        if round > 0 {
            emit_log(format!(
                "--- 第 {} 轮扫描入口（累计已读: {}/{}）---",
                round + 1,
                total_read,
                target
            ));
        }

        for (entry_idx, entry_url) in config.entry_urls.iter().enumerate() {
            if total_read >= target || state.should_stop() {
                break;
            }

            emit_log(format!(
                "[入口 {}/{}] 收集话题: {}",
                entry_idx + 1,
                config.entry_urls.len(),
                entry_url
            ));

            if let Err(e) = client.navigate_and_wait(entry_url, 30000).await {
                emit_warn(format!("导航失败: {}，跳过此入口", e));
                continue;
            }
            delay_ms(2000).await;

            // 向下滚动加载更多话题
            for _ in 0..10 {
                client.evaluate(JS_SCROLL_BOTTOM).await.ok();
                delay_ms(1500).await;
            }
            // 回到顶部
            client.evaluate("window.scrollTo(0, 0)").await.ok();
            delay_ms(1000).await;

            // 获取所有未读话题
            let topics_json = client
                .evaluate_as_string(JS_GET_ALL_UNREAD_TOPICS)
                .await
                .unwrap_or_else(|_| "[]".to_string());

            let topics: Vec<Value> = serde_json::from_str(&topics_json).unwrap_or_default();

            // 收集话题（去重 + 跳过热门）
            let mut topic_urls: Vec<(String, String)> = Vec::new();
            for topic in &topics {
                let idx = topic.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                if idx < config.skip_top {
                    continue;
                }
                let url = topic
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = topic
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if url.is_empty() {
                    continue;
                }
                let base = strip_page_number(&url);
                if visited_topic_urls.contains(&base) {
                    continue;
                }
                visited_topic_urls.insert(base);
                topic_urls.push((url, title));
            }

            if topic_urls.is_empty() {
                emit_log("  此入口无新话题".to_string());
                continue;
            }

            emit_log(format!("  收集到 {} 个新话题", topic_urls.len()));

            // 逐一浏览话题中的帖子
            for (i, (topic_url, topic_title)) in topic_urls.iter().enumerate() {
                if total_read >= target || state.should_stop() {
                    break;
                }

                emit_log(format!(
                    "[话题 {}/{}] {} (已读: {}/{})",
                    i + 1,
                    topic_urls.len(),
                    topic_title,
                    total_read,
                    target
                ));

                // 导航到话题
                if let Err(e) = client.navigate_and_wait(topic_url, 30000).await {
                    emit_warn(format!("导航话题失败: {}，跳过", e));
                    continue;
                }
                delay_ms(1500).await;

                // 在话题内快速阅读帖子
                let mut seen_post_ids: HashSet<String> = HashSet::new();
                let mut no_new_count = 0;

                // 获取话题总帖子数，用于判断是否还有更多帖子
                let topic_total_posts = client
                    .evaluate_as_i64(JS_GET_TOPIC_POST_COUNT)
                    .await
                    .unwrap_or(0) as u32;

                while total_read < target && !state.should_stop() {
                    // 获取当前可见的帖子 ID
                    let posts_json = client
                        .evaluate_as_string(JS_GET_POST_IDS)
                        .await
                        .unwrap_or_else(|_| "[]".to_string());

                    let post_ids: Vec<String> =
                        serde_json::from_str(&posts_json).unwrap_or_default();

                    let mut new_posts = false;
                    for post_id in &post_ids {
                        if total_read >= target || state.should_stop() {
                            break;
                        }
                        if post_id.is_empty() || seen_post_ids.contains(post_id) {
                            continue;
                        }
                        seen_post_ids.insert(post_id.clone());
                        total_read += 1;
                        new_posts = true;
                    }

                    // 尝试点赞
                    if config.like_enabled && total_likes < config.like_max_count {
                        let like_js = js_try_like_post(config.like_min_chars);
                        let liked =
                            client.evaluate(&like_js).await.unwrap_or(Value::Bool(false));
                        if liked == Value::Bool(true) {
                            total_likes += 1;
                            emit_log(format!(
                                "  👍 点赞！(已点赞: {}/{})",
                                total_likes, config.like_max_count
                            ));
                        }
                    }

                    emit_progress(total_read, target, total_likes, false);

                    if total_read >= target || state.should_stop() {
                        break;
                    }

                    // 滚动到页面底部，确保触发 Discourse 的帖子懒加载
                    client.evaluate(JS_SCROLL_BOTTOM).await.ok();

                    if new_posts {
                        no_new_count = 0;
                        delay_ms(delay).await;
                    } else {
                        no_new_count += 1;

                        // 检查是否确实已读完所有帖子
                        let all_loaded = topic_total_posts > 0
                            && seen_post_ids.len() as u32 >= topic_total_posts;

                        if all_loaded {
                            // 话题内所有帖子已读完
                            break;
                        }

                        if no_new_count >= 15 {
                            // 等待较多轮仍无新帖，可能确实加载完了
                            break;
                        }

                        // 等待 Discourse 完成懒加载
                        delay_ms(2000).await;
                        // 再次滚动到底部触发加载
                        client.evaluate(JS_SCROLL_BOTTOM).await.ok();
                        delay_ms(500).await;
                    }
                }

                emit_log(format!(
                    "  本话题阅读 {} 个帖子，累计: {}/{}",
                    seen_post_ids.len(),
                    total_read,
                    target
                ));

                // 话题间短暂停顿
                if total_read < target && !state.should_stop() {
                    random_delay(1.0, 3.0).await;
                }
            }

            emit_log(format!(
                "入口 {} 完成，累计阅读: {}/{}",
                entry_idx + 1,
                total_read,
                target
            ));
        }

        // 一整轮下来没有新增任何阅读，说明所有入口都没有新话题了
        if total_read == round_start_count {
            emit_log("所有入口均无新话题可读，任务结束".to_string());
            break;
        }
    }

    // 任务完成
    let msg = if state.should_stop() {
        format!(
            "任务已停止！共阅读 {} 个帖子，点赞 {} 个",
            total_read, total_likes
        )
    } else {
        format!(
            "✅ 任务完成！共阅读 {} 个帖子，点赞 {} 个",
            total_read, total_likes
        )
    };
    emit_log(msg);
    emit_progress(total_read, target, total_likes, true);

    state.finish_task();
}

/// 去掉 URL 末尾的页码数字，用于去重
fn strip_page_number(url: &str) -> String {
    // https://linux.do/t/topic/1697632/8 -> https://linux.do/t/topic/1697632
    let re = regex_lite::Regex::new(r"/\d+$").unwrap();
    re.replace(url, "").to_string()
}


use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::cdp::client::CdpClient;
use crate::config::PostTaskConfig;
use crate::state::AppState;
use crate::tasks::common::*;
use crate::tasks::{TaskLog, TaskProgress};

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

    // Step 1: 从所有入口页面收集话题链接
    let mut all_topic_urls: Vec<(String, String)> = Vec::new(); // (url, title)

    for (entry_idx, entry_url) in config.entry_urls.iter().enumerate() {
        if state.should_stop() {
            break;
        }

        emit_log(format!(
            "[入口 {}/{}] 收集话题: {}",
            entry_idx + 1,
            config.entry_urls.len(),
            entry_url
        ));

        if let Err(e) = client.navigate_and_wait(entry_url, 30000).await {
            emit_warn(format!("导航失败: {}", e));
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

        let mut count = 0;
        for topic in &topics {
            let idx = topic.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
            // 跳过前 N 个热门话题
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
            if !url.is_empty() {
                all_topic_urls.push((url, title));
                count += 1;
            }
        }
        emit_log(format!("  收集到 {} 个未读话题", count));
    }

    // 去重
    let mut seen = HashSet::new();
    let topic_urls: Vec<(String, String)> = all_topic_urls
        .into_iter()
        .filter(|(url, _)| {
            let base = strip_page_number(url);
            seen.insert(base)
        })
        .collect();

    if topic_urls.is_empty() {
        emit_warn("未收集到任何未读话题，任务结束".to_string());
        emit_progress(0, target, 0, true);
        state.finish_task();
        return;
    }

    emit_log(format!("共收集到 {} 个未读话题", topic_urls.len()));

    // Step 2: 逐一浏览话题中的帖子
    let mut total_read: u32 = 0;
    let mut total_likes: u32 = 0;

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
            emit_warn(format!("导航失败: {}", e));
            continue;
        }
        delay_ms(1500).await;

        // 在话题内快速阅读帖子
        let mut seen_post_ids: HashSet<String> = HashSet::new();
        let mut no_new_count = 0;

        while total_read < target && !state.should_stop() {
            // 获取当前可见的帖子 ID
            let posts_json = client
                .evaluate_as_string(JS_GET_POST_IDS)
                .await
                .unwrap_or_else(|_| "[]".to_string());

            let post_ids: Vec<String> = serde_json::from_str(&posts_json).unwrap_or_default();

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
                let liked = client.evaluate(&like_js).await.unwrap_or(Value::Bool(false));
                if liked == Value::Bool(true) {
                    total_likes += 1;
                    emit_log(format!("  👍 点赞！(已点赞: {}/{})", total_likes, config.like_max_count));
                }
            }

            emit_progress(total_read, target, total_likes, false);

            if total_read >= target || state.should_stop() {
                break;
            }

            // 快速滚动
            fast_scroll(&client, 800).await.ok();

            if new_posts {
                // 有新帖子：使用用户配置的快速阅读延时
                no_new_count = 0;
                delay_ms(delay).await;
            } else {
                // 无新帖子：可能是 Discourse 瀑布流还在加载
                // 需要等更长时间让 AJAX 请求完成
                no_new_count += 1;

                if no_new_count >= 8 {
                    // 已等待 8 轮（约 16 秒），确认帖子已全部加载完
                    break;
                }

                // 等待 2 秒让 Discourse 完成懒加载
                delay_ms(2000).await;

                // 再滚动一次触发加载
                fast_scroll(&client, 500).await.ok();
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

    // 任务完成
    let msg = if state.should_stop() {
        format!("任务已停止！共阅读 {} 个帖子，点赞 {} 个", total_read, total_likes)
    } else {
        format!("✅ 任务完成！共阅读 {} 个帖子，点赞 {} 个", total_read, total_likes)
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

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

                // 尝试通过 Discourse Ember 内部 API 获取帖子流
                // 此方式不依赖 DOM 渲染，在浏览器最小化时也能正常获取全部帖子 ID
                let mut stream_json = String::new();
                for _ in 0..3 {
                    stream_json = client
                        .evaluate_as_string(JS_GET_POST_STREAM)
                        .await
                        .unwrap_or_default();
                    if !stream_json.is_empty() {
                        break;
                    }
                    delay_ms(1000).await;
                }

                if !stream_json.is_empty() {
                    // ===== API 模式：通过 Discourse 模型获取话题阅读状态 =====
                    if let Ok(info) = serde_json::from_str::<Value>(&stream_json) {
                        let highest = info
                            .get("highestPostNumber")
                            .and_then(|n| n.as_u64())
                            .unwrap_or(0) as u32;
                        let last_read = info
                            .get("lastReadPostNumber")
                            .and_then(|n| n.as_u64())
                            .unwrap_or(0) as u32;

                        // 计算未读帖子数
                        let unread_count = if highest > last_read {
                            highest - last_read
                        } else {
                            0
                        };

                        if unread_count == 0 {
                            emit_log("  本话题无未读帖子，跳过".to_string());
                        } else {
                            // 本次需要阅读的数量 = min(未读数, 剩余目标数)
                            let remaining = target.saturating_sub(total_read);
                            let posts_to_read =
                                std::cmp::min(unread_count, remaining);

                            total_read += posts_to_read;
                            emit_progress(total_read, target, total_likes, false);

                            // 上报范围：从 last_read+1 到 last_read+posts_to_read
                            let from_post = last_read + 1;
                            let to_post = last_read + posts_to_read;
                            let timing_js =
                                js_send_post_timings(from_post, to_post);
                            let timing_result = client
                                .evaluate_as_string(&timing_js)
                                .await
                                .unwrap_or_else(|_| "error".to_string());

                            if timing_result.starts_with("sent:") {
                                emit_log(format!(
                                    "  📊 未读 {} 个，本次阅读 {} 个，已上报 #{}-#{} 的阅读记录",
                                    unread_count,
                                    posts_to_read,
                                    from_post,
                                    to_post
                                ));
                            } else {
                                emit_warn(format!(
                                    "  ⚠ 阅读上报失败: {}",
                                    timing_result
                                ));
                            }
                        }

                        // 点赞：需要将帖子逐批加载到 DOM 中才能操作点赞按钮
                        if config.like_enabled && total_likes < config.like_max_count {
                            let mut like_rounds = 0;
                            loop {
                                if state.should_stop() || total_likes >= config.like_max_count {
                                    break;
                                }

                                let like_js = js_try_like_post(config.like_min_chars);
                                let liked = client
                                    .evaluate(&like_js)
                                    .await
                                    .unwrap_or(Value::Bool(false));
                                if liked == Value::Bool(true) {
                                    total_likes += 1;
                                    emit_log(format!(
                                        "  👍 点赞！(已点赞: {}/{})",
                                        total_likes, config.like_max_count
                                    ));
                                    emit_progress(total_read, target, total_likes, false);
                                }

                                // 通过 Discourse API 加载下一批帖子到 DOM
                                let load_result = client
                                    .evaluate_as_string(JS_LOAD_NEXT_POSTS)
                                    .await
                                    .unwrap_or_else(|_| "no_api".to_string());

                                if load_result != "loaded" {
                                    break;
                                }
                                like_rounds += 1;
                                if like_rounds > 50 {
                                    break; // 安全上限：避免无限循环
                                }
                                delay_ms(delay).await;
                            }
                        }
                    }
                } else {
                    // ===== DOM 回退模式：使用增强版滚动 =====
                    let mut no_new_count = 0;

                    let topic_total_posts = client
                        .evaluate_as_i64(JS_GET_TOPIC_POST_COUNT)
                        .await
                        .unwrap_or(0) as u32;

                    while total_read < target && !state.should_stop() {
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
                            let liked = client
                                .evaluate(&like_js)
                                .await
                                .unwrap_or(Value::Bool(false));
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

                        // 增强版滚动：标准滚动 + 事件派发 + scrollIntoView
                        client.evaluate(JS_SCROLL_BOTTOM_ENHANCED).await.ok();

                        if new_posts {
                            no_new_count = 0;
                            delay_ms(delay).await;
                        } else {
                            no_new_count += 1;

                            let all_loaded = topic_total_posts > 0
                                && seen_post_ids.len() as u32 >= topic_total_posts;

                            if all_loaded {
                                break;
                            }

                            if no_new_count >= 15 {
                                break;
                            }

                            delay_ms(2000).await;
                            client.evaluate(JS_SCROLL_BOTTOM_ENHANCED).await.ok();
                            delay_ms(500).await;
                        }
                    }
                }

                emit_log(format!(
                    "  累计: {}/{}",
                    total_read,
                    target
                ));

                // 话题间短暂停顿
                if total_read < target && !state.should_stop() {
                    random_delay(1.0, 3.0).await;
                }
            }

            emit_log(format!(
                "入口 {} 完成，累计: {}/{}",
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
    if state.should_stop() {
        emit_log(format!(
            "⏹ 任务已停止，共阅读 {} 个帖子，点赞 {} 个",
            total_read, total_likes
        ));
    } else if total_read >= target {
        emit_log(format!(
            "✅ 任务完成！共阅读 {} 个帖子，点赞 {} 个",
            total_read, total_likes
        ));
    } else {
        emit_warn(format!(
            "⚠ 当前入口的未读帖子已全部处理，共阅读 {} 个（目标 {}，还差 {} 个）。如需继续，请在设置中添加其他阅读入口后重新执行",
            total_read, target, target - total_read
        ));
    }
    emit_progress(total_read, target, total_likes, true);

    state.finish_task();
}

/// 去掉 URL 末尾的页码数字，用于去重
fn strip_page_number(url: &str) -> String {
    // https://linux.do/t/topic/1697632/8 -> https://linux.do/t/topic/1697632
    let re = regex_lite::Regex::new(r"/\d+$").unwrap();
    re.replace(url, "").to_string()
}


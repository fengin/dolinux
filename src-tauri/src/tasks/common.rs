use std::sync::Arc;
use std::time::Duration;
use rand::Rng;
use crate::cdp::client::CdpClient;

/// 随机延时（秒）
pub async fn random_delay(min_secs: f64, max_secs: f64) {
    let delay = {
        let mut rng = rand::thread_rng();
        rng.gen_range(min_secs..max_secs)
    };
    tokio::time::sleep(Duration::from_secs_f64(delay)).await;
}

/// 固定延时（毫秒）
pub async fn delay_ms(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// 模拟人类滚动行为（用于刷主题任务）
pub async fn simulate_human_scroll(client: &Arc<CdpClient>) -> Result<(), String> {
    let (scroll_steps, distances, pauses, do_back, back_dist, back_pause) = {
        let mut rng = rand::thread_rng();
        let steps = rng.gen_range(3..=5);
        let dists: Vec<i32> = (0..steps).map(|_| rng.gen_range(300..=600)).collect();
        let paus: Vec<f64> = (0..steps).map(|_| rng.gen_range(1.0..2.0)).collect();
        let do_b = rng.gen_bool(0.5);
        let b_dist = rng.gen_range(200..=400);
        let b_pause = rng.gen_range(0.5..1.0);
        (steps, dists, paus, do_b, b_dist, b_pause)
    }; // rng dropped here, safe to await below

    for i in 0..scroll_steps {
        let js = format!("window.scrollBy(0, {})", distances[i]);
        client.evaluate(&js).await.ok();
        tokio::time::sleep(Duration::from_secs_f64(pauses[i])).await;
    }

    // 偶尔回滚
    if do_back {
        let js = format!("window.scrollBy(0, -{})", back_dist);
        client.evaluate(&js).await.ok();
        tokio::time::sleep(Duration::from_secs_f64(back_pause)).await;
    }

    // 等待 Discourse timings 请求发出
    tokio::time::sleep(Duration::from_secs(3)).await;

    Ok(())
}

/// 快速滚动（用于刷帖子任务）
pub async fn fast_scroll(client: &Arc<CdpClient>, distance: i32) -> Result<(), String> {
    let js = format!("window.scrollBy(0, {})", distance);
    client.evaluate(&js).await.ok();
    Ok(())
}

/// JS: 获取未读话题列表（带小蓝点标记）
pub const JS_GET_UNREAD_TOPICS: &str = r#"
(() => {
    const topics = [];
    const rows = document.querySelectorAll('tr.topic-list-item');
    rows.forEach(row => {
        const badge = row.querySelector('.badge.new-topic');
        if (badge) {
            const link = row.querySelector('a.title');
            if (link && link.href && link.href.includes('/t/')) {
                topics.push({
                    url: link.href,
                    title: (link.textContent || '').trim().substring(0, 60)
                });
            }
        }
    });
    return JSON.stringify(topics);
})()
"#;

/// JS: 获取未读话题列表（包含小蓝点和蓝框数字）
pub const JS_GET_ALL_UNREAD_TOPICS: &str = r#"
(() => {
    const topics = [];
    const rows = document.querySelectorAll('tr.topic-list-item');
    rows.forEach((row, idx) => {
        // 小蓝点：全新未读主题
        const newBadge = row.querySelector('.badge.new-topic');
        if (newBadge) {
            const link = row.querySelector('a.title');
            if (link && link.href && link.href.includes('/t/')) {
                topics.push({
                    url: link.href,
                    title: (link.textContent || '').trim().substring(0, 60),
                    index: idx
                });
            }
            return;
        }
        // 蓝框数字：有未读帖子
        const unreadBadge = row.querySelector('.badge.unread-posts');
        if (unreadBadge && unreadBadge.href) {
            let href = unreadBadge.href;
            if (href.startsWith('/')) href = 'https://linux.do' + href;
            if (href.includes('/t/')) {
                const link = row.querySelector('a.title');
                topics.push({
                    url: href,
                    title: (link ? link.textContent || '' : '').trim().substring(0, 60),
                    index: idx
                });
            }
        }
    });
    return JSON.stringify(topics);
})()
"#;

/// JS: 统计当前页面帖子数并返回新帖子ID列表
pub const JS_GET_POST_IDS: &str = r#"
(() => {
    const posts = document.querySelectorAll('.topic-post article');
    const ids = [];
    posts.forEach(p => {
        const id = p.dataset.postId || p.id;
        if (id) ids.push(id);
    });
    return JSON.stringify(ids);
})()
"#;

/// JS: 获取当前话题的总帖子数（从 Discourse 进度条或话题元数据中提取）
pub const JS_GET_TOPIC_POST_COUNT: &str = r#"
(() => {
    // 方式1: 从 timeline 中获取
    const total = document.querySelector('.timeline-replies .topic-replies-count, .timeline-footer-controls .total');
    if (total) {
        const n = parseInt(total.textContent.replace(/[^\d]/g, ''));
        if (n > 0) return n;
    }
    // 方式2: 从进度条获取
    const progress = document.querySelector('#topic-progress .nums .current-post-number + span');
    if (progress) {
        const n = parseInt(progress.textContent.replace(/[^\d]/g, ''));
        if (n > 0) return n;
    }
    // 方式3: 从 discourse 内部数据获取
    const topicController = document.querySelector('#topic-progress');
    if (topicController) {
        const total2 = topicController.querySelector('[data-topic-posts-count]');
        if (total2) return parseInt(total2.dataset.topicPostsCount) || 0;
    }
    return 0;
})()
"#;

/// JS: 尝试为帖子点赞，返回点赞结果
pub fn js_try_like_post(min_chars: u32) -> String {
    format!(r#"
(() => {{
    const posts = document.querySelectorAll('.topic-post');
    let liked = false;
    for (const post of posts) {{
        // 检查是否已处理过
        if (post.dataset.liuduLiked) continue;

        const cooked = post.querySelector('.cooked');
        if (!cooked) continue;

        const text = cooked.textContent || '';
        const chineseChars = (text.match(/[\u4e00-\u9fff]/g) || []).length;
        if (chineseChars < {min_chars}) continue;

        // 70% 概率点赞
        if (Math.random() > 0.7) {{
            post.dataset.liuduLiked = '1';
            continue;
        }}

        // 查找点赞按钮
        const selectors = ['button.toggle-like', 'button.like-count',
                           '.discourse-reactions-reaction-button', "button[class*='like']"];
        let btn = null;
        for (const sel of selectors) {{
            const b = post.querySelector(sel);
            if (b && b.offsetParent !== null) {{ btn = b; break; }}
        }}
        if (!btn) {{ post.dataset.liuduLiked = '1'; continue; }}

        // 检查是否已点赞
        const cls = btn.className || '';
        if (cls.includes('has-like') || cls.includes('my-likes') || cls.includes('liked')) {{
            post.dataset.liuduLiked = '1';
            continue;
        }}

        btn.click();
        post.dataset.liuduLiked = '1';
        liked = true;
        break; // 每次只点赞一个
    }}
    return liked;
}})()
"#, min_chars = min_chars)
}

/// JS: 滚动到页面底部
pub const JS_SCROLL_BOTTOM: &str = "window.scrollTo(0, document.body.scrollHeight)";

/// JS: 检查列表是否存在
pub const JS_CHECK_TOPIC_LIST: &str =
    "!!document.querySelector('tr.topic-list-item')";

/// JS: 注入 track_visit 请求，触发 Discourse 服务端记录主题浏览数
/// Page.navigate 会触发全页刷新，绕过 Discourse SPA 路由中的 track_visit AJAX 请求，
/// 因此需要在页面加载后手动发起等价请求。
pub const JS_TRACK_TOPIC_VIEW: &str = r#"
(() => {
    const m = window.location.pathname.match(/\/t\/(?:[^\/]+\/)?(\d+)/);
    if (!m) return false;
    const topicId = m[1];
    const csrf = document.querySelector('meta[name="csrf-token"]')?.content || '';
    fetch('/t/' + topicId + '.json?track_visit=true&forceLoad=true', {
        headers: {
            'Accept': 'application/json, text/javascript, */*; q=0.01',
            'X-Requested-With': 'XMLHttpRequest',
            'Discourse-Logged-In': 'true',
            'Discourse-Present': 'true',
            'Discourse-Track-View': 'true',
            'Discourse-Track-View-Topic-Id': topicId,
            'X-CSRF-Token': csrf
        }
    });
    return true;
})()
"#;

/// JS: 通过 Discourse 内部 Ember API 获取话题信息
/// 返回 JSON: { "total": N, "highestPostNumber": N, "lastReadPostNumber": N, "canAppend": bool }
/// highestPostNumber: 话题中最高帖子编号（含已删除的编号）
/// lastReadPostNumber: 当前用户已读到的帖子编号（0=未读过）
/// 未读帖子数 = highestPostNumber - lastReadPostNumber
pub const JS_GET_POST_STREAM: &str = r#"
(() => {
    try {
        const container = window.Discourse && (Discourse.__container__ || Discourse.__registry__);
        if (container) {
            const tc = container.lookup('controller:topic');
            if (tc) {
                const model = tc.get('model');
                const ps = model && model.get('postStream');
                if (ps) {
                    const stream = ps.get('stream') || [];
                    return JSON.stringify({
                        total: stream.length,
                        highestPostNumber: model.get('highest_post_number') || 0,
                        lastReadPostNumber: model.get('last_read_post_number') || 0,
                        canAppend: !!ps.get('canAppendMore')
                    });
                }
            }
        }
    } catch(e) {}
    return '';
})()
"#;

/// JS: 触发 Discourse 加载下一批帖子到 DOM（通过 Ember 内部 API，不依赖滚动触发）
/// 返回: "loaded"（成功加载更多）/ "no_more"（已全部加载）/ "no_api"（API 不可用）
pub const JS_LOAD_NEXT_POSTS: &str = r#"
(async () => {
    try {
        const container = window.Discourse && (Discourse.__container__ || Discourse.__registry__);
        if (container) {
            const tc = container.lookup('controller:topic');
            if (tc) {
                const ps = tc.get('model.postStream');
                if (ps && ps.appendMore) {
                    if (ps.get('canAppendMore')) {
                        await ps.appendMore();
                        return 'loaded';
                    }
                    return 'no_more';
                }
            }
        }
    } catch(e) {}
    return 'no_api';
})()
"#;

/// JS: 增强版滚动到页面底部（用于 DOM 回退模式）
/// 在标准 scrollTo 基础上主动派发 scroll 事件，
/// 并尝试将最后一个帖子滚入视口，尽量触发 Discourse 的懒加载。
pub const JS_SCROLL_BOTTOM_ENHANCED: &str = r#"
(() => {
    window.scrollTo(0, document.body.scrollHeight);
    window.dispatchEvent(new Event('scroll', {bubbles: true}));
    const posts = document.querySelectorAll('.topic-post');
    if (posts.length > 0) {
        posts[posts.length - 1].scrollIntoView({behavior: 'instant', block: 'end'});
    }
})()
"#;

/// JS: 向 Discourse 服务端发送帖子阅读时间报告，使帖子被记录为已读
/// from_post 和 to_post 指定上报的帖子号范围（闭区间），
/// 只上报未读范围，避免重复上报已读帖子。
/// 自动分批（每批最多 500 个帖子号），返回格式: "sent:数量" / "no_api" / "error:原因"
pub fn js_send_post_timings(from_post: u32, to_post: u32) -> String {
    format!(r#"
(async () => {{
    try {{
        const container = window.Discourse && (Discourse.__container__ || Discourse.__registry__);
        if (!container) return 'no_api';

        const tc = container.lookup('controller:topic');
        if (!tc) return 'no_api';

        const model = tc.get('model');
        if (!model) return 'no_api';

        const topicId = model.get('id');
        if (!topicId) return 'no_data';

        const fromPost = {from_post};
        const toPost = {to_post};
        if (fromPost > toPost || fromPost <= 0) return 'no_data';

        const csrf = document.querySelector('meta[name="csrf-token"]')?.content || '';

        const batchSize = 500;
        let sentCount = 0;
        for (let start = fromPost; start <= toPost; start += batchSize) {{
            const end = Math.min(start + batchSize - 1, toPost);
            const params = new URLSearchParams();
            params.append('topic_id', topicId);
            params.append('topic_time', String((end - start + 1) * 4000));
            for (let i = start; i <= end; i++) {{
                params.append('timings[' + i + ']', '4000');
            }}

            await fetch('/topics/timings', {{
                method: 'POST',
                headers: {{
                    'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
                    'X-CSRF-Token': csrf,
                    'X-Requested-With': 'XMLHttpRequest',
                    'Discourse-Present': 'true'
                }},
                body: params.toString()
            }});
            sentCount += (end - start + 1);
        }}

        return 'sent:' + sentCount;
    }} catch(e) {{
        return 'error:' + e.message;
    }}
}})()
"#, from_post = from_post, to_post = to_post)
}

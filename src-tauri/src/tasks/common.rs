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

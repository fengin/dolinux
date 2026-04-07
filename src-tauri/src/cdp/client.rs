use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::protocol::{CdpRequest, CdpResponse, PageInfo};

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;

/// 轻量 CDP 客户端 - 通过 WebSocket 直接与 Chrome 通信
pub struct CdpClient {
    writer: Arc<Mutex<WsSink>>,
    pending: Arc<Mutex<HashMap<u32, oneshot::Sender<Result<Value, String>>>>>,
    next_id: AtomicU32,
    connected: Arc<AtomicBool>,
}

impl CdpClient {
    /// 发现 Chrome 调试端口上的可用页面
    pub async fn discover_pages(port: u16) -> Result<Vec<PageInfo>, String> {
        let url = format!("http://127.0.0.1:{}/json/list", port);
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("无法连接到 Chrome 调试端口 {}: {}", port, e))?;
        let pages: Vec<PageInfo> = resp
            .json()
            .await
            .map_err(|e| format!("解析页面列表失败: {}", e))?;
        Ok(pages)
    }

    /// 选择一个合适的页面（跳过扩展页、devtools 页）
    pub fn pick_page(pages: &[PageInfo]) -> Option<&PageInfo> {
        // 优先选择有实际内容的页面（非 about:blank）
        let mut best: Option<&PageInfo> = None;
        for page in pages {
            if page.page_type != "page" {
                continue;
            }
            if page.url.starts_with("chrome-extension://")
                || page.url.starts_with("devtools://")
                || page.url.starts_with("chrome://")
            {
                continue;
            }
            if page.web_socket_debugger_url.is_none() {
                continue;
            }
            match best {
                None => best = Some(page),
                Some(b) if b.url == "about:blank" && page.url != "about:blank" => {
                    best = Some(page);
                }
                _ => {}
            }
        }
        best
    }

    /// 连接到指定页面的 WebSocket
    pub async fn connect(ws_url: &str) -> Result<Arc<Self>, String> {
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .map_err(|e| format!("WebSocket 连接失败: {}", e))?;

        let (write, mut read) = ws_stream.split();

        let pending: Arc<Mutex<HashMap<u32, oneshot::Sender<Result<Value, String>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(AtomicBool::new(true));

        // 启动消息读取任务
        let pending_clone = pending.clone();
        let connected_clone = connected.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(resp) = serde_json::from_str::<CdpResponse>(&text) {
                            // 如果有 id，是对某个请求的响应
                            if let Some(id) = resp.id {
                                let mut map = pending_clone.lock().await;
                                if let Some(sender) = map.remove(&id) {
                                    if let Some(error) = resp.error {
                                        let _ = sender.send(Err(format!(
                                            "CDP Error {}: {}",
                                            error.code, error.message
                                        )));
                                    } else {
                                        let _ = sender.send(Ok(
                                            resp.result.unwrap_or(Value::Null)
                                        ));
                                    }
                                }
                            }
                            // 无 id 的是事件，忽略
                        }
                    }
                    Ok(Message::Close(_)) => {
                        connected_clone.store(false, Ordering::SeqCst);
                        break;
                    }
                    Err(_) => {
                        connected_clone.store(false, Ordering::SeqCst);
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Arc::new(Self {
            writer: Arc::new(Mutex::new(write)),
            pending,
            next_id: AtomicU32::new(1),
            connected,
        }))
    }

    /// 发送 CDP 命令并等待响应
    pub async fn send_command(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err("CDP 连接已断开".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
        };

        let msg_text =
            serde_json::to_string(&request).map_err(|e| format!("序列化失败: {}", e))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        self.writer
            .lock()
            .await
            .send(Message::Text(msg_text.into()))
            .await
            .map_err(|e| format!("发送消息失败: {}", e))?;

        // 超时等待响应 (30秒)
        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("响应通道已关闭".to_string()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err("CDP 命令超时".to_string())
            }
        }
    }

    /// 执行 JavaScript 并返回结果
    pub async fn evaluate(&self, expression: &str) -> Result<Value, String> {
        let result = self
            .send_command(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true
                }),
            )
            .await?;

        // 检查 JS 异常
        if let Some(exception) = result.get("exceptionDetails") {
            let msg = exception
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("Unknown JS error");
            return Err(format!("JS 执行异常: {}", msg));
        }

        // 提取 result.value
        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// 执行 JS 并返回字符串结果
    pub async fn evaluate_as_string(&self, expression: &str) -> Result<String, String> {
        let val = self.evaluate(expression).await?;
        Ok(val.as_str().unwrap_or("").to_string())
    }

    /// 执行 JS 并返回 i64 结果
    pub async fn evaluate_as_i64(&self, expression: &str) -> Result<i64, String> {
        let val = self.evaluate(expression).await?;
        Ok(val.as_i64().unwrap_or(0))
    }

    /// 导航到指定 URL
    pub async fn navigate(&self, url: &str) -> Result<(), String> {
        self.send_command("Page.navigate", json!({ "url": url }))
            .await?;
        Ok(())
    }

    /// 导航并等待页面加载完成
    pub async fn navigate_and_wait(&self, url: &str, timeout_ms: u64) -> Result<(), String> {
        self.navigate(url).await?;

        let start = std::time::Instant::now();
        // 先等待一小段时间让导航开始
        tokio::time::sleep(Duration::from_millis(500)).await;

        loop {
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Ok(()); // 超时，继续执行
            }

            match self.evaluate("document.readyState").await {
                Ok(val) => {
                    if val.as_str() == Some("complete") {
                        // 额外等待一下让 Discourse JS 初始化
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        return Ok(());
                    }
                }
                Err(_) => {
                    // 导航中，JS 上下文可能暂时不可用
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    /// 是否仍然连接
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
}

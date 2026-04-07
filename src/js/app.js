/* ========== 主应用逻辑 ========== */

let APP = {
    connected: false,
    taskRunning: false,
    currentTask: null,
    config: null,
};

// ========== 初始化 ==========
document.addEventListener('DOMContentLoaded', async () => {
    Logger.init();

    // 加载配置
    try {
        APP.config = await API.getConfig();
        applyConfigToUI(APP.config);
    } catch (e) {
        Logger.add('加载配置失败: ' + e, 'error');
    }

    // 绑定事件
    bindEvents();

    // 订阅后端事件
    await subscribeEvents();

    // 检查连接状态
    try {
        APP.connected = await API.checkChromeStatus();
        updateConnectionUI();
    } catch (_) {}
});

// ========== 事件绑定 ==========
function bindEvents() {
    // Chrome 连接
    document.getElementById('btn-launch').addEventListener('click', onLaunchChrome);
    document.getElementById('btn-connect').addEventListener('click', onConnectChrome);

    // 任务
    document.getElementById('btn-topic-start').addEventListener('click', () => onStartTask('topic'));
    document.getElementById('btn-post-start').addEventListener('click', () => onStartTask('post'));

    // 日志
    document.getElementById('btn-log-clear').addEventListener('click', () => Logger.clear());
    document.getElementById('btn-log-toggle').addEventListener('click', () => Logger.toggle());

    // 设置弹窗
    document.getElementById('btn-settings').addEventListener('click', openSettings);
    document.getElementById('btn-settings-close').addEventListener('click', closeSettings);
    document.getElementById('btn-settings-save').addEventListener('click', saveSettings);
    document.getElementById('btn-settings-reset').addEventListener('click', resetSettings);
    document.getElementById('btn-add-topic-url').addEventListener('click', () => addUrlRow('settings-topic-urls'));
    document.getElementById('btn-add-post-url').addEventListener('click', () => addUrlRow('settings-post-urls'));

    // 关于弹窗
    document.getElementById('btn-about').addEventListener('click', openAbout);
    document.getElementById('btn-about-close').addEventListener('click', closeAbout);

    // 关于页面的外部链接
    document.querySelectorAll('.about-link').forEach(link => {
        link.addEventListener('click', (e) => {
            e.preventDefault();
            const url = link.dataset.url;
            if (url && window.__TAURI__) {
                window.__TAURI__.shell.open(url);
            }
        });
    });

    // 点击遮罩关闭弹窗
    document.querySelectorAll('.modal-overlay').forEach(overlay => {
        overlay.addEventListener('click', (e) => {
            if (e.target === overlay) {
                overlay.style.display = 'none';
            }
        });
    });
}

// ========== 事件订阅 ==========
async function subscribeEvents() {
    await API.onTaskLog((data) => {
        Logger.add(data.message, data.level, data.timestamp);
    });

    await API.onTaskProgress((data) => {
        updateProgress(data);
        if (data.finished) {
            APP.taskRunning = false;
            APP.currentTask = null;
            updateTaskButtons();
        }
    });

    await API.onChromeStatus((connected) => {
        APP.connected = connected;
        updateConnectionUI();
    });
}

// ========== Chrome 操作 ==========
async function onLaunchChrome() {
    const btn = document.getElementById('btn-launch');
    btn.disabled = true;
    btn.textContent = '启动中...';
    try {
        const msg = await API.launchChrome();
        Logger.add(msg, 'info');
        // 启动后等几秒再连接
        Logger.add('等待 Chrome 启动...', 'info');
        setTimeout(async () => {
            try {
                await onConnectChrome();
            } catch(_) {}
        }, 3000);
    } catch (e) {
        Logger.add('启动失败: ' + e, 'error');
    } finally {
        btn.disabled = false;
        btn.textContent = '启动 Chrome';
    }
}

async function onConnectChrome() {
    const btn = document.getElementById('btn-connect');
    btn.disabled = true;
    btn.textContent = '连接中...';
    try {
        const msg = await API.connectChrome();
        APP.connected = true;
        updateConnectionUI();
    } catch (e) {
        Logger.add('连接失败: ' + e, 'error');
        APP.connected = false;
        updateConnectionUI();
    } finally {
        btn.disabled = false;
        btn.textContent = APP.connected ? '已连接' : '连接';
    }
}

function updateConnectionUI() {
    const dot = document.getElementById('status-dot');
    const text = document.getElementById('status-text');
    const connectBtn = document.getElementById('btn-connect');

    if (APP.connected) {
        dot.className = 'status-dot connected';
        text.textContent = '已连接';
        connectBtn.textContent = '重新连接';
    } else {
        dot.className = 'status-dot disconnected';
        text.textContent = '未连接';
        connectBtn.textContent = '连接';
    }

    updateTaskButtons();
}

// ========== 任务操作 ==========
async function onStartTask(taskType) {
    if (APP.taskRunning) {
        // 停止任务
        try {
            await API.stopTask();
        } catch (e) {
            Logger.add('停止失败: ' + e, 'error');
        }
        return;
    }

    if (!APP.connected) {
        Logger.add('请先连接 Chrome', 'warn');
        return;
    }

    // 同步界面配置到后端
    await syncConfigFromUI();

    // 显示进度条
    showProgress(taskType);

    try {
        await API.startTask(taskType);
        APP.taskRunning = true;
        APP.currentTask = taskType;
        updateTaskButtons();
    } catch (e) {
        Logger.add('启动任务失败: ' + e, 'error');
        hideProgress(taskType);
    }
}

function updateTaskButtons() {
    const topicBtn = document.getElementById('btn-topic-start');
    const postBtn = document.getElementById('btn-post-start');

    if (APP.taskRunning) {
        if (APP.currentTask === 'topic') {
            topicBtn.textContent = '⏹ 停止';
            topicBtn.className = 'btn btn-stop';
            topicBtn.disabled = false;
            postBtn.disabled = true;
        } else if (APP.currentTask === 'post') {
            postBtn.textContent = '⏹ 停止';
            postBtn.className = 'btn btn-stop';
            postBtn.disabled = false;
            topicBtn.disabled = true;
        }
    } else {
        topicBtn.textContent = '▶ 开始';
        topicBtn.className = 'btn btn-start';
        topicBtn.disabled = !APP.connected;

        postBtn.textContent = '▶ 开始';
        postBtn.className = 'btn btn-start';
        postBtn.disabled = !APP.connected;
    }
}

// ========== 进度更新 ==========
function updateProgress(data) {
    if (data.task_type === 'topic') {
        const fill = document.getElementById('topic-progress-fill');
        const text = document.getElementById('topic-progress-text');
        const pct = data.total > 0 ? (data.current / data.total * 100) : 0;
        fill.style.width = Math.min(pct, 100) + '%';
        text.textContent = `${data.current}/${data.total}`;
    } else if (data.task_type === 'post') {
        const fill = document.getElementById('post-progress-fill');
        const text = document.getElementById('post-progress-text');
        const pct = data.total > 0 ? (data.current / data.total * 100) : 0;
        fill.style.width = Math.min(pct, 100) + '%';
        text.textContent = `帖子 ${data.current}/${data.total}  点赞 ${data.likes}`;
    }
}

function showProgress(taskType) {
    const id = taskType === 'topic' ? 'topic-progress-container' : 'post-progress-container';
    document.getElementById(id).style.display = 'flex';
}

function hideProgress(taskType) {
    const id = taskType === 'topic' ? 'topic-progress-container' : 'post-progress-container';
    document.getElementById(id).style.display = 'none';
}

// ========== 配置同步 ==========
function applyConfigToUI(config) {
    if (!config) return;

    document.getElementById('input-port').value = config.chrome.debug_port;
    document.getElementById('topic-target').value = config.topic_task.target_count;
    document.getElementById('post-target').value = config.post_task.target_count;
    document.getElementById('post-skip').value = config.post_task.skip_top;
    document.getElementById('post-delay').value = config.post_task.delay_ms;
    document.getElementById('post-like-enabled').checked = config.post_task.like_enabled;
    document.getElementById('post-like-chars').value = config.post_task.like_min_chars;
    document.getElementById('post-like-max').value = config.post_task.like_max_count;
}

async function syncConfigFromUI() {
    if (!APP.config) {
        APP.config = await API.getConfig();
    }

    APP.config.chrome.debug_port = parseInt(document.getElementById('input-port').value) || 9222;
    APP.config.topic_task.target_count = parseInt(document.getElementById('topic-target').value) || 500;
    APP.config.post_task.target_count = parseInt(document.getElementById('post-target').value) || 10000;
    APP.config.post_task.skip_top = parseInt(document.getElementById('post-skip').value) || 3;
    APP.config.post_task.delay_ms = parseInt(document.getElementById('post-delay').value) || 200;
    APP.config.post_task.like_enabled = document.getElementById('post-like-enabled').checked;
    APP.config.post_task.like_min_chars = parseInt(document.getElementById('post-like-chars').value) || 50;
    APP.config.post_task.like_max_count = parseInt(document.getElementById('post-like-max').value) || 30;

    try {
        await API.saveConfig(APP.config);
    } catch (e) {
        Logger.add('保存配置失败: ' + e, 'warn');
    }
}

// ========== 设置弹窗 ==========
function openSettings() {
    if (!APP.config) return;

    document.getElementById('settings-chrome-path').value = APP.config.chrome.executable_path;
    document.getElementById('settings-chrome-port').value = APP.config.chrome.debug_port;
    document.getElementById('settings-chrome-datadir').value = APP.config.chrome.user_data_dir;
    document.getElementById('settings-auto-close').checked = APP.config.auto_close_chrome;

    renderUrlList('settings-topic-urls', APP.config.topic_task.entry_urls);
    renderUrlList('settings-post-urls', APP.config.post_task.entry_urls);

    document.getElementById('modal-settings').style.display = 'flex';
}

function closeSettings() {
    document.getElementById('modal-settings').style.display = 'none';
}

async function saveSettings() {
    APP.config.chrome.executable_path = document.getElementById('settings-chrome-path').value;
    APP.config.chrome.debug_port = parseInt(document.getElementById('settings-chrome-port').value) || 9222;
    APP.config.chrome.user_data_dir = document.getElementById('settings-chrome-datadir').value;
    APP.config.auto_close_chrome = document.getElementById('settings-auto-close').checked;

    APP.config.topic_task.entry_urls = collectUrls('settings-topic-urls');
    APP.config.post_task.entry_urls = collectUrls('settings-post-urls');

    // 同步端口到主界面
    document.getElementById('input-port').value = APP.config.chrome.debug_port;

    try {
        await API.saveConfig(APP.config);
        Logger.add('设置已保存', 'info');
        closeSettings();
    } catch (e) {
        Logger.add('保存设置失败: ' + e, 'error');
    }
}

async function resetSettings() {
    try {
        // 获取默认配置需要重新构造，这里简单处理
        const defaults = {
            chrome: {
                executable_path: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
                debug_port: 9222,
                user_data_dir: 'D:\\temp\\chrome_liudu'
            },
            topic_task: {
                target_count: 500,
                entry_urls: [
                    'https://linux.do/latest',
                    'https://linux.do/c/develop/4',
                    'https://linux.do/c/resource/14',
                    'https://linux.do/c/welfare/36',
                    'https://linux.do/c/news/34'
                ]
            },
            post_task: {
                target_count: 10000,
                skip_top: 3,
                delay_ms: 200,
                entry_urls: ['https://linux.do/hot?order=posts'],
                like_enabled: true,
                like_min_chars: 50,
                like_max_count: 30
            },
            auto_close_chrome: false
        };
        APP.config = defaults;
        openSettings(); // 刷新弹窗内容
        Logger.add('已恢复默认设置（需点击保存生效）', 'info');
    } catch (e) {
        Logger.add('恢复失败: ' + e, 'error');
    }
}

// ========== URL 列表管理 ==========
function renderUrlList(containerId, urls) {
    const container = document.getElementById(containerId);
    container.innerHTML = '';
    urls.forEach(url => {
        addUrlRow(containerId, url);
    });
}

function addUrlRow(containerId, value = '') {
    const container = document.getElementById(containerId);
    const row = document.createElement('div');
    row.className = 'url-row';
    row.innerHTML = `
        <input type="text" class="url-input" value="${value}" placeholder="https://linux.do/...">
        <button class="btn-url-remove" onclick="this.parentElement.remove()">✕</button>
    `;
    container.appendChild(row);
}

function collectUrls(containerId) {
    const container = document.getElementById(containerId);
    const inputs = container.querySelectorAll('.url-input');
    const urls = [];
    inputs.forEach(input => {
        const val = input.value.trim();
        if (val) urls.push(val);
    });
    return urls;
}

// ========== 关于弹窗 ==========
function openAbout() {
    document.getElementById('modal-about').style.display = 'flex';
}

function closeAbout() {
    document.getElementById('modal-about').style.display = 'none';
}

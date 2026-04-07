# ⚡ 流读 — Linux.do 阅读助手

> 轻量、高效、一键运行的 Linux.do 论坛阅读辅助工具

**流读**（LiuDu）是一款基于 **Rust + Tauri** 构建的桌面应用，帮助 [Linux.do](https://linux.do) 用户高效完成日常阅读任务。无需安装 Python 环境，无需命令行操作，双击即用。

---

## ✨ 核心功能

### 🔖 刷主题阅读

- 自动浏览论坛未读话题，增加主题阅读数
- **模拟真人行为**：渐进式滚动、随机停顿、偶尔回滚，触发 Discourse 的 `timings` 上报机制
- 支持多入口 URL（latest / 各分区），连续 3 轮无新内容自动切换下一入口
- 可配置目标阅读数量

### 📄 刷帖子 + 点赞

- 从入口页收集未读话题，逐一进入快速浏览帖子
- **快速阅读模式**：不模拟人类复杂行为，通过快速滚动+可配延时高效完成
- 智能等待 Discourse 瀑布流加载，确保每个话题的帖子被完整读取
- **自动点赞**（可选）：
  - 中文字数 ≥ 阈值时才点赞
  - 70% 概率随机点赞，避免机械化
  - 可配置点赞上限
- 支持跳过前 N 个热门话题

### ⚙️ 便捷配置

- 所有参数均通过界面配置，无需编辑文件
- Chrome 路径、调试端口、用户数据目录可自定义
- 入口 URL 列表可视化管理（支持增删）
- 配置自动持久化，下次启动自动加载

### 🔔 系统托盘

- 关闭窗口自动最小化到系统托盘，后台继续运行
- 托盘菜单：显示窗口 / 退出
- 单击托盘图标快速还原窗口

---

## 🚀 快速开始

### 使用方式

1. **双击启动**流读应用
2. 点击 **「启动 Chrome」**，自动打开带调试端口的 Chrome 浏览器
3. 在 Chrome 中 **登录 Linux.do**
4. 回到流读，点击 **「连接」**，状态变为 ✅ 已连接
5. 配置任务参数，点击 **▶ 开始**
6. 实时查看运行日志和进度条
7. 随时可点击 **⏹ 停止** 安全中断任务

### 系统要求

- **Windows 10** (1803+) / **Windows 11**（已内置 WebView2）
- **macOS 11+** / **Linux**（需 WebKitGTK）
- 已安装 **Google Chrome** 浏览器

---

## 🏗️ 技术架构

### 技术栈

| 层 | 技术 | 说明 |
|---|---|---|
| **框架** | [Tauri v2](https://v2.tauri.app/) | 原生 WebView 渲染，极小体积 |
| **后端** | Rust | 系统级性能，内存安全，异步运行时 tokio |
| **前端** | Vanilla HTML/CSS/JS | 零框架依赖，前端资源 < 50KB |
| **浏览器通信** | CDP（Chrome DevTools Protocol） | 基于 WebSocket 直连 Chrome |
| **配置存储** | JSON | 存储于系统 AppData 目录 |

### 产品指标

| 指标 | 数值 |
|---|---|
| 安装包体积 | **~1.8 MB** (NSIS) |
| 可执行文件 | **~5 MB** |
| 启动时间 | **< 1 秒** |
| 内存占用 | **~15 MB** |
| 前端框架依赖 | **0** |

### 轻量 CDP 通信

没有使用 chromiumoxide 等重量级 crate（含 ~60K 行 CDP 类型定义），而是基于 `tokio-tungstenite` 实现了极简 CDP 客户端，仅覆盖 3 个实际需要的 CDP 命令：

```
Runtime.evaluate    →  所有 DOM 操作的基础（查找元素、点赞、计数等）
Page.navigate       →  页面导航
GET /json/list      →  发现 Chrome 标签页
```

### 项目结构

```
dolinux/
├── package.json                # npm 配置（仅 Tauri CLI）
├── src/                        # 前端
│   ├── index.html              # 主页面（单页应用）
│   ├── css/styles.css          # 浅色简洁主题
│   └── js/
│       ├── api.js              # Tauri invoke/listen 封装
│       ├── logger.js           # 日志面板组件
│       └── app.js              # 主逻辑与状态管理
└── src-tauri/                  # Rust 后端
    ├── Cargo.toml              # 依赖 + 体积优化配置
    ├── tauri.conf.json         # 窗口 / 安全 / 打包配置
    └── src/
        ├── main.rs             # 入口
        ├── lib.rs              # Tauri Commands + 托盘 + 事件
        ├── config.rs           # 配置管理（JSON 持久化）
        ├── state.rs            # 全局状态（线程安全）
        ├── chrome.rs           # Chrome 启动 / 检测
        ├── cdp/
        │   ├── client.rs       # WebSocket CDP 客户端
        │   └── protocol.rs     # CDP 消息类型定义
        └── tasks/
            ├── common.rs       # JS 脚本注入 + 滚动行为
            ├── topic_reader.rs # 刷主题阅读
            └── post_reader.rs  # 刷帖子 + 点赞
```

### 构建指南

**环境准备：**

- [Rust](https://rustup.rs/) (1.70+)
- [Node.js](https://nodejs.org/) (18+)
- Windows: Microsoft C++ Build Tools

**开发与构建：**

```bash
# 安装依赖
npm install

# 开发模式（热重载前端）
npx tauri dev

# 构建 Release
npx tauri build

# 仅编译（不打包安装程序）
cd src-tauri && cargo build --release
```

构建产物位于 `src-tauri/target/release/`。

---

## 📌 注意事项

- 本工具通过 Chrome 远程调试协议（CDP）与浏览器交互，**需要先在 Chrome 中登录 Linux.do**
- 刷主题阅读采用模拟真人滚动方式，确保 Discourse 的阅读追踪机制正常记录
- 刷帖子延时建议 ≥ 100ms，过低可能导致 Discourse 不记录阅读数
- 关闭应用窗口会最小化到系统托盘，需通过托盘菜单「退出」才会真正退出

---

## 👤 关于

| | |
|---|---|
| **作者** | 凌封 |
| **开源框架** | [auto-chrome](https://github.com/fengin/auto-chrome) — 基于 Playwright 的浏览器自动化框架 |
| **技术圈** | [aibook.ren](https://aibook.ren)（AI全书）|

> 流读是 [auto-chrome](https://github.com/fengin/auto-chrome) 框架在 Linux.do 场景下的轻量独立实现，使用 Rust + Tauri 重写以获得极致的体积和性能。

---

## 📄 License

MIT

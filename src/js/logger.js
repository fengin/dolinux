/* ========== 日志面板组件 ========== */

const Logger = {
    container: null,
    maxEntries: 500,
    collapsed: false,

    init() {
        this.container = document.getElementById('log-content');
    },

    add(message, level = 'info', timestamp = null) {
        if (!this.container) return;

        const entry = document.createElement('div');
        entry.className = `log-entry log-${level}`;

        const time = timestamp || new Date().toLocaleTimeString('zh-CN', { hour12: false });
        entry.innerHTML = `<span class="log-time">${time}</span><span class="log-msg">${this.escapeHtml(message)}</span>`;

        this.container.appendChild(entry);

        // 限制日志条数
        while (this.container.children.length > this.maxEntries) {
            this.container.removeChild(this.container.firstChild);
        }

        // 自动滚动到底部
        this.container.scrollTop = this.container.scrollHeight;
    },

    clear() {
        if (this.container) {
            this.container.innerHTML = '';
        }
    },

    toggle() {
        this.collapsed = !this.collapsed;
        if (this.container) {
            this.container.style.display = this.collapsed ? 'none' : 'block';
        }
        const btn = document.getElementById('btn-log-toggle');
        if (btn) {
            btn.textContent = this.collapsed ? '展开' : '折叠';
        }
    },

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
};

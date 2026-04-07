/* ========== Tauri API 封装 ========== */

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const API = {
    async launchChrome() {
        return await invoke('launch_chrome');
    },

    async connectChrome() {
        return await invoke('connect_chrome');
    },

    async disconnectChrome() {
        return await invoke('disconnect_chrome');
    },

    async checkChromeStatus() {
        return await invoke('check_chrome_status');
    },

    async startTask(taskType) {
        return await invoke('start_task', { taskType });
    },

    async stopTask() {
        return await invoke('stop_task');
    },

    async getConfig() {
        return await invoke('get_config');
    },

    async saveConfig(newConfig) {
        return await invoke('save_config', { newConfig });
    },

    async isTaskRunning() {
        return await invoke('is_task_running');
    },

    // 事件监听
    async onTaskLog(callback) {
        return await listen('task-log', (event) => callback(event.payload));
    },

    async onTaskProgress(callback) {
        return await listen('task-progress', (event) => callback(event.payload));
    },

    async onChromeStatus(callback) {
        return await listen('chrome-status', (event) => callback(event.payload));
    }
};

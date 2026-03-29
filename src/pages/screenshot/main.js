import {createApp} from 'vue'
import App from './App.vue'

const screenshotBoot = window.__SCREENSHOT_BOOT__ || {
    pendingData: null,
    pendingStartSessionId: 0
}
if (typeof screenshotBoot.pendingStartSessionId !== 'number') {
    screenshotBoot.pendingStartSessionId = Number(screenshotBoot.pendingStartSessionId) || 0
}
window.__SCREENSHOT_BOOT__ = screenshotBoot

window.addEventListener('screenshot-data', (event) => {
    screenshotBoot.pendingData = event.detail || null
})

window.addEventListener('start-region-select', (event) => {
    const sessionId = Number(event?.detail?.session_id) || 0
    screenshotBoot.pendingStartSessionId = sessionId || screenshotBoot.pendingStartSessionId || 0
})

createApp(App).mount('#app')

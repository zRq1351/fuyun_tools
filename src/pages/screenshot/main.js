import '../../utils/disableContextMenu'
import {createApp} from 'vue'
import App from './App.vue'

const screenshotBoot = window.__SCREENSHOT_BOOT__ || {
    pendingData: null,
    pendingStartSessionId: 0,
    pendingMode: null
}
if (typeof screenshotBoot.pendingStartSessionId !== 'number') {
    screenshotBoot.pendingStartSessionId = Number(screenshotBoot.pendingStartSessionId) || 0
}
if (typeof screenshotBoot.pendingMode !== 'string') {
    screenshotBoot.pendingMode = null
}
window.__SCREENSHOT_BOOT__ = screenshotBoot
window.__SCREENSHOT_BOOT_READY__ = true

window.addEventListener('screenshot-data', (event) => {
    screenshotBoot.pendingData = event.detail || null
})

window.addEventListener('start-region-select', (event) => {
    const sessionId = Number(event?.detail?.session_id) || 0
    const mode = String(event?.detail?.mode || 'screenshot')
    screenshotBoot.pendingStartSessionId = sessionId || screenshotBoot.pendingStartSessionId || 0
    screenshotBoot.pendingMode = mode
})

createApp(App).mount('#app')

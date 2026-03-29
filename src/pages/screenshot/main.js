import {createApp} from 'vue'
import App from './App.vue'

const screenshotBoot = window.__SCREENSHOT_BOOT__ || {
    pendingData: null,
    pendingStart: false
}
window.__SCREENSHOT_BOOT__ = screenshotBoot

window.addEventListener('screenshot-data', (event) => {
    screenshotBoot.pendingData = event.detail || null
})

window.addEventListener('start-region-select', () => {
    screenshotBoot.pendingStart = true
})

createApp(App).mount('#app')

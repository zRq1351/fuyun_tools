import '../../utils/disableContextMenu'
import {createApp} from 'vue'
import '../shared/theme-variables.css'
import {initTheme} from '../../utils/themeManager'
import App from './App.vue'

// 初始化主题
initTheme()

createApp(App).mount('#app')

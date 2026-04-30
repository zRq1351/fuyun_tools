import '../../utils/disableContextMenu'
import {createApp} from 'vue'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'
import '../shared/theme-variables.css'
import {initTheme} from '../../utils/themeManager'
import App from './App.vue'

// 初始化主题
initTheme()

const app = createApp(App)
app.use(ElementPlus)
app.mount('#app')

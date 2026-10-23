import {createPageApp} from '../../utils/createPageApp'
// ElLoading.service() 以编程方式使用，unplugin 的按需样式注入覆盖不到（只扫模板/自动导入），
// 显式补 loading 样式（本页是唯一使用方，体积可忽略）
import 'element-plus/es/components/loading/style/css'
import App from './App.vue'

createPageApp(App)

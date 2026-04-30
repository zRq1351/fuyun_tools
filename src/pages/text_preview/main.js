import {createPageApp} from '../../utils/createPageApp'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import App from './App.vue'

createPageApp(App, {
    setup(app) {
        app.use(ElementPlus)
    }
})

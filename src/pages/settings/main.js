import {createPageApp} from '../../utils/createPageApp'
import ElementPlus from 'element-plus'
import App from './App.vue'

createPageApp(App, {
    setup(app) {
        app.use(ElementPlus)
    }
})

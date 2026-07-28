import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import {ElementPlusResolver} from 'unplugin-vue-components/resolvers'
import {resolve} from 'path'

export default defineConfig(({mode}) => {
    const isDevMode = mode === 'development'
    const vueRuntimePath = resolve(__dirname, 'node_modules/vue/dist/vue.runtime.esm-bundler.js')
    return {
        plugins: [
            vue(),
            AutoImport({
                imports: ['vue'],
                resolvers: [ElementPlusResolver({importStyle: 'css'})],
                dts: false,
            }),
            Components({
                resolvers: [ElementPlusResolver({importStyle: 'css'})],
                dts: false,
            }),
        ],
        define: {
            __DEV_PANEL__: JSON.stringify(isDevMode),
        },
        optimizeDeps: {
            include: [
                'vue',
                'element-plus',
                'element-plus/es',
                'element-plus/dist/locale/zh-cn',
                '@element-plus/icons-vue',
                'element-plus/es/components/alert/style/css',
                'element-plus/es/components/base/style/css',
                'element-plus/es/components/button/style/css',
                'element-plus/es/components/card/style/css',
                'element-plus/es/components/checkbox/style/css',
                'element-plus/es/components/config-provider/style/css',
                'element-plus/es/components/empty/style/css',
                'element-plus/es/components/form-item/style/css',
                'element-plus/es/components/form/style/css',
                'element-plus/es/components/icon/style/css',
                'element-plus/es/components/input-number/style/css',
                'element-plus/es/components/input/style/css',
                'element-plus/es/components/link/style/css',
                'element-plus/es/components/loading/style/css',
                'element-plus/es/components/message-box/style/css',
                'element-plus/es/components/message/style/css',
                'element-plus/es/components/option/style/css',
                'element-plus/es/components/progress/style/css',
                'element-plus/es/components/radio-button/style/css',
                'element-plus/es/components/radio-group/style/css',
                'element-plus/es/components/select/style/css',
                'element-plus/es/components/switch/style/css',
                'element-plus/es/components/tag/style/css',
                'element-plus/es/components/tooltip/style/css',
            ],
        },
        build: {
            rollupOptions: {
                input: {
                    settings: resolve(__dirname, 'settings.html'),
                    clipboard: resolve(__dirname, 'clipboard.html'),
                    image_clipboard: resolve(__dirname, 'image_clipboard.html'),
                    image_preview: resolve(__dirname, 'image_preview.html'),
                    text_preview: resolve(__dirname, 'text_preview.html'),
                    selection_toolbar: resolve(__dirname, 'selection_toolbar.html'),
                    result_display: resolve(__dirname, 'result_display.html'),
                    screenshot: resolve(__dirname, 'screenshot.html'),
                    longshot_toolbar: resolve(__dirname, 'longshot_toolbar.html'),
                    longshot_border: resolve(__dirname, 'longshot_border.html'),
                    recording_toolbar: resolve(__dirname, 'recording_toolbar.html'),
                    pinned_image: resolve(__dirname, 'pinned_image.html'),
                    ocr_text: resolve(__dirname, 'ocr_text.html'),
                    launcher: resolve(__dirname, 'launcher.html'),
                    document_manager: resolve(__dirname, 'document_manager.html'),
                    document_manager_widget: resolve(__dirname, 'document_manager_widget.html'),
                },
                output: {
                    manualChunks(id) {
                        // Vue 核心生态
                        if (id.includes('/node_modules/vue/') || id.includes('/node_modules/@vue/') || id.includes('/node_modules/vue-')) {
                            return 'vendor-vue'
                        }
                        // Element Plus UI 框架
                        if (id.includes('/node_modules/element-plus/') || id.includes('/node_modules/@element-plus/')) {
                            return 'vendor-element-plus'
                        }
                        // 拖拽排序库
                        if (id.includes('/node_modules/sortablejs/') || id.includes('/node_modules/vuedraggable/')) {
                            return 'vendor-sortable'
                        }
                        // Markdown 渲染
                        if (id.includes('/node_modules/marked/') || id.includes('/node_modules/dompurify/') || id.includes('/node_modules/highlight.js/')) {
                            return 'vendor-markdown'
                        }
                        // 图标库
                        if (id.includes('/node_modules/lucide-vue-next/')) {
                            return 'vendor-icons'
                        }
                        // 虚拟滚动
                        if (id.includes('/node_modules/vue-virtual-scroller/')) {
                            return 'vendor-virtual'
                        }
                        // 工具库
                        if (id.includes('/node_modules/@vueuse/')) {
                            return 'vendor-utils'
                        }
                    }
                }
            },
            outDir: 'dist',
            emptyOutDir: true,
            chunkSizeWarningLimit: 2000,
        },
        resolve: {
            dedupe: ['vue'],
            alias: {
                '@': resolve(__dirname, '.'),
                vue: vueRuntimePath,
                '@dev/DeveloperSettings': isDevMode
                    ? resolve(__dirname, 'pages/settings/components/DeveloperSettings.vue')
                    : resolve(__dirname, 'pages/settings/components/DeveloperSettings.stub.vue'),
            },
        },
    }
})

import {defineConfig} from 'vitest/config'

export default defineConfig({
    test: {
        environment: 'node',
        // clipboard-logic.test.js 是 node:test 编写，由 node --test 运行，不纳入 vitest
        include: ['tests/{errorHandler,fileUrl,localeManager}.test.js'],
        globals: false,
    },
})

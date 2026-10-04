import {describe, it, expect, beforeEach, afterEach, vi} from 'vitest'

// services/ipc.js 顶层使用 Vite define 注入的 __DEV_PANEL__，vitest 环境需提前声明
vi.hoisted(() => {
    globalThis.__DEV_PANEL__ = false
})

import {useClipboardHistory} from '../pages/clipboard/composables/useClipboardHistory.js'
import {ClipboardService} from '../services/ipc.js'
import {
    visibleToRawIndex,
    removeImageHistoryItem,
    applyImagePageToHistory
} from '../pages/image_clipboard/historyListUtils.js'

// ===== F9: 过滤态鼠标导航可见位 → 原始下标换算 =====

describe('F9 visibleToRawIndex 可见位→原始 history 下标', () => {
    it('过滤态下按条目 index 字段换算为原始下标', () => {
        const visible = [{index: 2}, {index: 5}, {index: 9}]
        expect(visibleToRawIndex(visible, 0)).toBe(2)
        expect(visibleToRawIndex(visible, 1)).toBe(5)
        expect(visibleToRawIndex(visible, 2)).toBe(9)
    })

    it('无过滤时可见位等于原始下标', () => {
        const visible = [{index: 0}, {index: 1}, {index: 2}]
        expect(visibleToRawIndex(visible, 1)).toBe(1)
    })

    it('越界或缺失条目时回退为可见位', () => {
        expect(visibleToRawIndex([{index: 3}], 5)).toBe(5)
        expect(visibleToRawIndex(null, 1)).toBe(1)
        expect(visibleToRawIndex([{index: 'x'}], 1)).toBe(1)
    })
})

// ===== F10: 图片过滤态删除按 id 定位 =====

describe('F10 removeImageHistoryItem 按 id 定位删除', () => {
    const makeHistory = (n) => Array.from({length: n}, (_, i) => ({id: `img${i}`}))

    it('过滤态传入错误下标也删除 id 对应的正确条目', () => {
        const history = makeHistory(10)
        const result = removeImageHistoryItem(history, 3, 'img7')
        expect(result.history.map((x) => x.id)).toEqual([
            'img0', 'img1', 'img2', 'img3', 'img4', 'img5', 'img6', 'img8', 'img9'
        ])
        // 选中项 img3（原始下标 3）不受影响
        expect(result.selectedIndex).toBe(3)
        expect(result.history[result.selectedIndex].id).toBe('img3')
    })

    it('删除选中项时选中跟随原位置的下一条', () => {
        const history = makeHistory(5)
        const result = removeImageHistoryItem(history, 2, 'img2')
        expect(result.history).toHaveLength(4)
        expect(result.selectedIndex).toBe(2)
        expect(result.history[result.selectedIndex].id).toBe('img3')
    })

    it('选中末位条目被删除时收敛到新末位', () => {
        const history = makeHistory(5)
        const result = removeImageHistoryItem(history, 4, 'img4')
        expect(result.selectedIndex).toBe(3)
        expect(result.history[result.selectedIndex].id).toBe('img3')
    })

    it('删除唯一条目时 selectedIndex 置为 -1', () => {
        const result = removeImageHistoryItem([{id: 'only'}], 0, 'only')
        expect(result.history).toHaveLength(0)
        expect(result.selectedIndex).toBe(-1)
    })

    it('id 不存在时原样返回', () => {
        const history = makeHistory(3)
        const result = removeImageHistoryItem(history, 1, 'missing')
        expect(result.history).toBe(history)
        expect(result.selectedIndex).toBe(1)
    })
})

// ===== F11: syncHistory(reset) 写入前截断旧数组 =====

describe('F11 applyImagePageToHistory reset 截断', () => {
    const pageItem = (id) => ({id, width: 1, height: 1, previewPngBase64: '', imagePath: `/p/${id}`})

    it('reset 时先重建空数组，已加载的旧条目全部截断', () => {
        const old = Array.from({length: 30}, (_, i) => ({id: `old${i}`}))
        const page = [pageItem('n0'), pageItem('n1'), pageItem('n2')]
        const next = applyImagePageToHistory(old, page, 0, true)
        expect(next.map((x) => x.id)).toEqual(['n0', 'n1', 'n2'])
    })

    it('reset 首屏 + 非 reset 续页可无缝推进（无缺口、无重复）', () => {
        const firstPage = Array.from({length: 10}, (_, i) => pageItem(`p${i}`))
        const page1 = applyImagePageToHistory([], firstPage, 0, true)
        expect(page1).toHaveLength(10)

        const secondPage = Array.from({length: 10}, (_, i) => pageItem(`p${10 + i}`))
        const page2 = applyImagePageToHistory(page1, secondPage, 10, false)
        expect(page2.map((x) => x.id)).toEqual(
            Array.from({length: 20}, (_, i) => `p${i}`)
        )
    })

    it('非 reset 写入按槽位覆盖并按 id 去重压缩', () => {
        const old = [{id: 'a'}, {id: 'b'}, {id: 'c'}]
        // 槽位 4、5 写入；'a' 与已有条目重复只保留首个
        const next = applyImagePageToHistory(old, [pageItem('d'), pageItem('a')], 4, false)
        expect(next.map((x) => x.id)).toEqual(['a', 'b', 'c', 'd'])
    })
})

// ===== F12: 文本搜索态「加载更多」的 offset 口径与命中集并入 =====

describe('F12 搜索态加载更多', () => {
    const originalGetHistoryPage = ClipboardService.getHistoryPage
    let calls

    const makeServer = () => {
        const list = []
        for (let i = 0; i < 100; i++) {
            list.push({
                id: `b${i}`,
                content: `beta ${i}`,
                position: 0,
                category: '未分类',
                pinned: false,
                updatedAt: 1_000_000 - i,
                snippet: ''
            })
        }
        for (let i = 0; i < 120; i++) {
            list.push({
                id: `a${i}`,
                content: `alpha ${i}`,
                position: 0,
                category: '未分类',
                pinned: false,
                updatedAt: 999_900 - i,
                snippet: ''
            })
        }
        return list
    }

    beforeEach(() => {
        calls = []
    })

    afterEach(() => {
        ClipboardService.getHistoryPage = originalGetHistoryPage
    })

    it('首屏命中后加载更多：offset 用已加载命中数，新命中并入可见列表', async () => {
        const server = makeServer()
        ClipboardService.getHistoryPage = async ({offset = 0, limit = 50, keyword = null}) => {
            const filtered = keyword
                ? server.filter((item) => item.content.includes(String(keyword).toLowerCase()))
                : server
            calls.push({offset, keyword})
            return {
                items: filtered.slice(offset, offset + limit),
                total: filtered.length,
                offset,
                limit
            }
        }

        const history = useClipboardHistory()
        // 无关键词先加载两页（100 条非命中数据，用于验证 offset 口径不被它们污染）
        await history.resetAndReloadHistory()
        await history.loadMoreHistory()
        expect(history.visibleHistory.value).toHaveLength(100)

        history.searchKeyword.value = 'alpha'
        await history.syncHistoryIncremental()
        // 首屏命中 50 条；旧代码 hasMore 用分类计数 150 对比被抬高的 totalCount → 误报已全部加载
        expect(history.visibleHistory.value).toHaveLength(50)
        expect(history.hasMore.value).toBe(true)

        calls.length = 0
        await history.loadMoreHistory()
        // 旧代码 offset 用 getActiveCategoryCount()=150，跳过命中 50..99
        expect(calls[0].offset).toBe(50)
        // 旧代码不更新 searchMatchedIds，新命中被 visibleHistory 过滤掉
        expect(history.visibleHistory.value).toHaveLength(100)

        await history.loadMoreHistory()
        expect(calls[1].offset).toBe(100)
        const ids = history.visibleHistory.value.map((entry) => entry.id)
        expect(ids).toHaveLength(120)
        expect(new Set(ids)).toEqual(new Set(server.slice(100).map((item) => item.id)))
        expect(ids).toContain('a50')
        expect(ids).toContain('a119')
        expect(history.hasMore.value).toBe(false)
    })
})

// ===== F13: 先跳到末尾再加载，中段页按后端时间序归位 =====

describe('F13 跳到末尾后再加载的顺序归位', () => {
    const originalGetHistoryPage = ClipboardService.getHistoryPage
    const makeItem = (id, updatedAt) => ({
        id,
        content: `content ${id}`,
        position: 0,
        category: '未分类',
        pinned: false,
        updatedAt,
        snippet: ''
    })

    afterEach(() => {
        ClipboardService.getHistoryPage = originalGetHistoryPage
    })

    const runScenario = async (tsOf) => {
        const server = Array.from({length: 150}, (_, i) => makeItem(`n${i}`, tsOf(i)))
        ClipboardService.getHistoryPage = async ({offset = 0, limit = 50}) => ({
            items: server.slice(offset, offset + limit),
            total: server.length,
            offset,
            limit
        })

        const history = useClipboardHistory()
        await history.resetAndReloadHistory()               // 新50: n0..n49
        await history.loadTailPage()                // 旧50: n100..n149

        // 模拟头插 50 条新记录（新复制/导入），旧条目整体后移一位段
        const shifted = Array.from({length: 50}, (_, i) => makeItem(`x${i}`, 2_000_000 - i))
        server.unshift(...shifted)

        await history.loadMoreHistory() // 中50: 请求 offset=100 → n50..n99
        return history.visibleHistory.value.map((entry) => entry.id)
    }

    it('时间降序（新→旧）：中段页插入旧50之前，而非追加到尾部', async () => {
        const ids = await runScenario((i) => 1_000_000 - i)
        expect(ids.slice(0, 50)).toEqual(Array.from({length: 50}, (_, i) => `n${i}`))
        expect(ids.slice(50, 100)).toEqual(Array.from({length: 50}, (_, i) => `n${50 + i}`))
        expect(ids.slice(100, 150)).toEqual(Array.from({length: 50}, (_, i) => `n${100 + i}`))
    })

    it('时间升序（旧→新）：中段页同样插入旧50之前', async () => {
        const ids = await runScenario((i) => 1_000_000 + i)
        expect(ids.slice(0, 50)).toEqual(Array.from({length: 50}, (_, i) => `n${i}`))
        expect(ids.slice(50, 100)).toEqual(Array.from({length: 50}, (_, i) => `n${50 + i}`))
        expect(ids.slice(100, 150)).toEqual(Array.from({length: 50}, (_, i) => `n${100 + i}`))
    })
})

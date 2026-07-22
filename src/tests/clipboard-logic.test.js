import {describe, it} from 'node:test'
import assert from 'node:assert/strict'

// ===== 模拟 Vue ref（用于测试 composable 纯逻辑） =====
function ref(value) {
    return {
        get value() {
            return value
        }, set value(v) {
            value = v
        }
    }
}

// ===== 从 useClipboardHistory 提取的纯逻辑测试 =====

describe('ClipboardHistory - sortPageItems', () => {
    // 复制排序逻辑用于独立测试
    function sortPageItems(entries, sortBy, sortOrder) {
        const merged = entries.slice()
        merged.sort((a, b) => {
            if (sortBy === 'pinnedFirst') {
                const pinDiff = (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0)
                if (pinDiff !== 0) return pinDiff
                const diff = a.position - b.position
                if (a.pinned && b.pinned) return diff
                return diff
            }
            if (sortBy === 'updatedAt') {
                const diff = (b.updatedAt || 0) - (a.updatedAt || 0)
                if (diff !== 0) return sortOrder === 'asc' ? -diff : diff
                return a.position - b.position
            }
            return a.position - b.position
        })
        return merged
    }

    it('pinnedFirst: pinned items before unpinned', () => {
        const items = [
            {id: '1', pinned: false, position: 0, updatedAt: 100},
            {id: '2', pinned: true, position: 1, updatedAt: 200},
            {id: '3', pinned: false, position: 2, updatedAt: 300},
        ]
        const sorted = sortPageItems(items, 'pinnedFirst', 'asc')
        assert.equal(sorted[0].id, '2')
        assert.equal(sorted[1].id, '1')
        assert.equal(sorted[2].id, '3')
    })

    it('pinnedFirst: pinned items sorted by position', () => {
        const items = [
            {id: '1', pinned: true, position: 2},
            {id: '2', pinned: true, position: 0},
            {id: '3', pinned: true, position: 1},
        ]
        const sorted = sortPageItems(items, 'pinnedFirst', 'asc')
        assert.equal(sorted.map(i => i.id).join(','), '2,3,1')
    })

    it('updatedAt desc: newer items first', () => {
        const items = [
            {id: '1', pinned: false, position: 0, updatedAt: 100},
            {id: '2', pinned: false, position: 1, updatedAt: 300},
            {id: '3', pinned: false, position: 2, updatedAt: 200},
        ]
        const sorted = sortPageItems(items, 'updatedAt', 'desc')
        assert.equal(sorted[0].id, '2')
        assert.equal(sorted[1].id, '3')
        assert.equal(sorted[2].id, '1')
    })

    it('updatedAt asc: older items first', () => {
        const items = [
            {id: '1', pinned: false, position: 0, updatedAt: 100},
            {id: '2', pinned: false, position: 1, updatedAt: 300},
        ]
        const sorted = sortPageItems(items, 'updatedAt', 'asc')
        assert.equal(sorted[0].id, '1')
        assert.equal(sorted[1].id, '2')
    })

    it('default: sort by position', () => {
        const items = [
            {id: '1', pinned: false, position: 2},
            {id: '2', pinned: false, position: 0},
            {id: '3', pinned: false, position: 1},
        ]
        const sorted = sortPageItems(items, 'other', 'asc')
        assert.equal(sorted.map(i => i.id).join(','), '2,3,1')
    })
})

describe('ClipboardHistory - getActiveCategoryCount', () => {
    function getActiveCategoryCount(pagedHistory, categoryFilter, getItemCategory) {
        const activeCategory = categoryFilter === '全部' ? null : categoryFilter
        if (!activeCategory) return pagedHistory.length
        let count = 0
        for (const item of pagedHistory) {
            if (item.category === activeCategory || getItemCategory(item.id) === activeCategory) {
                count++
            }
        }
        return count
    }

    it('no filter: returns total count', () => {
        const items = [{id: '1'}, {id: '2'}, {id: '3'}]
        assert.equal(getActiveCategoryCount(items, '全部', () => '未分类'), 3)
    })

    it('with filter: counts matching items', () => {
        const items = [
            {id: '1', category: '工作'},
            {id: '2', category: '生活'},
            {id: '3', category: '工作'},
        ]
        assert.equal(getActiveCategoryCount(items, '工作', () => ''), 2)
        assert.equal(getActiveCategoryCount(items, '生活', () => ''), 1)
    })

    it('with filter: uses getItemCategory fallback', () => {
        const items = [{id: '1'}, {id: '2'}]
        const catMap = {'1': '工作', '2': '生活'}
        assert.equal(
            getActiveCategoryCount(items, '工作', (id) => catMap[id] || '未分类'),
            1
        )
    })
})

describe('ClipboardHistory - keywordHitCount', () => {
    function keywordHitCount(visibleHistory, searchKeyword) {
        const tokens = searchKeyword.trim().toLowerCase().split(/\s+/).map(t => t.trim()).filter(Boolean)
        if (tokens.length === 0) return 0
        return visibleHistory.filter(entry => {
            const text = `${entry.content || ''}\n${entry.snippet || ''}`.toLowerCase()
            return tokens.some(token => text.includes(token))
        }).length
    }

    it('empty keyword: returns 0', () => {
        assert.equal(keywordHitCount([{content: 'hello'}], ''), 0)
    })

    it('single keyword: counts matches', () => {
        const items = [
            {content: 'hello world'},
            {content: 'foo bar'},
            {content: 'hello rust'},
        ]
        assert.equal(keywordHitCount(items, 'hello'), 2)
    })

    it('multi keywords: OR matching (any token matches)', () => {
        const items = [
            {content: 'hello world'},
            {content: 'hello rust'},
            {content: 'foo bar'},
        ]
        // tokens ["hello", "rust"], items matching "hello" OR "rust" = 2
        assert.equal(keywordHitCount(items, 'hello rust'), 2)
    })

    it('case insensitive', () => {
        const items = [{content: 'Hello World'}]
        assert.equal(keywordHitCount(items, 'hello'), 1)
    })
})

describe('ClipboardHistory - mergePageItems category protection', () => {
    // 模拟修复后的 mergePageItems 逻辑
    function mergeCategories(existingCategoryMap, items) {
        for (const item of items) {
            if (item.id) {
                if (item.category) {
                    existingCategoryMap[item.id] = item.category
                } else if (!existingCategoryMap[item.id]) {
                    existingCategoryMap[item.id] = '未分类'
                }
            }
        }
    }

    it('does not overwrite existing category when item.category is absent', () => {
        const catMap = {'1': '工作'}
        mergeCategories(catMap, [{id: '1', content: 'test'}])
        assert.equal(catMap['1'], '工作', '已有分类不应被覆盖')
    })

    it('updates category when item.category is present', () => {
        const catMap = {'1': '工作'}
        mergeCategories(catMap, [{id: '1', category: '生活'}])
        assert.equal(catMap['1'], '生活')
    })

    it('sets 未分类 for new items without category', () => {
        const catMap = {}
        mergeCategories(catMap, [{id: '1'}])
        assert.equal(catMap['1'], '未分类')
    })
})

describe('CategorySearchIndex - keyword matching', () => {
    function getKeywordCategoryMatchedIds(keyword, categorySearchIndex) {
        if (!keyword) return null
        const matchedIds = new Set()
        for (const [category, idSet] of categorySearchIndex.entries()) {
            if (!String(category).toLowerCase().includes(keyword)) continue
            for (const id of idSet) {
                matchedIds.add(id)
            }
        }
        return matchedIds
    }

    it('matches category name containing keyword', () => {
        const index = new Map([
            ['工作', new Set(['1', '2'])],
            ['生活', new Set(['3'])],
            ['学习', new Set(['4', '5'])],
        ])
        const matched = getKeywordCategoryMatchedIds('工', index)
        assert.deepEqual([...matched].sort(), ['1', '2'])
    })

    it('case insensitive match', () => {
        const index = new Map([
            ['Work', new Set(['1'])],
        ])
        const matched = getKeywordCategoryMatchedIds('work', index)
        assert.deepEqual([...matched], ['1'])
    })

    it('no match returns empty set', () => {
        const index = new Map([['工作', new Set(['1'])]])
        const matched = getKeywordCategoryMatchedIds('xyz', index)
        assert.equal(matched.size, 0)
    })

    it('empty keyword returns null', () => {
        assert.equal(getKeywordCategoryMatchedIds('', new Map()), null)
    })
})

describe('CategoryActions - runCategoryAssignment', () => {
    async function runCategoryAssignment({itemKey, category, persist, onFinally}) {
        if (!itemKey || category === '全部') {
            if (typeof onFinally === 'function') onFinally()
            return false
        }
        try {
            if (typeof persist === 'function') {
                await Promise.resolve(persist(itemKey, category))
            }
            return true
        } catch (error) {
            return false
        } finally {
            if (typeof onFinally === 'function') onFinally()
        }
    }

    it('returns false for empty itemKey', async () => {
        const result = await runCategoryAssignment({itemKey: '', category: '工作'})
        assert.equal(result, false)
    })

    it('returns false for 全部 category', async () => {
        const result = await runCategoryAssignment({itemKey: '1', category: '全部'})
        assert.equal(result, false)
    })

    it('calls persist and returns true', async () => {
        let called = false
        const result = await runCategoryAssignment({
            itemKey: '1',
            category: '工作',
            persist: () => {
                called = true
            },
        })
        assert.equal(result, true)
        assert.equal(called, true)
    })

    it('returns false on persist error', async () => {
        const result = await runCategoryAssignment({
            itemKey: '1',
            category: '工作',
            persist: () => {
                throw new Error('fail')
            },
        })
        assert.equal(result, false)
    })

    it('calls onFinally always', async () => {
        let finallyCalled = 0
        await runCategoryAssignment({
            itemKey: '1',
            category: '工作',
            onFinally: () => {
                finallyCalled++
            },
        })
        assert.equal(finallyCalled, 1)
    })
})

describe('App.vue - selectedStatusText', () => {
    function selectedStatusText(totalCount, visibleHistory, selectedItemId) {
        const total = totalCount || visibleHistory.length
        if (total === 0) return '无选中'
        const current = visibleHistory.findIndex(e => e.id === selectedItemId)
        const display = current >= 0 ? current + 1 : 1
        return `第 ${display} / 共 ${total} 条`
    }

    it('empty list', () => {
        assert.equal(selectedStatusText(0, [], ''), '无选中')
    })

    it('first item selected', () => {
        const items = [{id: '1'}, {id: '2'}]
        assert.equal(selectedStatusText(2, items, '1'), '第 1 / 共 2 条')
    })

    it('second item selected', () => {
        const items = [{id: '1'}, {id: '2'}]
        assert.equal(selectedStatusText(2, items, '2'), '第 2 / 共 2 条')
    })

    it('totalCount overrides visibleHistory length', () => {
        const items = [{id: '1'}]
        assert.equal(selectedStatusText(100, items, '1'), '第 1 / 共 100 条')
    })
})

describe('ClipboardList - isWebUrl', () => {
    function isWebUrl(value) {
        if (!value) return false
        const text = value.trim()
        return /^https?:\/\/\S+$/i.test(text) || /^www\.\S+$/i.test(text)
    }

    it('valid https url', () => {
        assert.equal(isWebUrl('https://example.com'), true)
    })

    it('valid http url', () => {
        assert.equal(isWebUrl('http://example.com/path?q=1'), true)
    })

    it('www prefix', () => {
        assert.equal(isWebUrl('www.example.com'), true)
    })

    it('not a url', () => {
        assert.equal(isWebUrl('hello world'), false)
    })

    it('empty', () => {
        assert.equal(isWebUrl(''), false)
        assert.equal(isWebUrl(null), false)
    })

    it('url with spaces', () => {
        assert.equal(isWebUrl('https://example.com with spaces'), false)
    })
})

// ===== 更多边界用例 =====

describe('ClipboardHistory - applyGroupedEntries position calculation', () => {
    function applyGroupedEntries(pinnedEntries, unpinnedEntries, sortOrder) {
        const pinned = pinnedEntries.map((entry, idx) => ({
            ...entry,
            pinned: true,
            position: idx
        }))
        const unpinnedBase = pinned.length
        const unpinnedCount = unpinnedEntries.length
        const unpinned = unpinnedEntries.map((entry, idx) => ({
            ...entry,
            pinned: false,
            position: sortOrder === 'desc'
                ? unpinnedBase + (unpinnedCount - idx - 1)
                : unpinnedBase + idx
        }))
        return [...pinned, ...unpinned]
    }

    it('pinned items get positions 0..N-1', () => {
        const result = applyGroupedEntries(
            [{id: '1'}, {id: '2'}],
            [{id: '3'}],
            'asc'
        )
        assert.equal(result[0].position, 0)
        assert.equal(result[1].position, 1)
        assert.equal(result[2].position, 2)
    })

    it('unpinned asc: positions continue after pinned', () => {
        const result = applyGroupedEntries(
            [{id: '1'}],
            [{id: '2'}, {id: '3'}],
            'asc'
        )
        assert.equal(result[1].position, 1)
        assert.equal(result[2].position, 2)
    })

    it('unpinned desc: positions reversed', () => {
        const result = applyGroupedEntries(
            [{id: '1'}],
            [{id: '2'}, {id: '3'}],
            'desc'
        )
        // unpinnedBase=1, unpinnedCount=2
        // idx=0: 1 + (2-0-1) = 2
        // idx=1: 1 + (2-1-1) = 1
        assert.equal(result[1].position, 2)
        assert.equal(result[2].position, 1)
    })

    it('empty pinned and unpinned', () => {
        const result = applyGroupedEntries([], [], 'asc')
        assert.equal(result.length, 0)
    })
})

describe('ClipboardHistory - loadHistoryPage offset calculation', () => {
    function calcOffset(reset, getActiveCategoryCount) {
        return reset ? 0 : getActiveCategoryCount()
    }

    it('reset: offset is 0', () => {
        assert.equal(calcOffset(true, () => 50), 0)
    })

    it('not reset: offset is current count', () => {
        assert.equal(calcOffset(false, () => 50), 50)
    })
})

describe('ClipboardHistory - hasMore calculation', () => {
    function calcHasMore(activeCount, totalCount) {
        return activeCount < totalCount
    }

    it('has more when count < total', () => {
        assert.equal(calcHasMore(50, 100), true)
    })

    it('no more when count >= total', () => {
        assert.equal(calcHasMore(100, 100), false)
        assert.equal(calcHasMore(150, 100), false)
    })

    it('no more when total is 0', () => {
        assert.equal(calcHasMore(0, 0), false)
    })
})

describe('ClipboardHistory - normalizePageSize', () => {
    const PAGE_SIZE_OPTIONS = [10, 30, 50]
    const normalizePageSize = (value) => {
        const parsed = Number(value)
        return PAGE_SIZE_OPTIONS.includes(parsed) ? parsed : 50
    }

    it('valid sizes', () => {
        assert.equal(normalizePageSize(10), 10)
        assert.equal(normalizePageSize(30), 30)
        assert.equal(normalizePageSize(50), 50)
    })

    it('invalid size defaults to 50', () => {
        assert.equal(normalizePageSize(20), 50)
        assert.equal(normalizePageSize(100), 50)
        assert.equal(normalizePageSize(NaN), 50)
        assert.equal(normalizePageSize('abc'), 50)
    })

    it('string numbers', () => {
        assert.equal(normalizePageSize('10'), 10)
        assert.equal(normalizePageSize('30'), 30)
    })
})

describe('ClipboardList - normalizeUrl', () => {
    function normalizeUrl(value) {
        const text = value.trim()
        if (/^https?:\/\//i.test(text)) return text
        if (/^www\./i.test(text)) return `https://${text}`
        return text
    }

    it('https passthrough', () => {
        assert.equal(normalizeUrl('https://example.com'), 'https://example.com')
    })

    it('http passthrough', () => {
        assert.equal(normalizeUrl('http://example.com'), 'http://example.com')
    })

    it('www gets https prefix', () => {
        assert.equal(normalizeUrl('www.example.com'), 'https://www.example.com')
    })

    it('no prefix passthrough', () => {
        assert.equal(normalizeUrl('example.com'), 'example.com')
    })

    it('trims whitespace', () => {
        assert.equal(normalizeUrl('  https://example.com  '), 'https://example.com')
    })
})

describe('CategoryManager - canDeleteCategory', () => {
    function canDeleteCategory(category) {
        return category !== '未分类'
    }

    it('can delete custom category', () => {
        assert.equal(canDeleteCategory('工作'), true)
        assert.equal(canDeleteCategory('生活'), true)
    })

    it('cannot delete 未分类', () => {
        assert.equal(canDeleteCategory('未分类'), false)
    })
})

describe('CategoryManager - category validation', () => {
    function validateCategory(category) {
        if (!category || category === '未分类' || category === '全部') return false
        return true
    }

    it('valid category', () => {
        assert.equal(validateCategory('工作'), true)
    })

    it('empty', () => {
        assert.equal(validateCategory(''), false)
        assert.equal(validateCategory(null), false)
        assert.equal(validateCategory(undefined), false)
    })

    it('reserved names', () => {
        assert.equal(validateCategory('未分类'), false)
        assert.equal(validateCategory('全部'), false)
    })
})

describe('SearchHighlight - renderHighlightParts', () => {
    function renderHighlightParts(text, keyword) {
        const value = typeof text === 'string' ? text : ''
        const tokens = Array.from(new Set(keyword.split(/\s+/).map(v => v.trim()).filter(Boolean)))
            .sort((a, b) => b.length - a.length)
        if (!value || tokens.length === 0) {
            return [{text: value, hit: false}]
        }
        const sourceLower = value.toLowerCase()
        const tokenLowers = tokens.map(t => t.toLowerCase())
        const out = []
        let start = 0
        while (start < value.length) {
            let bestIndex = -1
            let bestToken = ''
            for (let i = 0; i < tokenLowers.length; i++) {
                const token = tokenLowers[i]
                const idx = sourceLower.indexOf(token, start)
                if (idx === -1) continue
                if (bestIndex === -1 || idx < bestIndex || (idx === bestIndex && token.length > bestToken.length)) {
                    bestIndex = idx
                    bestToken = token
                }
            }
            if (bestIndex === -1) {
                out.push({text: value.slice(start), hit: false})
                break
            }
            if (bestIndex > start) {
                out.push({text: value.slice(start, bestIndex), hit: false})
            }
            const hitEnd = bestIndex + bestToken.length
            out.push({text: value.slice(bestIndex, hitEnd), hit: true})
            start = hitEnd
        }
        return out.length > 0 ? out : [{text: value, hit: false}]
    }

    it('no keyword: all non-hit', () => {
        const parts = renderHighlightParts('hello world', '')
        assert.equal(parts.length, 1)
        assert.equal(parts[0].hit, false)
    })

    it('keyword found: split into parts', () => {
        const parts = renderHighlightParts('hello world', 'world')
        assert.equal(parts.length, 2)
        assert.equal(parts[0].hit, false)
        assert.equal(parts[1].hit, true)
        assert.equal(parts[1].text, 'world')
    })

    it('keyword at start', () => {
        const parts = renderHighlightParts('hello world', 'hello')
        assert.equal(parts[0].hit, true)
        assert.equal(parts[0].text, 'hello')
    })

    it('keyword at end', () => {
        const parts = renderHighlightParts('hello world', 'world')
        assert.equal(parts[parts.length - 1].hit, true)
    })

    it('multiple keywords: longest first', () => {
        const parts = renderHighlightParts('hello world', 'world hello')
        // Both should be highlighted
        const hits = parts.filter(p => p.hit)
        assert.equal(hits.length, 2)
    })

    it('case insensitive', () => {
        const parts = renderHighlightParts('Hello World', 'hello')
        assert.equal(parts[0].hit, true)
        assert.equal(parts[0].text, 'Hello')
    })

    it('empty text', () => {
        const parts = renderHighlightParts('', 'hello')
        assert.equal(parts.length, 1)
        assert.equal(parts[0].text, '')
    })
})

describe('selectedItemId auto-selection', () => {
    function autoSelect(visibleHistory, selectedItemId) {
        if (!Array.isArray(visibleHistory) || visibleHistory.length === 0) return ''
        const exists = visibleHistory.some(entry => entry.id === selectedItemId)
        if (!exists) return visibleHistory[0].id
        return selectedItemId
    }

    it('empty list returns empty', () => {
        assert.equal(autoSelect([], '1'), '')
    })

    it('selected exists: keeps it', () => {
        const items = [{id: '1'}, {id: '2'}]
        assert.equal(autoSelect(items, '2'), '2')
    })

    it('selected missing: picks first', () => {
        const items = [{id: '1'}, {id: '2'}]
        assert.equal(autoSelect(items, '99'), '1')
    })

    it('null selected: picks first', () => {
        const items = [{id: '1'}]
        assert.equal(autoSelect(items, null), '1')
    })
})

// ===== 前端集成测试：模拟完整数据流 =====

describe('Integration - 完整剪贴板数据流', () => {
    // 模拟完整的 ClipboardHistory composable 数据流
    function createMockClipboardState() {
        const pagedHistory = []
        const categoryMap = {}
        const categorySearchIndex = new Map()
        const pinnedItems = []
        let filterRevision = 0

        const setItemCategoryLocal = (id, category) => {
            categoryMap[id] = category
            // 更新搜索索引
            let idSet = categorySearchIndex.get(category)
            if (!idSet) {
                idSet = new Set()
                categorySearchIndex.set(category, idSet)
            }
            idSet.add(id)
        }

        const removeItemCategoryLocal = (id) => {
            const oldCat = categoryMap[id]
            if (oldCat) {
                const idSet = categorySearchIndex.get(oldCat)
                if (idSet) {
                    idSet.delete(id)
                    if (idSet.size === 0) categorySearchIndex.delete(oldCat)
                }
                delete categoryMap[id]
            }
        }

        const insertItem = (id, content, category, pinned = false) => {
            pagedHistory.unshift({
                id,
                content,
                position: 0,
                snippet: '',
                pinned,
                category: category || '未分类'
            })
            // 重排位置
            pagedHistory.forEach((e, i) => e.position = i)
            if (category) setItemCategoryLocal(id, category)
            if (pinned) pinnedItems.unshift(id)
            filterRevision++
        }

        const removeItem = (id) => {
            const idx = pagedHistory.findIndex(e => e.id === id)
            if (idx >= 0) pagedHistory.splice(idx, 1)
            pagedHistory.forEach((e, i) => e.position = i)
            removeItemCategoryLocal(id)
            const pIdx = pinnedItems.indexOf(id)
            if (pIdx >= 0) pinnedItems.splice(pIdx, 1)
            filterRevision++
        }

        const pinItem = (id) => {
            if (!pinnedItems.includes(id)) pinnedItems.unshift(id)
            const item = pagedHistory.find(e => e.id === id)
            if (item) item.pinned = true
            filterRevision++
        }

        const unpinItem = (id) => {
            const pIdx = pinnedItems.indexOf(id)
            if (pIdx >= 0) pinnedItems.splice(pIdx, 1)
            const item = pagedHistory.find(e => e.id === id)
            if (item) item.pinned = false
            filterRevision++
        }

        const getVisibleHistory = (categoryFilter, keyword) => {
            return pagedHistory.filter(item => {
                if (categoryFilter && categoryFilter !== '全部') {
                    const cat = categoryMap[item.id] || '未分类'
                    if (cat !== categoryFilter) return false
                }
                if (keyword) {
                    if (!item.content.toLowerCase().includes(keyword.toLowerCase())) return false
                }
                return true
            })
        }

        return {
            pagedHistory, categoryMap, categorySearchIndex, pinnedItems,
            insertItem, removeItem, pinItem, unpinItem, getVisibleHistory,
            setItemCategoryLocal, removeItemCategoryLocal,
            get filterRevision() {
                return filterRevision
            }
        }
    }

    it('full flow: insert → categorize → pin → search → remove', () => {
        const state = createMockClipboardState()

        // 1. 插入 5 条记录
        state.insertItem('1', 'Hello World', '工作')
        state.insertItem('2', 'Good Morning', '生活')
        state.insertItem('3', 'Hello Rust', '工作')
        state.insertItem('4', 'Foo Bar', null)
        state.insertItem('5', 'Good Night', '生活')

        assert.equal(state.pagedHistory.length, 5)

        // 2. 验证分类
        const workItems = state.getVisibleHistory('工作', null)
        assert.equal(workItems.length, 2)

        const lifeItems = state.getVisibleHistory('生活', null)
        assert.equal(lifeItems.length, 2)

        const uncatItems = state.getVisibleHistory('未分类', null)
        assert.equal(uncatItems.length, 1)

        // 3. 搜索
        const helloItems = state.getVisibleHistory(null, 'hello')
        assert.equal(helloItems.length, 2)

        const goodItems = state.getVisibleHistory(null, 'good')
        assert.equal(goodItems.length, 2)

        // 4. 置顶
        state.pinItem('3')
        assert.equal(state.pinnedItems.includes('3'), true)
        assert.equal(state.pagedHistory.find(e => e.id === '3').pinned, true)

        // 5. 取消置顶
        state.unpinItem('3')
        assert.equal(state.pinnedItems.includes('3'), false)
        assert.equal(state.pagedHistory.find(e => e.id === '3').pinned, false)

        // 6. 分类筛选 + 搜索组合
        const filtered = state.getVisibleHistory('工作', 'rust')
        assert.equal(filtered.length, 1)
        assert.equal(filtered[0].id, '3')

        // 7. 删除
        state.removeItem('1')
        assert.equal(state.pagedHistory.length, 4)
        assert.equal(state.categoryMap['1'], undefined)

        // 8. 验证 filterRevision 递增
        assert.ok(state.filterRevision > 0)
    })

    it('integration: pagination offset calculation', () => {
        const state = createMockClipboardState()

        // 插入 100 条
        for (let i = 0; i < 100; i++) {
            state.insertItem(`item${i}`, `content ${i}`, i % 3 === 0 ? '工作' : null)
        }

        const pageSize = 50

        // 第一页
        const page1 = state.getVisibleHistory(null, null).slice(0, pageSize)
        assert.equal(page1.length, 50)

        // 第二页
        const page2 = state.getVisibleHistory(null, null).slice(pageSize, pageSize * 2)
        assert.equal(page2.length, 50)

        // hasMore 计算
        const totalCount = 100
        assert.ok(page1.length < totalCount, '第一页应该有更多')

        // 分类筛选后的分页
        const workItems = state.getVisibleHistory('工作', null)
        const workPage1 = workItems.slice(0, 10)
        assert.equal(workPage1.length, 10)
    })

    it('integration: category filter preserves across operations', () => {
        const state = createMockClipboardState()

        state.insertItem('1', 'work item 1', '工作')
        state.insertItem('2', 'life item 1', '生活')
        state.insertItem('3', 'work item 2', '工作')

        // 设置分类后，分类筛选应该正确
        let filtered = state.getVisibleHistory('工作', null)
        assert.equal(filtered.length, 2)

        // 删除一个工作项
        state.removeItem('1')
        filtered = state.getVisibleHistory('工作', null)
        assert.equal(filtered.length, 1)
        assert.equal(filtered[0].id, '3')

        // 添加新工作项
        state.insertItem('4', 'work item 3', '工作')
        filtered = state.getVisibleHistory('工作', null)
        assert.equal(filtered.length, 2)
    })

    it('integration: pin/unpin affects display order concept', () => {
        const state = createMockClipboardState()

        state.insertItem('1', 'item 1', null)
        state.insertItem('2', 'item 2', null)
        state.insertItem('3', 'item 3', null)

        // 置顶 item 3
        state.pinItem('3')

        // 验证置顶状态
        const pinnedItems = state.pagedHistory.filter(e => e.pinned)
        assert.equal(pinnedItems.length, 1)
        assert.equal(pinnedItems[0].id, '3')

        // 取消置顶
        state.unpinItem('3')
        const pinnedAfter = state.pagedHistory.filter(e => e.pinned)
        assert.equal(pinnedAfter.length, 0)
    })
})

describe('Integration - 分类搜索索引一致性', () => {
    it('索引与 categoryMap 始终同步', () => {
        const categoryMap = {}
        const searchIndex = new Map()

        const setItemCategory = (id, category) => {
            // 删除旧索引
            const oldCat = categoryMap[id]
            if (oldCat) {
                const oldSet = searchIndex.get(oldCat)
                if (oldSet) {
                    oldSet.delete(id)
                    if (oldSet.size === 0) searchIndex.delete(oldCat)
                }
            }
            // 设置新值
            categoryMap[id] = category
            let idSet = searchIndex.get(category)
            if (!idSet) {
                idSet = new Set()
                searchIndex.set(category, idSet)
            }
            idSet.add(id)
        }

        const removeItemCategory = (id) => {
            const oldCat = categoryMap[id]
            if (oldCat) {
                const oldSet = searchIndex.get(oldCat)
                if (oldSet) {
                    oldSet.delete(id)
                    if (oldSet.size === 0) searchIndex.delete(oldCat)
                }
                delete categoryMap[id]
            }
        }

        // 设置分类
        setItemCategory('1', '工作')
        setItemCategory('2', '工作')
        setItemCategory('3', '生活')

        // 验证索引
        assert.deepEqual([...searchIndex.get('工作')], ['1', '2'])
        assert.deepEqual([...searchIndex.get('生活')], ['3'])

        // 更改分类
        setItemCategory('2', '学习')

        // 验证索引更新
        assert.deepEqual([...searchIndex.get('工作')], ['1'])
        assert.deepEqual([...searchIndex.get('学习')], ['2'])
        assert.deepEqual([...searchIndex.get('生活')], ['3'])

        // 删除分类
        removeItemCategory('1')

        // 验证索引清理
        const workSet = searchIndex.get('工作')
        assert.equal(workSet === undefined || workSet.size === 0, true, '工作分类索引应为空')
        assert.equal(categoryMap['1'], undefined)
    })
})

describe('Integration - 增量同步数据合并', () => {
    // 模拟 syncHistoryIncremental 的核心合并逻辑
    function mergeIncoming(pagedHistory, incomingItems, existingById) {
        const incomingIds = new Set(incomingItems.map(item => item.id))

        const front = []
        for (const item of incomingItems) {
            if (!item.id) continue
            const existing = existingById.get(item.id) || {}
            front.push({
                ...existing,
                id: item.id,
                content: item.content,
                position: item.position ?? existing.position ?? 0,
                pinned: item.pinned ?? existing.pinned ?? false,
                category: item.category || existing.category || '未分类'
            })
        }

        const rest = []
        for (const entry of pagedHistory) {
            if (!incomingIds.has(entry.id)) {
                rest.push({...entry})
            }
        }

        return [...front, ...rest]
    }

    it('merges new items before existing', () => {
        const existing = [
            {id: '1', content: 'old 1', position: 0},
            {id: '2', content: 'old 2', position: 1},
        ]
        const incoming = [
            {id: '3', content: 'new 3', position: 0},
        ]
        const existingById = new Map(existing.map(e => [e.id, e]))

        const result = mergeIncoming(existing, incoming, existingById)
        assert.equal(result.length, 3)
        assert.equal(result[0].id, '3', '新项应该在前面')
        assert.equal(result[1].id, '1')
        assert.equal(result[2].id, '2')
    })

    it('updates existing items from incoming', () => {
        const existing = [
            {id: '1', content: 'old', position: 0, pinned: false},
        ]
        const incoming = [
            {id: '1', content: 'updated', position: 0, pinned: true},
        ]
        const existingById = new Map(existing.map(e => [e.id, e]))

        const result = mergeIncoming(existing, incoming, existingById)
        assert.equal(result.length, 1)
        assert.equal(result[0].content, 'updated', '内容应该被更新')
        assert.equal(result[0].pinned, true, '置顶状态应该被更新')
    })

    it('preserves existing category when incoming has none', () => {
        const existing = [
            {id: '1', content: 'item', position: 0, category: '工作'},
        ]
        const incoming = [
            {id: '1', content: 'item', position: 0},
        ]
        const existingById = new Map(existing.map(e => [e.id, e]))

        const result = mergeIncoming(existing, incoming, existingById)
        assert.equal(result[0].category, '工作', '已有分类应保留')
    })

    it('handles empty incoming', () => {
        const existing = [{id: '1', content: 'item', position: 0}]
        const result = mergeIncoming(existing, [], new Map())
        assert.equal(result.length, 1)
    })
})

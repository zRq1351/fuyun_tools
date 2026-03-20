/**
 * 性能监控工具
 * 收集和分析详细的性能指标，提供性能优化建议
 */

class PerformanceMonitor {
    constructor() {
        this.metrics = new Map()
        this.timers = new Map()
        this.memorySnapshots = []
        this.maxSnapshots = 100
        this.thresholds = {
            loadTime: 1000, // 1秒
            renderTime: 16, // 16ms (60fps)
            memoryUsage: 100 * 1024 * 1024, // 100MB
            apiResponseTime: 5000 // 5秒
        }
    }

    /**
     * 开始性能测量
     */
    startMeasure(name, category = 'general') {
        const timerKey = `${category}_${name}`
        this.timers.set(timerKey, {
            startTime: performance.now(),
            category,
            name
        })
        return timerKey
    }

    /**
     * 结束性能测量
     */
    endMeasure(timerKey) {
        const timer = this.timers.get(timerKey)
        if (!timer) {
            console.warn(`性能计时器未找到: ${timerKey}`)
            return null
        }

        const endTime = performance.now()
        const duration = endTime - timer.startTime

        const metric = {
            name: timer.name,
            category: timer.category,
            duration,
            timestamp: Date.now(),
            startTime: timer.startTime,
            endTime
        }

        // 存储指标
        const metricKey = `${timer.category}_${timer.name}`
        if (!this.metrics.has(metricKey)) {
            this.metrics.set(metricKey, [])
        }
        this.metrics.get(metricKey).push(metric)

        // 限制存储数量
        const metrics = this.metrics.get(metricKey)
        if (metrics.length > 100) {
            metrics.shift()
        }

        this.timers.delete(timerKey)

        // 检查性能阈值
        this.checkThresholds(metric)

        return metric
    }

    /**
     * 测量异步函数性能
     */
    async measureAsync(name, asyncFn, category = 'async') {
        const timerKey = this.startMeasure(name, category)
        try {
            const result = await asyncFn()
            this.endMeasure(timerKey)
            return result
        } catch (error) {
            this.endMeasure(timerKey)
            throw error
        }
    }

    /**
     * 测量同步函数性能
     */
    measureSync(name, syncFn, category = 'sync') {
        const timerKey = this.startMeasure(name, category)
        try {
            const result = syncFn()
            this.endMeasure(timerKey)
            return result
        } catch (error) {
            this.endMeasure(timerKey)
            throw error
        }
    }

    /**
     * 记录内存快照
     */
    takeMemorySnapshot(label = 'manual') {
        const snapshot = {
            label,
            timestamp: Date.now(),
            usedJSHeapSize: performance.memory?.usedJSHeapSize || 0,
            totalJSHeapSize: performance.memory?.totalJSHeapSize || 0,
            jsHeapSizeLimit: performance.memory?.jsHeapSizeLimit || 0
        }

        this.memorySnapshots.push(snapshot)

        // 限制快照数量
        if (this.memorySnapshots.length > this.maxSnapshots) {
            this.memorySnapshots.shift()
        }

        return snapshot
    }

    /**
     * 获取性能指标统计
     */
    getMetricStats(category = null, name = null) {
        const stats = {}

        for (const [key, metrics] of this.metrics) {
            const [cat, n] = key.split('_')

            if (category && cat !== category) continue
            if (name && n !== name) continue

            if (metrics.length === 0) continue

            const durations = metrics.map(m => m.duration)
            stats[key] = {
                category: cat,
                name: n,
                count: metrics.length,
                avg: durations.reduce((a, b) => a + b, 0) / durations.length,
                min: Math.min(...durations),
                max: Math.max(...durations),
                median: this.calculateMedian(durations),
                p95: this.calculatePercentile(durations, 95),
                p99: this.calculatePercentile(durations, 99),
                lastValue: durations[durations.length - 1]
            }
        }

        return stats
    }

    /**
     * 获取内存使用趋势
     */
    getMemoryTrend() {
        if (this.memorySnapshots.length < 2) {
            return {trend: 'insufficient_data', snapshots: this.memorySnapshots.length}
        }

        const recent = this.memorySnapshots.slice(-10)
        const first = recent[0]
        const last = recent[recent.length - 1]

        const memoryDiff = last.usedJSHeapSize - first.usedJSHeapSize
        const timeDiff = last.timestamp - first.timestamp

        let trend = 'stable'
        if (memoryDiff > 1024 * 1024) { // 增加超过 1MB
            trend = 'increasing'
        } else if (memoryDiff < -1024 * 1024) { // 减少超过 1MB
            trend = 'decreasing'
        }

        return {
            trend,
            memoryDiff,
            timeDiff,
            growthRate: timeDiff > 0 ? memoryDiff / timeDiff : 0,
            snapshots: recent.length,
            currentUsage: last.usedJSHeapSize,
            peakUsage: Math.max(...this.memorySnapshots.map(s => s.usedJSHeapSize))
        }
    }

    /**
     * 生成性能报告
     */
    generateReport() {
        const stats = this.getMetricStats()
        const memoryTrend = this.getMemoryTrend()
        const recommendations = this.generateRecommendations(stats, memoryTrend)

        return {
            timestamp: Date.now(),
            summary: {
                totalMetrics: this.metrics.size,
                totalSnapshots: this.memorySnapshots.length,
                memoryTrend: memoryTrend.trend,
                currentMemoryUsage: memoryTrend.currentUsage
            },
            metrics: stats,
            memoryTrend,
            recommendations,
            thresholds: this.thresholds
        }
    }

    /**
     * 生成性能优化建议
     */
    generateRecommendations(stats, memoryTrend) {
        const recommendations = []

        // 检查加载性能
        for (const [key, stat] of Object.entries(stats)) {
            if (stat.category === 'load' && stat.avg > this.thresholds.loadTime) {
                recommendations.push({
                    type: 'performance',
                    severity: 'warning',
                    message: `${stat.name} 平均加载时间过长 (${stat.avg.toFixed(2)}ms)，建议优化`,
                    suggestion: '考虑使用懒加载、代码分割或缓存策略'
                })
            }

            if (stat.category === 'render' && stat.avg > this.thresholds.renderTime) {
                recommendations.push({
                    type: 'performance',
                    severity: 'warning',
                    message: `${stat.name} 渲染时间过长 (${stat.avg.toFixed(2)}ms)，可能影响用户体验`,
                    suggestion: '优化渲染逻辑，减少 DOM 操作，使用虚拟滚动'
                })
            }

            if (stat.category === 'api' && stat.avg > this.thresholds.apiResponseTime) {
                recommendations.push({
                    type: 'performance',
                    severity: 'error',
                    message: `${stat.name} API 响应时间过长 (${stat.avg.toFixed(2)}ms)`,
                    suggestion: '检查网络连接，优化 API 调用，添加超时处理'
                })
            }
        }

        // 检查内存使用
        if (memoryTrend.trend === 'increasing' && memoryTrend.growthRate > 1024 * 100) { // 每毫秒增长超过 100KB
            recommendations.push({
                type: 'memory',
                severity: 'error',
                message: '内存使用持续增长，可能存在内存泄漏',
                suggestion: '检查事件监听器是否正确清理，避免闭包引用，使用弱引用'
            })
        }

        if (memoryTrend.currentUsage > this.thresholds.memoryUsage) {
            recommendations.push({
                type: 'memory',
                severity: 'warning',
                message: `当前内存使用量较高 (${(memoryTrend.currentUsage / 1024 / 1024).toFixed(2)}MB)`,
                suggestion: '考虑清理缓存，优化数据结构，使用对象池'
            })
        }

        return recommendations
    }

    /**
     * 检查性能阈值
     */
    checkThresholds(metric) {
        const checks = [
            {threshold: this.thresholds.loadTime, category: 'load', message: '加载时间'},
            {threshold: this.thresholds.renderTime, category: 'render', message: '渲染时间'},
            {threshold: this.thresholds.apiResponseTime, category: 'api', message: 'API响应时间'}
        ]

        for (const check of checks) {
            if (metric.category === check.category && metric.duration > check.threshold) {
                console.warn(`性能警告: ${metric.name} ${check.message} ${metric.duration.toFixed(2)}ms (阈值: ${check.threshold}ms)`)
            }
        }
    }

    /**
     * 计算中位数
     */
    calculateMedian(arr) {
        const sorted = [...arr].sort((a, b) => a - b)
        const mid = Math.floor(sorted.length / 2)
        return sorted.length % 2 !== 0 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2
    }

    /**
     * 计算百分位数
     */
    calculatePercentile(arr, percentile) {
        const sorted = [...arr].sort((a, b) => a - b)
        const index = Math.ceil((percentile / 100) * sorted.length) - 1
        return sorted[Math.max(0, index)]
    }

    /**
     * 清除所有数据
     */
    clear() {
        this.metrics.clear()
        this.timers.clear()
        this.memorySnapshots = []
    }

    /**
     * 导出性能数据
     */
    exportData() {
        return {
            metrics: Object.fromEntries(this.metrics),
            memorySnapshots: this.memorySnapshots,
            thresholds: this.thresholds
        }
    }

    /**
     * 导入性能数据
     */
    importData(data) {
        if (data.metrics) {
            this.metrics = new Map(Object.entries(data.metrics))
        }
        if (data.memorySnapshots) {
            this.memorySnapshots = data.memorySnapshots
        }
        if (data.thresholds) {
            this.thresholds = {...this.thresholds, ...data.thresholds}
        }
    }
}

// 创建全局实例
const performanceMonitor = new PerformanceMonitor()

// 自动开始内存监控
setInterval(() => {
    performanceMonitor.takeMemorySnapshot('auto')
}, 30000) // 每30秒记录一次

// 导出实例和类
export {performanceMonitor, PerformanceMonitor}
export default performanceMonitor
export function useCalculator() {
    // 安全的数学表达式求值
    const safeEval = (expr) => {
        try {
            // 替换常见的数学符号
            let sanitized = expr
                .replace(/\^/g, '**')  // 幂运算
                .replace(/%/g, '/100*')  // 百分比
                .replace(/×/g, '*')  // 乘号
                .replace(/÷/g, '/')  // 除号

            // 验证表达式只包含安全的字符
            if (!/^[\d\s+\-*/().e]+$/.test(sanitized)) {
                return null
            }

            // 使用 Function 构造函数进行安全求值
            const result = new Function(`return (${sanitized})`)()

            // 检查结果是否为有效数字
            if (typeof result === 'number' && !isNaN(result) && isFinite(result)) {
                return result
            }

            return null
        } catch (error) {
            return null
        }
    }

    // 格式化数字
    const formatNumber = (num) => {
        if (typeof num !== 'number') return String(num)

        // 处理整数
        if (Number.isInteger(num)) {
            return num.toLocaleString('zh-CN')
        }

        // 处理小数，保留合理精度
        const str = String(num)
        const decimalPart = str.split('.')[1] || ''

        if (decimalPart.length > 10) {
            return num.toFixed(10).replace(/\.?0+$/, '')
        }

        return str
    }

    // 检测是否为数学表达式
    const isMathExpression = (text) => {
        const mathPattern = /^[\d\s+\-*/().%^]+$/
        return mathPattern.test(text) && /[+\-*/^%]/.test(text)
    }

    // 计算表达式
    const calculate = (expr) => {
        if (!expr || !isMathExpression(expr)) {
            return null
        }

        const result = safeEval(expr)
        if (result === null) {
            return null
        }

        return {
            expression: expr,
            result: result,
            formattedResult: formatNumber(result)
        }
    }

    // 获取计算历史（可扩展）
    const getHistory = () => {
        // 可以从本地存储获取历史记录
        return []
    }

    return {
        calculate,
        isMathExpression,
        formatNumber,
        getHistory
    }
}

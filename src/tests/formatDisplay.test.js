import {describe, it, expect} from 'vitest'
import {formatBytes, formatTimestampMs} from '../utils/formatDisplay.js'

describe('formatBytes', () => {
  it('空/零显示 0 B', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes()).toBe('0 B')
  })

  it('B 级别不保留小数', () => {
    expect(formatBytes(1)).toBe('1 B')
    expect(formatBytes(9)).toBe('9 B')
  })

  it('KB 一位小数，>=10 取整', () => {
    expect(formatBytes(1024)).toBe('1.0 KB')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(10 * 1024)).toBe('10 KB')
  })

  it('MB / GB 阶梯', () => {
    expect(formatBytes(1024 * 1024)).toBe('1.0 MB')
    expect(formatBytes(1024 * 1024 * 1024)).toBe('1.0 GB')
    expect(formatBytes(3 * 1024 * 1024 * 1024)).toBe('3.0 GB')
  })

  it('超过 GB 上限仍停在 GB 单位', () => {
    expect(formatBytes(5 * 1024 * 1024 * 1024)).toBe('5.0 GB')
    expect(formatBytes(10 * 1024 * 1024 * 1024)).toBe('10 GB')
  })
})

describe('formatTimestampMs', () => {
  it('空值返回 fallback', () => {
    expect(formatTimestampMs(0, '未执行')).toBe('未执行')
    expect(formatTimestampMs(null, 'x')).toBe('x')
    expect(formatTimestampMs(undefined, 'y')).toBe('y')
  })

  it('有效时间戳输出本地时间字符串', () => {
    const ts = new Date('2026-01-02T03:04:05.000Z').getTime()
    const out = formatTimestampMs(ts, 'n/a')
    expect(out).toBe(new Date(ts).toLocaleString())
    expect(out).not.toBe('n/a')
  })
})

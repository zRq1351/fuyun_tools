import {describe, it, expect} from 'vitest'
import {
  constrainLinePoint,
  constrainRectPoint,
  hexToRgba,
  nextNumberCalloutIndex,
  ensureShapeFillFields,
  resolveExportShapeFill,
  computeMosaicBlockParams
} from '../utils/screenshotAnnotation.js'

describe('hexToRgba', () => {
  it('标准 6 位 hex + alpha', () => {
    expect(hexToRgba('#ff0000', 0.35)).toBe('rgba(255, 0, 0, 0.35)')
  })

  it('3 位 hex 展开', () => {
    expect(hexToRgba('#f00', 1)).toBe('rgba(255, 0, 0, 1)')
  })

  it('空 hex 回退红色', () => {
    expect(hexToRgba('', 0.5)).toBe('rgba(255, 0, 0, 0.5)')
    expect(hexToRgba(null, 0.5)).toBe('rgba(255, 0, 0, 0.5)')
  })

  it('alpha clamp 到 [0,1]，非法 alpha 记 0', () => {
    expect(hexToRgba('#00ff00', 2)).toBe('rgba(0, 255, 0, 1)')
    expect(hexToRgba('#00ff00', -1)).toBe('rgba(0, 255, 0, 0)')
    expect(hexToRgba('#00ff00', 'x')).toBe('rgba(0, 255, 0, 0)')
  })
})

describe('nextNumberCalloutIndex', () => {
  it('空列表为 1', () => {
    expect(nextNumberCalloutIndex([])).toBe(1)
  })

  it('取 number 最大 n + 1', () => {
    const items = [
      {type: 'rect'},
      {type: 'number', n: 3},
      {type: 'number', n: 7},
      {type: 'number', n: 2}
    ]
    expect(nextNumberCalloutIndex(items)).toBe(8)
  })
})

describe('ensureShapeFillFields', () => {
  it('number 强制 filled / fillOpacity=1 / 补 n', () => {
    const item = ensureShapeFillFields({type: 'number'}, 5)
    expect(item.filled).toBe(true)
    expect(item.fillOpacity).toBe(1)
    expect(item.n).toBe(5)
  })

  it('rect filled 缺省 false，fillOpacity 缺省 0', () => {
    const item = ensureShapeFillFields({type: 'rect'}, 1)
    expect(item.filled).toBe(false)
    expect(item.fillOpacity).toBe(0)
  })

  it('rect 已 filled 时 fillOpacity 缺省 0.35', () => {
    const item = ensureShapeFillFields({type: 'rect', filled: true}, 1)
    expect(item.fillOpacity).toBe(0.35)
  })

  it('已有 fillOpacity 不覆盖，null 入参原样返回', () => {
    const item = ensureShapeFillFields({type: 'rect', filled: true, fillOpacity: 0.8}, 1)
    expect(item.fillOpacity).toBe(0.8)
    expect(ensureShapeFillFields(null, 1)).toBeNull()
  })
})

describe('resolveExportShapeFill', () => {
  it('number 未填 fillOpacity 时导出 filled=true / opacity=1', () => {
    const r = resolveExportShapeFill({type: 'number'})
    expect(r.filled).toBe(true)
    expect(r.fillOpacity).toBe(1)
  })

  it('普通形状按 item.filled 与显式 fillOpacity', () => {
    expect(resolveExportShapeFill({type: 'rect', filled: true})).toEqual({
      filled: true,
      fillOpacity: 0.35
    })
    expect(resolveExportShapeFill({type: 'rect', filled: false})).toEqual({
      filled: false,
      fillOpacity: 0
    })
    expect(resolveExportShapeFill({type: 'circle', filled: true, fillOpacity: 0.9})).toEqual({
      filled: true,
      fillOpacity: 0.9
    })
  })
})

describe('constrainRectPoint', () => {
  it('右下拖拽取正方形边长', () => {
    expect(constrainRectPoint({x: 0, y: 0}, {x: 80, y: 30})).toEqual({x: 80, y: 80})
  })
  it('左上拖拽保持负方向', () => {
    expect(constrainRectPoint({x: 100, y: 100}, {x: 40, y: 70})).toEqual({x: 40, y: 40})
  })
})

describe('constrainLinePoint', () => {
  it('接近水平时吸附到水平', () => {
    const p = constrainLinePoint({x: 0, y: 0}, {x: 100, y: 5})
    expect(Math.abs(p.y)).toBeLessThan(1)
    expect(p.x).toBeGreaterThan(99)
  })
  it('接近 45° 时吸附', () => {
    const p = constrainLinePoint({x: 0, y: 0}, {x: 50, y: 48})
    expect(Math.abs(p.x - p.y)).toBeLessThan(2)
  })
})

describe('computeMosaicBlockParams', () => {
  it('普通路径：scale=dpr=2, stroke=8', () => {
    const p = computeMosaicBlockParams({
      stroke: 8,
      scale: 2,
      centerX: 100,
      centerY: 80,
      srcW: 200,
      srcH: 200
    })
    expect(p.size).toBe(48)
    expect(p.blockSize).toBe(12)
    expect(p.half).toBe(24)
    expect(p.centerX).toBe(100)
    expect(p.centerY).toBe(80)
    expect(p.regionX).toBe(76)
    expect(p.regionY).toBe(56)
    expect(p.regionW).toBe(48)
    expect(p.regionH).toBe(48)
  })

  it('中心靠近原点时 region clamp 到边界内', () => {
    const p = computeMosaicBlockParams({
      stroke: 8,
      scale: 1,
      centerX: 2,
      centerY: 1,
      srcW: 10,
      srcH: 10
    })
    expect(p.size).toBe(24)
    expect(p.half).toBe(12)
    expect(p.regionX).toBe(0)
    expect(p.regionY).toBe(0)
    expect(p.regionW).toBe(10)
    expect(p.regionH).toBe(10)
  })

  it('stroke=0 回退 8，scale=0 时几何塌缩到 1', () => {
    const p = computeMosaicBlockParams({
      stroke: 0,
      scale: 0,
      centerX: 50,
      centerY: 50,
      srcW: 100,
      srcH: 100
    })
    expect(p.size).toBe(1)
    expect(p.blockSize).toBe(1)
  })
})

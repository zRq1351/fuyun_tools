export function hexToRgba(hex, opacity) {
  let h = String(hex || '#ff0000').replace('#', '')
  if (h.length === 3) {
    h = h.split('').map(c => c + c).join('')
  }
  const r = parseInt(h.slice(0, 2), 16) || 0
  const g = parseInt(h.slice(2, 4), 16) || 0
  const b = parseInt(h.slice(4, 6), 16) || 0
  const a = Math.min(1, Math.max(0, Number(opacity) || 0))
  return `rgba(${r}, ${g}, ${b}, ${a})`
}

export function nextNumberCalloutIndex(shapeItems) {
  let maxN = 0
  for (const item of shapeItems || []) {
    if (item.type === 'number' && Number(item.n) > maxN) {
      maxN = Number(item.n)
    }
  }
  return maxN + 1
}

export function ensureShapeFillFields(item, nextNumberIndex) {
  if (!item) return item
  if (item.type === 'number') {
    item.filled = true
    if (item.fillOpacity == null) item.fillOpacity = 1
    if (item.n == null) item.n = nextNumberIndex
  } else if (item.filled == null) {
    item.filled = false
  }
  if (item.fillOpacity == null) {
    item.fillOpacity = item.filled ? 0.35 : 0
  }
  return item
}

export function resolveExportShapeFill(item) {
  return {
    filled: !!item.filled || item.type === 'number',
    fillOpacity: item.fillOpacity != null
        ? Number(item.fillOpacity)
        : (item.type === 'number' ? 1 : (item.filled ? 0.35 : 0))
  }
}

export function computeMosaicBlockParams({stroke, scale, centerX, centerY, srcW, srcH}) {
  const strokePx = Math.max(1, Number(stroke) || 8)
  const scaleFactor = Number(scale) || 0
  const size = Math.max(1, Math.round(strokePx * 3 * scaleFactor))
  const blockSize = Math.max(1, Math.round(6 * scaleFactor))
  const half = Math.floor(size / 2)
  const cx = Math.round(Number(centerX) || 0)
  const cy = Math.round(Number(centerY) || 0)
  const regionX = Math.max(0, cx - half)
  const regionY = Math.max(0, cy - half)
  const regionW = Math.min((Number(srcW) || 0) - regionX, size)
  const regionH = Math.min((Number(srcH) || 0) - regionY, size)
  return {
    size,
    blockSize,
    half,
    centerX: cx,
    centerY: cy,
    regionX,
    regionY,
    regionW,
    regionH
  }
}

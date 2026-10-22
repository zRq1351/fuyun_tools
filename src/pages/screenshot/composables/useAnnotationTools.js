//! useAnnotationTools — 标注工具域 composable。
//! 职责：绘图工具状态（当前工具/颜色/线宽/形状填充/马赛克尺寸）、
//! 文字条目与形状条目的增删改查、条目样式计算（供模板 overlay 使用）、
//! 二级属性栏显隐状态。
//! 跨域依赖（历史命令落库、长截图视图变换、文字创建边界）通过 deps 注入。

import {computed, nextTick, reactive, ref} from 'vue'
import {
  ArrowUpRight,
  Circle,
  EyeOff,
  Grid3x3,
  Hash,
  Highlighter,
  Minus,
  MousePointer2,
  Pencil,
  Pipette,
  Square,
  Type
} from 'lucide-vue-next'
import {
  ensureShapeFillFields as ensureShapeFillFieldsImpl,
  hexToRgba,
  nextNumberCalloutIndex as nextNumberCalloutIndexFromItems
} from '../../../utils/screenshotAnnotation'

export function useAnnotationTools(deps) {
    const {
        toScenePoint,
        longshotResultActive,
        longshotViewScale,
        longshotViewOffset,
        getTextCreationBounds,
        appendVectorUpsertCommand,
        appendVectorDeleteCommand,
        saveToHistory
    } = deps

    // 工具数据
    const currentTool = ref('select')
    const currentColor = ref('#ff0000')
    const lineWidth = ref(3)
    const shapeFillEnabled = ref(false)
    const shapeFillOpacity = ref(0.35)
    const mosaicSize = ref(8)
    const NUMBER_DEFAULT_RADIUS = 22
    const NUMBER_MIN_RADIUS = 14
    const SHAPE_TOOLS = ['line', 'arrow', 'rect', 'circle', 'number', 'redact']
    const FILLABLE_SHAPE_TOOLS = ['rect', 'circle', 'number', 'redact']

    const textItems = ref([])
    const shapeItems = ref([])
    let textItemIdSeed = 1
    let shapeItemIdSeed = 1
    const editingTextId = ref(null)
    const selectedTextId = ref(null)
    const selectedShapeId = ref(null)
    const editingElementRef = ref(null)

    const showShapeFillControls = computed(() =>
        FILLABLE_SHAPE_TOOLS.includes(currentTool.value) ||
        FILLABLE_SHAPE_TOOLS.includes(String(shapeItems.value.find(s => s.id === selectedShapeId.value)?.type || ''))
    )
    const showMosaicSizeControl = computed(() => currentTool.value === 'mosaic')
    const selectedFillableShape = computed(() => {
      const selected = shapeItems.value.find(s => s.id === selectedShapeId.value)
      return selected && FILLABLE_SHAPE_TOOLS.includes(selected.type) ? selected : null
    })
    const showSecondaryTools = computed(() => {
      if (currentTool.value !== 'select' && currentTool.value !== 'picker') return true
      // 选中 rect/circle/number 时仍可改填充/透明度
      return !!selectedFillableShape.value
    })
    const fillControlsActive = computed(() => {
      if (selectedFillableShape.value) {
        return !!selectedFillableShape.value.filled || selectedFillableShape.value.type === 'number'
      }
      return shapeFillEnabled.value || currentTool.value === 'number'
    })

    function applyShapeFillMutation(selected, filled, opacity) {
      if (!selected) return
      selected.filled = filled
      selected.fillOpacity = filled ? opacity : 0
      ensureShapeFillFields(selected)
      appendVectorUpsertCommand('shape', selected)
      saveToHistory()
    }

    function toggleShapeFill() {
      const selected = selectedFillableShape.value
      if (currentTool.value === 'number' || selected?.type === 'number') {
        // 编号标注始终填充
        return
      }
      if (selected) {
        const nextFilled = !selected.filled
        shapeFillEnabled.value = nextFilled
        applyShapeFillMutation(
            selected,
            nextFilled,
            nextFilled ? (Number(shapeFillOpacity.value) || 0.35) : 0
        )
        return
      }
      shapeFillEnabled.value = !shapeFillEnabled.value
    }

    function onShapeFillOpacityInput() {
      const selected = selectedFillableShape.value
      if (!selected || selected.type === 'number') return
      if (!selected.filled) return
      const op = Math.min(1, Math.max(0.05, Number(shapeFillOpacity.value) || 0.35))
      selected.fillOpacity = op
      appendVectorUpsertCommand('shape', selected)
      saveToHistory()
    }

    function nextNumberCalloutIndex() {
      return nextNumberCalloutIndexFromItems(shapeItems.value)
    }

    function ensureShapeFillFields(item) {
      return ensureShapeFillFieldsImpl(item, nextNumberCalloutIndex())
    }

    const textOverlayRefMap = new Map()
    const editingBeforeText = ref('')
    const editingBeforeItem = ref(null)
    const fontFamilies = ['Arial', 'Microsoft YaHei', 'PingFang SC', 'Consolas', 'Times New Roman']
    const textStyle = reactive({
      fontFamily: 'Arial',
      bold: false,
      stroke: false,
      strokeColor: '#000000',
      shadow: false
    })

    // 定义工具
    const drawingTools = [
      {id: 'select', name: 'screenshot.selectMove', icon: MousePointer2},
      {id: 'pen', name: 'screenshot.brush', icon: Pencil},
      {id: 'highlight', name: 'screenshot.highlight', icon: Highlighter},
      {id: 'line', name: 'screenshot.line', icon: Minus},
      {id: 'arrow', name: 'screenshot.arrow', icon: ArrowUpRight},
      {id: 'rect', name: 'screenshot.rectangle', icon: Square},
      {id: 'circle', name: 'screenshot.circle', icon: Circle},
      {id: 'number', name: 'screenshot.numberCallout', icon: Hash},
      {id: 'redact', name: 'screenshot.redact', icon: EyeOff},
      {id: 'text', name: 'screenshot.text', icon: Type},
      {id: 'mosaic', name: 'screenshot.mosaic', icon: Grid3x3},
      {id: 'picker', name: 'screenshot.colorPicker', icon: Pipette}
    ]

    const movingTextStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, id: 0})
    const movingShapeStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, id: 0})
    const resizingShapeStart = reactive({x: 0, y: 0, itemX: 0, itemY: 0, itemWidth: 0, itemHeight: 0, handle: '', id: 0})
    const adjustingLinePointStart = reactive({id: 0, point: 'start'})

    function setTextOverlayRef(el, id) {
      if (!id) return
      if (el) {
        textOverlayRefMap.set(id, el)
      } else {
        textOverlayRefMap.delete(id)
      }
    }

    function startCreateTextItem(x, y) {
      const bounds = getTextCreationBounds()
      const safeX = Math.max(bounds.minX, Math.min(x, bounds.maxX))
      const safeY = Math.max(bounds.minY, Math.min(y, bounds.maxY))
      const item = {
        id: textItemIdSeed++,
        x: safeX,
        y: safeY,
        text: '',
        color: currentColor.value,
        fontSize: lineWidth.value * 8,
        fontFamily: textStyle.fontFamily,
        bold: textStyle.bold,
        stroke: textStyle.stroke,
        strokeColor: textStyle.strokeColor,
        shadow: textStyle.shadow
      }
      textItems.value.push(item)
      startEditTextItem(item)
    }

    function startEditTextItem(item) {
      currentTool.value = 'text'
      editingTextId.value = item.id
      selectedTextId.value = item.id
      editingBeforeText.value = item.text
      editingBeforeItem.value = {...item}
      currentColor.value = item.color
      lineWidth.value = Math.max(1, Math.round(item.fontSize / 8))
      textStyle.fontFamily = item.fontFamily || 'Arial'
      textStyle.bold = !!item.bold
      textStyle.stroke = !!item.stroke
      textStyle.strokeColor = item.strokeColor || '#000000'
      textStyle.shadow = !!item.shadow
      nextTick(() => {
        const el = editingElementRef.value
        if (el) {
          el.focus()
          const selection = window.getSelection()
          const range = document.createRange()
          range.selectNodeContents(el)
          range.collapse(false)
          selection.removeAllRanges()
          selection.addRange(range)
        }
      })
    }

    function setEditingElementRef(el) {
      if (!el || editingTextId.value === null) return
      editingElementRef.value = el
      const item = textItems.value.find(t => t.id === editingTextId.value)
      if (!item) return
      if (el.innerText !== item.text) {
        el.innerText = item.text
      }
    }

    function onInlineTextInput(item, event) {
      item.text = event.target.innerText.replace(/\r/g, '')
      selectedTextId.value = item.id
    }

    function handleInlineEditorKeydown(event) {
      if (event.key === 'Escape') {
        event.preventDefault()
        cancelInlineEdit()
      }
    }

    function finishInlineEdit() {
      if (editingTextId.value === null) return
      const id = editingTextId.value
      const index = textItems.value.findIndex(item => item.id === id)
      const before = editingBeforeItem.value ? {...editingBeforeItem.value} : null
      if (index === -1) {
        editingTextId.value = null
        editingElementRef.value = null
        editingBeforeText.value = ''
        editingBeforeItem.value = null
        return
      }
      const text = textItems.value[index].text.trim()
      let shouldSave = false
      if (!text) {
        textItems.value.splice(index, 1)
        selectedTextId.value = null
        if (before && String(before.text || '').trim()) {
          appendVectorDeleteCommand('text', id)
          shouldSave = true
        }
      } else {
        textItems.value[index].text = text
        selectedTextId.value = id
        const after = {...textItems.value[index]}
        if (!areFlatItemsEqual(before, after)) {
          appendVectorUpsertCommand('text', after)
          shouldSave = true
        }
      }
      editingTextId.value = null
      editingElementRef.value = null
      editingBeforeText.value = ''
      editingBeforeItem.value = null
      if (shouldSave) {
        saveToHistory()
      }
    }

    function cancelInlineEdit() {
      if (editingTextId.value === null) return
      const id = editingTextId.value
      const index = textItems.value.findIndex(item => item.id === id)
      const before = editingBeforeItem.value ? {...editingBeforeItem.value} : null
      if (index !== -1) {
        if (!String(before?.text || '').trim()) {
          textItems.value.splice(index, 1)
          selectedTextId.value = null
        } else {
          textItems.value[index] = {...before}
          selectedTextId.value = id
        }
      }
      editingTextId.value = null
      editingElementRef.value = null
      editingBeforeText.value = ''
      editingBeforeItem.value = null
    }

    function syncFillControlsFromShape(item) {
      if (!item || !FILLABLE_SHAPE_TOOLS.includes(item.type)) return
      if (item.type === 'number') {
        shapeFillEnabled.value = true
        shapeFillOpacity.value = 1
        return
      }
      shapeFillEnabled.value = !!item.filled
      shapeFillOpacity.value = item.filled
          ? (Number(item.fillOpacity) || 0.35)
          : (Number(shapeFillOpacity.value) || 0.35)
    }

    function handleResizeShapeItem(event) {
      const item = shapeItems.value.find((entry) => entry.id === resizingShapeStart.id)
      if (!item) return
      const p = toScenePoint(event)
      const dx = p.x - resizingShapeStart.x
      const dy = p.y - resizingShapeStart.y
      let x = resizingShapeStart.itemX
      let y = resizingShapeStart.itemY
      let width = resizingShapeStart.itemWidth
      let height = resizingShapeStart.itemHeight
      const handle = resizingShapeStart.handle
      if (handle.includes('l')) {
        x += dx
        width -= dx
      }
      if (handle.includes('r')) {
        width += dx
      }
      if (handle.includes('t')) {
        y += dy
        height -= dy
      }
      if (handle.includes('b')) {
        height += dy
      }
      const minSize = 4
      if (width < minSize) {
        if (handle.includes('l')) {
          x = resizingShapeStart.itemX + resizingShapeStart.itemWidth - minSize
        }
        width = minSize
      }
      if (height < minSize) {
        if (handle.includes('t')) {
          y = resizingShapeStart.itemY + resizingShapeStart.itemHeight - minSize
        }
        height = minSize
      }
      item.x = x
      item.y = y
      item.width = width
      item.height = height
    }

    function handleAdjustLineEndpoint(event) {
      const item = shapeItems.value.find((entry) => entry.id === adjustingLinePointStart.id)
      if (!item || (item.type !== 'line' && item.type !== 'arrow')) return
      const p = toScenePoint(event)
      const startAbs = {
        x: item.x + item.x1,
        y: item.y + item.y1
      }
      const endAbs = {
        x: item.x + item.x2,
        y: item.y + item.y2
      }
      if (adjustingLinePointStart.point === 'start') {
        startAbs.x = p.x
        startAbs.y = p.y
      } else {
        endAbs.x = p.x
        endAbs.y = p.y
      }
      updateLineLikeShapeFromAbsolutePoints(item, startAbs, endAbs)
    }

    function updateLineLikeShapeFromAbsolutePoints(item, startAbs, endAbs) {
      const minX = Math.min(startAbs.x, endAbs.x)
      const minY = Math.min(startAbs.y, endAbs.y)
      item.x = minX
      item.y = minY
      item.width = Math.max(1, Math.abs(endAbs.x - startAbs.x))
      item.height = Math.max(1, Math.abs(endAbs.y - startAbs.y))
      item.x1 = startAbs.x - minX
      item.y1 = startAbs.y - minY
      item.x2 = endAbs.x - minX
      item.y2 = endAbs.y - minY
      if (item.type === 'arrow') {
        const arrowHead = getArrowHeadPoints(item.x1, item.y1, item.x2, item.y2)
        item.arrowLeft = arrowHead.left
        item.arrowRight = arrowHead.right
      }
    }

    function createShapeItem(type, fromX, fromY, toX, toY) {
      const dx = toX - fromX
      const dy = toY - fromY
      const stroke = Math.max(1, Number(lineWidth.value) || 1)
      if (type === 'number') {
        const dist = Math.sqrt(dx * dx + dy * dy)
        const radius = dist < 4 ? NUMBER_DEFAULT_RADIUS : Math.max(NUMBER_MIN_RADIUS, dist)
        const n = nextNumberCalloutIndex()
        const item = ensureShapeFillFields({
          id: shapeItemIdSeed++,
          type,
          n,
          x: fromX - radius,
          y: fromY - radius,
          width: radius * 2,
          height: radius * 2,
          color: currentColor.value,
          lineWidth: Math.max(2, stroke),
          filled: true,
          fillOpacity: 1
        })
        shapeItems.value.push(item)
        return item
      }
      if (Math.abs(dx) < 2 && Math.abs(dy) < 2) return
      if (type === 'rect' || type === 'redact') {
        const item = ensureShapeFillFields({
          id: shapeItemIdSeed++,
          type,
          x: Math.min(fromX, toX),
          y: Math.min(fromY, toY),
          width: Math.max(2, Math.abs(dx)),
          height: Math.max(2, Math.abs(dy)),
          color: type === 'redact' ? '#000000' : currentColor.value,
          lineWidth: type === 'redact' ? 0 : stroke,
          filled: type === 'redact' ? true : shapeFillEnabled.value,
          fillOpacity: type === 'redact' ? 1 : (shapeFillEnabled.value ? (Number(shapeFillOpacity.value) || 0.35) : 0)
        })
        shapeItems.value.push(item)
        return item
      }
      if (type === 'circle') {
        const radius = Math.sqrt(dx * dx + dy * dy)
        const item = ensureShapeFillFields({
          id: shapeItemIdSeed++,
          type,
          x: fromX - radius,
          y: fromY - radius,
          width: Math.max(2, radius * 2),
          height: Math.max(2, radius * 2),
          color: currentColor.value,
          lineWidth: stroke,
          filled: shapeFillEnabled.value,
          fillOpacity: shapeFillEnabled.value ? (Number(shapeFillOpacity.value) || 0.35) : 0
        })
        shapeItems.value.push(item)
        return item
      }
      const minX = Math.min(fromX, toX)
      const minY = Math.min(fromY, toY)
      const width = Math.max(2, Math.abs(dx))
      const height = Math.max(2, Math.abs(dy))
      const x1 = fromX - minX
      const y1 = fromY - minY
      const x2 = toX - minX
      const y2 = toY - minY
      const arrowHead = getArrowHeadPoints(x1, y1, x2, y2)
      const item = {
        id: shapeItemIdSeed++,
        type,
        x: minX,
        y: minY,
        width,
        height,
        x1,
        y1,
        x2,
        y2,
        color: currentColor.value,
        lineWidth: stroke,
        arrowLeft: arrowHead.left,
        arrowRight: arrowHead.right
      }
      shapeItems.value.push(item)
      return item
    }

    function getArrowHeadPoints(fromX, fromY, toX, toY) {
      const angle = Math.atan2(toY - fromY, toX - fromX)
      const headLength = 12
      return {
        left: {
          x: toX - headLength * Math.cos(angle - Math.PI / 6),
          y: toY - headLength * Math.sin(angle - Math.PI / 6)
        },
        right: {
          x: toX - headLength * Math.cos(angle + Math.PI / 6),
          y: toY - headLength * Math.sin(angle + Math.PI / 6)
        }
      }
    }

    function getShapeItemStyle(shape) {
      const scale = longshotResultActive.value ? longshotViewScale.value : 1
      const ox = longshotResultActive.value ? longshotViewOffset.x : 0
      const oy = longshotResultActive.value ? longshotViewOffset.y : 0
      return {
        left: `${shape.x * scale + ox}px`,
        top: `${shape.y * scale + oy}px`,
        width: `${shape.width * scale}px`,
        height: `${shape.height * scale}px`
      }
    }

    function getShapeStrokeStyle(shape) {
      const scale = longshotResultActive.value ? longshotViewScale.value : 1
      return {
        borderColor: shape.color,
        borderWidth: `${Math.max(1, shape.lineWidth * scale)}px`
      }
    }

    function getShapeFillStyle(shape) {
      if (!shape?.filled && shape?.type !== 'redact') return {display: 'none'}
      return {
        backgroundColor: shape.type === 'redact'
            ? '#000000'
            : hexToRgba(shape.color, shape.fillOpacity ?? 0.35)
      }
    }

    function getShapeNumberStyle(shape) {
      const scale = longshotResultActive.value ? longshotViewScale.value : 1
      const w = Math.max(2, shape.width * scale)
      const h = Math.max(2, shape.height * scale)
      return {
        width: `${w}px`,
        height: `${h}px`,
        backgroundColor: shape.color,
        borderColor: shape.color,
        borderWidth: `${Math.max(1, (shape.lineWidth || 2) * scale)}px`,
        fontSize: `${Math.max(10, Math.min(w, h) * 0.45)}px`,
        lineHeight: `${h}px`
      }
    }

    function getShapeLineViewBox(shape) {
      const width = Math.max(1, Number(shape?.width) || 1)
      const height = Math.max(1, Number(shape?.height) || 1)
      return `0 0 ${width} ${height}`
    }

    function getShapePointHandleStyle(x, y) {
      const scale = longshotResultActive.value ? longshotViewScale.value : 1
      return {
        left: `${x * scale}px`,
        top: `${y * scale}px`
      }
    }

    function toggleTextBold() {
      textStyle.bold = !textStyle.bold
      syncEditingTextStyle()
    }

    function toggleTextStroke() {
      textStyle.stroke = !textStyle.stroke
      syncEditingTextStyle()
    }

    function toggleTextShadow() {
      textStyle.shadow = !textStyle.shadow
      syncEditingTextStyle()
    }

    function syncEditingTextStyle() {
      if (editingTextId.value === null) return
      const item = textItems.value.find(t => t.id === editingTextId.value)
      if (!item) return
      item.color = currentColor.value
      item.fontSize = lineWidth.value * 8
      item.fontFamily = textStyle.fontFamily
      item.bold = textStyle.bold
      item.stroke = textStyle.stroke
      item.strokeColor = textStyle.strokeColor
      item.shadow = textStyle.shadow
    }

    function getTextItemStyle(item) {
      const fontWeight = item.bold ? '700' : '400'
      const textShadow = item.shadow ? '0 2px 8px rgba(0,0,0,0.6)' : 'none'
      const scale = longshotResultActive.value ? longshotViewScale.value : 1
      const ox = longshotResultActive.value ? longshotViewOffset.x : 0
      const oy = longshotResultActive.value ? longshotViewOffset.y : 0
      const displayFontSize = Math.max(8, item.fontSize * scale)
      const strokeWidth = Math.max(1, Math.round(displayFontSize / 14))
      return {
        left: `${item.x * scale + ox}px`,
        top: `${item.y * scale + oy}px`,
        color: item.color,
        fontSize: `${displayFontSize}px`,
        fontFamily: item.fontFamily || 'Arial',
        fontWeight,
        textShadow,
        WebkitTextStroke: item.stroke ? `${strokeWidth}px ${item.strokeColor || '#000000'}` : '0'
      }
    }

    function drawArrowHead(ctx, fromX, fromY, toX, toY) {
      const angle = Math.atan2(toY - fromY, toX - fromX)
      const headLength = 15
      ctx.beginPath()
      ctx.moveTo(toX, toY)
      ctx.lineTo(toX - headLength * Math.cos(angle - Math.PI / 6), toY - headLength * Math.sin(angle - Math.PI / 6))
      ctx.moveTo(toX, toY)
      ctx.lineTo(toX - headLength * Math.cos(angle + Math.PI / 6), toY - headLength * Math.sin(angle + Math.PI / 6))
      ctx.stroke()
    }

    function cloneTextItem(item) {
      return item ? {...item} : null
    }

    function cloneShapeItem(item) {
      if (!item) return null
      return {...item}
    }

    function areFlatItemsEqual(a, b) {
      if (a === b) return true
      if (!a || !b) return false
      const keys = new Set([...Object.keys(a), ...Object.keys(b)])
      for (const key of keys) {
        if (a[key] !== b[key]) {
          return false
        }
      }
      return true
    }

    return {
        currentTool,
        currentColor,
        lineWidth,
        shapeFillEnabled,
        shapeFillOpacity,
        mosaicSize,
        NUMBER_DEFAULT_RADIUS,
        NUMBER_MIN_RADIUS,
        SHAPE_TOOLS,
        textItems,
        shapeItems,
        editingTextId,
        selectedTextId,
        selectedShapeId,
        editingElementRef,
        showShapeFillControls,
        showMosaicSizeControl,
        showSecondaryTools,
        fillControlsActive,
        textOverlayRefMap,
        editingBeforeItem,
        fontFamilies,
        textStyle,
        drawingTools,
        setTextOverlayRef,
        toggleShapeFill,
        onShapeFillOpacityInput,
        nextNumberCalloutIndex,
        startCreateTextItem,
        startEditTextItem,
        setEditingElementRef,
        onInlineTextInput,
        handleInlineEditorKeydown,
        finishInlineEdit,
        cancelInlineEdit,
        syncFillControlsFromShape,
        handleResizeShapeItem,
        handleAdjustLineEndpoint,
        createShapeItem,
        getShapeItemStyle,
        getShapeStrokeStyle,
        getShapeFillStyle,
        getShapeNumberStyle,
        getShapeLineViewBox,
        getShapePointHandleStyle,
        toggleTextBold,
        toggleTextStroke,
        toggleTextShadow,
        syncEditingTextStyle,
        getTextItemStyle,
        cloneTextItem,
        cloneShapeItem,
        areFlatItemsEqual,
        drawArrowHead,
        movingTextStart,
        movingShapeStart,
        resizingShapeStart,
        adjustingLinePointStart
    }
}

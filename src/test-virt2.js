import { ref, watchEffect } from 'vue'
import { useVirtualList } from '@vueuse/core'

const list = ref(Array.from({ length: 21 }, (_, i) => i))
const { containerProps, wrapperProps, list: virtualList } = useVirtualList(list, { itemWidth: 258 })

const containerRef = {
  clientWidth: 1000,
  scrollWidth: 5418,
  scrollLeft: 4000,
  addEventListener: () => {},
  removeEventListener: () => {}
}

containerProps.ref.value = containerRef
containerProps.onScroll({ target: containerRef })

setTimeout(() => {
  console.log('virtualList:', virtualList.value.map(i => i.index))
  console.log('wrapperProps:', wrapperProps.value)
}, 100)

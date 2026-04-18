import { ref, watchEffect } from 'vue'
import { useVirtualList } from '@vueuse/core'

const list = ref(Array.from({ length: 21 }, (_, i) => i))
const { containerProps, wrapperProps, list: virtualList } = useVirtualList(list, { itemWidth: 258, overscan: 10 })

const containerRef = {
  clientWidth: 774,
  scrollWidth: 5418,
  scrollLeft: 0,
  addEventListener: () => {},
  removeEventListener: () => {}
}

containerProps.ref.value = containerRef
containerProps.onScroll({ target: containerRef })

setTimeout(() => {
  console.log('At scrollLeft = 0:')
  console.log('virtualList:', virtualList.value.map(i => i.index))
  console.log('wrapperProps:', wrapperProps.value)
  
  containerRef.scrollLeft = 4000;
  containerProps.onScroll({ target: containerRef })
  
  setTimeout(() => {
    console.log('At scrollLeft = 4000:')
    console.log('virtualList:', virtualList.value.map(i => i.index))
    console.log('wrapperProps:', wrapperProps.value)
  }, 100)
}, 100)

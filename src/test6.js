import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const containerRef = ref({ clientWidth: 500, scrollLeft: 1950, scrollTop: 0 })
const { list, containerProps, wrapperProps } = useVirtualList(ref(Array(100).fill(1)), { itemWidth: 20, overscan: 10 })

containerProps.ref.value = containerRef.value
containerProps.onScroll({ target: containerRef.value })

console.log(wrapperProps.value)
console.log('List length:', list.value.length)

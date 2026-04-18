import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const containerRef = ref({ clientWidth: 500, scrollLeft: 500, scrollTop: 0 })
const { containerProps, wrapperProps } = useVirtualList(ref(Array(100).fill(1)), { itemWidth: 20 })

containerProps.ref.value = containerRef.value

containerProps.onScroll({ target: containerRef.value })

console.log(wrapperProps.value)

import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const containerRef = ref({ clientWidth: 800, scrollLeft: 2000, scrollTop: 0 })
const { list, containerProps, wrapperProps } = useVirtualList(ref(Array(21).fill(1)), { itemWidth: 258, overscan: 10 })

containerProps.ref.value = containerRef.value
containerProps.onScroll({ target: containerRef.value })

console.log('width:', wrapperProps.value.style.width)
console.log('marginLeft:', wrapperProps.value.style.marginLeft)

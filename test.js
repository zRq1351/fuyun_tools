import { useVirtualList } from "@vueuse/core"
import { ref } from "vue"

const list = ref(Array.from({ length: 100 }, (_, i) => i))
const { containerProps, wrapperProps } = useVirtualList(list, { itemWidth: 258 })
console.log(wrapperProps.value)
// Simulate scrolling
containerProps.onScroll({ target: { scrollLeft: 2580, scrollWidth: 25800, clientWidth: 1000 } })
console.log(wrapperProps.value)

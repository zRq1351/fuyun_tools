import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const { wrapperProps } = useVirtualList(ref(Array(21).fill(1)), { itemWidth: 258 })
console.log(wrapperProps.value)

const { wrapperProps: verticalProps } = useVirtualList(ref(Array(21).fill(1)), { itemHeight: 258 })
console.log(verticalProps.value)

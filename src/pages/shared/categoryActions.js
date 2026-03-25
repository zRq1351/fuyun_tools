export const runCategoryAssignment = async ({
                                                itemKey,
                                                category,
                                                applyLocal,
                                                persist,
                                                onError,
                                                onFinally
                                            }) => {
    if (!itemKey || category === '全部') {
        if (typeof onFinally === 'function') {
            onFinally()
        }
        return false
    }
    if (typeof applyLocal === 'function') {
        applyLocal(itemKey, category)
    }
    try {
        if (typeof persist === 'function') {
            await Promise.resolve(persist(itemKey, category))
        }
        return true
    } catch (error) {
        if (typeof onError === 'function') {
            onError(error)
        }
        return false
    } finally {
        if (typeof onFinally === 'function') {
            onFinally()
        }
    }
}

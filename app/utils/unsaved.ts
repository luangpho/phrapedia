/**
 * Guard against losing unsaved edits: browser beforeunload + in-app navigation confirm.
 * Call from a page's setup.
 */
export function useUnsavedGuard(dirty: Ref<boolean> | ComputedRef<boolean>, message = '保存されていない変更があります。破棄して移動しますか？') {
  const onBeforeUnload = (e: BeforeUnloadEvent) => {
    if (!dirty.value) return
    e.preventDefault()
    e.returnValue = ''
  }
  onMounted(() => window.addEventListener('beforeunload', onBeforeUnload))
  onBeforeUnmount(() => window.removeEventListener('beforeunload', onBeforeUnload))

  const guard = () => {
    if (!dirty.value) return true
    return window.confirm(message)
  }
  onBeforeRouteLeave(guard)
  onBeforeRouteUpdate((to, from) => (to.path === from.path ? true : guard()))
}

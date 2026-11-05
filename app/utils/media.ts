/** Reactive media query (client only; false during SSR and before mount). */
export function useMediaQueryRef(query: string) {
  const matches = ref(false)
  let cleanup: (() => void) | undefined
  onMounted(() => {
    const mql = window.matchMedia(query)
    matches.value = mql.matches
    const onChange = (e: MediaQueryListEvent) => { matches.value = e.matches }
    mql.addEventListener('change', onChange)
    cleanup = () => mql.removeEventListener('change', onChange)
  })
  onBeforeUnmount(() => cleanup?.())
  return matches
}

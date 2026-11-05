import type { EntityKind } from '#shared/types'

/**
 * The entity kind shown in the sidebar (高僧 / 寺院 tabs).
 * Backed by useState; follows the route when on /<kind>/... pages.
 */
export function useEntityKind() {
  const kind = useState<EntityKind>('entity-kind', () => 'monks')
  const route = useRoute()
  const fromRoute = computed(() => {
    const k = route.params.kind
    return isEntityKind(k) ? k : undefined
  })
  // Keep in sync with the route; cheap and idempotent even when called from several components.
  watch(fromRoute, (k) => { if (k && k !== kind.value) kind.value = k }, { immediate: true })
  return kind
}

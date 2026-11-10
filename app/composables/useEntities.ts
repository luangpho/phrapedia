import type { EntityKind, EntitySummary } from '#shared/types'

export const entityListKey = (kind: EntityKind) => `entities:${kind}`
export const entityKey = (kind: EntityKind, slug: string) => `entity:${kind}:${slug}`

/**
 * List of records of one kind. The data is shared through the Nuxt payload by key
 * (`entities:<kind>`), so every component using the same kind sees the same cache and
 * `refreshList(kind)` updates all of them (e.g. the sidebar after save/delete).
 */
export function useEntityList(kind: MaybeRefOrGetter<EntityKind>) {
  return useAsyncData<EntitySummary[]>(() => entityListKey(toValue(kind)), () => api.listEntities(toValue(kind)), {
    default: () => [],
    dedupe: 'defer',
    watch: [() => toValue(kind)],
  })
}

export function useEntities() {
  async function refreshList(kind: EntityKind) {
    // The changed-file count in the sidebar sync button changes with every write as well.
    await refreshNuxtData([entityListKey(kind), 'sync-status'])
  }

  function get(kind: EntityKind, slug: string) {
    return api.getEntity(kind, slug)
  }

  async function create(kind: EntityKind, raw: string, message?: string) {
    const res = await api.createEntity(kind, raw, message)
    await refreshList(kind)
    return res
  }

  async function update(kind: EntityKind, slug: string, raw: string, message?: string) {
    const res = await api.updateEntity(kind, slug, raw, message)
    await refreshList(kind)
    return res
  }

  async function remove(kind: EntityKind, slug: string) {
    const res = await api.deleteEntity(kind, slug)
    clearNuxtData(entityKey(kind, slug))
    await refreshList(kind)
    return res
  }

  async function template(kind: EntityKind, name = '') {
    return await api.entityTemplate(kind, name)
  }

  return { refreshList, get, create, update, remove, template }
}

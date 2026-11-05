import type { EntityKind, EntityRecord, EntitySummary } from '#shared/types'

export interface SaveEntityResult {
  record: EntityRecord
  commit?: unknown
}

export const entityListKey = (kind: EntityKind) => `entities:${kind}`
export const entityKey = (kind: EntityKind, slug: string) => `entity:${kind}:${slug}`

/**
 * List of records of one kind. The data is shared through the Nuxt payload by key
 * (`entities:<kind>`), so every component using the same kind sees the same cache and
 * `refreshList(kind)` updates all of them (e.g. the sidebar after save/delete).
 */
export function useEntityList(kind: MaybeRefOrGetter<EntityKind>) {
  return useFetch<EntitySummary[]>(() => `/api/entities/${toValue(kind)}`, {
    key: computed(() => entityListKey(toValue(kind))),
    default: () => [],
    dedupe: 'defer',
  })
}

export function useEntities() {
  async function refreshList(kind: EntityKind) {
    // The changed-file count in the sidebar sync button changes with every write as well.
    await refreshNuxtData([entityListKey(kind), 'sync-status'])
  }

  function get(kind: EntityKind, slug: string) {
    return $fetch<EntityRecord>(`/api/entities/${kind}/${encodeURIComponent(slug)}`)
  }

  async function create(kind: EntityKind, raw: string, message?: string) {
    const res = await $fetch<SaveEntityResult>(`/api/entities/${kind}`, { method: 'POST', body: { raw, message } })
    await refreshList(kind)
    return res
  }

  async function update(kind: EntityKind, slug: string, raw: string, message?: string) {
    const res = await $fetch<SaveEntityResult>(`/api/entities/${kind}/${encodeURIComponent(slug)}`, { method: 'PUT', body: { raw, message } })
    await refreshList(kind)
    return res
  }

  async function remove(kind: EntityKind, slug: string) {
    const res = await $fetch<{ ok: boolean; commit?: unknown }>(`/api/entities/${kind}/${encodeURIComponent(slug)}`, { method: 'DELETE' })
    clearNuxtData(entityKey(kind, slug))
    await refreshList(kind)
    return res
  }

  async function template(kind: EntityKind, name = '') {
    const res = await $fetch<{ raw: string }>(`/api/entities/${kind}/template`, { query: name ? { name } : undefined })
    return res.raw
  }

  return { refreshList, get, create, update, remove, template }
}

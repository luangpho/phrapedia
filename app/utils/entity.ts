import type { EntityKind, EntityStatus } from '#shared/types'

export const UI_KINDS: readonly EntityKind[] = ['monks', 'temples']

export function isEntityKind(v: unknown): v is EntityKind {
  return v === 'monks' || v === 'temples' || v === 'amulets'
}

export function kindLabel(kind: EntityKind): string {
  return kind === 'monks' ? '高僧' : kind === 'temples' ? '寺院' : 'プラクルアン'
}

export function kindIcon(kind: EntityKind): string {
  return kind === 'monks' ? 'i-lucide-user-round' : kind === 'temples' ? 'i-lucide-landmark' : 'i-lucide-gem'
}

export const STATUS_META: Record<EntityStatus, { label: string; color: 'neutral' | 'warning' | 'success' }> = {
  draft: { label: '下書き', color: 'neutral' },
  reviewed: { label: 'レビュー済', color: 'warning' },
  verified: { label: '検証済', color: 'success' },
}

export function statusMeta(status: string | undefined) {
  return STATUS_META[(status as EntityStatus) ?? 'draft'] ?? STATUS_META.draft
}

/** Extract a human readable message from a $fetch / useFetch error. */
export function errorMessage(err: unknown, fallback = 'エラーが発生しました'): string {
  if (!err) return fallback
  const e = err as { data?: { message?: string; statusMessage?: string }; statusMessage?: string; message?: string }
  return e.data?.message || e.data?.statusMessage || e.statusMessage || e.message || fallback
}

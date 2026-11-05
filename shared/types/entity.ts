/** Kinds of records stored under data/<kind>/. Amulets are reserved for a later phase (not in the MVP UI). */
export type EntityKind = 'monks' | 'temples' | 'amulets'

export const ENTITY_KINDS: readonly EntityKind[] = ['monks', 'temples', 'amulets'] as const

/** Kinds exposed in the MVP UI (tabs in the sidebar). */
export const MVP_ENTITY_KINDS: readonly EntityKind[] = ['monks', 'temples'] as const

export const ENTITY_KIND_LABELS: Record<EntityKind, { ja: string; en: string; singularJa: string }> = {
  monks: { ja: '高僧', en: 'Monks', singularJa: '高僧' },
  temples: { ja: '寺院', en: 'Temples', singularJa: '寺院' },
  amulets: { ja: 'プラクルアン', en: 'Amulets', singularJa: 'プラクルアン' },
}

export type EntityStatus = 'draft' | 'reviewed' | 'verified'

/**
 * YAML frontmatter of a record. Everything is optional except `slug`, `kind`, `name`
 * so that hand-written files stay valid. Unknown keys are preserved on round-trip.
 */
export interface EntityFrontmatter {
  slug: string
  kind: EntityKind
  /** Display name (Japanese preferred, e.g. ルアンポー・トー) */
  name: string
  name_th?: string
  name_en?: string
  aliases?: string[]
  /** Monks: home temple slug or name. Temples: not used. */
  temple?: string
  /** Temples: province; Monks: province of the temple */
  province?: string
  /** Monks: birth year or date (string, may be Buddhist era) */
  born?: string
  died?: string
  /** Temples: founding year or period */
  founded?: string
  status?: EntityStatus
  /** Relative paths (under the sources dir) or URLs that back the content */
  sources?: string[]
  tags?: string[]
  created_at?: string
  updated_at?: string
  [key: string]: unknown
}

export interface EntitySummary {
  kind: EntityKind
  slug: string
  name: string
  name_th?: string
  name_en?: string
  status: EntityStatus
  updatedAt?: string
}

export interface EntityRecord extends EntitySummary {
  /** Parsed frontmatter */
  frontmatter: EntityFrontmatter
  /** Markdown body without the frontmatter block */
  body: string
  /** Full file content (frontmatter + body) */
  raw: string
  /** Path relative to the project root (e.g. data/monks/luang-pho-to.md) */
  path: string
}

export interface SaveEntityInput {
  /** Full markdown content including frontmatter. If frontmatter lacks slug/kind they are filled in. */
  raw: string
  /** Optional commit message hint used by auto-commit */
  message?: string
}

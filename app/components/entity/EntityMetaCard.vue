<script setup lang="ts">
import type { EntityFrontmatter, EntityKind } from '#shared/types'

const props = defineProps<{ kind: EntityKind; frontmatter: EntityFrontmatter; path?: string }>()

const fm = computed(() => props.frontmatter)

const rows = computed(() => {
  const r: { label: string; value: string; to?: string }[] = []
  const f = fm.value
  if (props.kind === 'monks') {
    if (f.temple) {
      const isSlug = /^[a-z0-9][a-z0-9-]*$/.test(String(f.temple))
      r.push({ label: '所属寺院', value: String(f.temple), to: isSlug ? `/temples/${f.temple}` : undefined })
    }
    if (f.province) r.push({ label: '県', value: String(f.province) })
    if (f.born || f.died) r.push({ label: '生没年', value: `${f.born ?? '?'} – ${f.died ?? ''}` })
  } else {
    if (f.province) r.push({ label: '県', value: String(f.province) })
    if (f.founded) r.push({ label: '創建', value: String(f.founded) })
  }
  if (f.aliases?.length) r.push({ label: '別名', value: f.aliases.join('、') })
  return r
})

const sources = computed(() => (fm.value.sources ?? []).map(String))
</script>

<template>
  <UCard :ui="{ body: 'p-4 sm:p-4 space-y-4' }">
    <dl v-if="rows.length" class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
      <template v-for="row in rows" :key="row.label">
        <dt class="text-muted">{{ row.label }}</dt>
        <dd class="text-highlighted min-w-0 break-words">
          <ULink v-if="row.to" :to="row.to" class="text-primary hover:underline">{{ row.value }}</ULink>
          <span v-else>{{ row.value }}</span>
        </dd>
      </template>
    </dl>

    <div v-if="fm.tags?.length" class="flex flex-wrap gap-1.5" aria-label="タグ">
      <UBadge v-for="tag in fm.tags" :key="tag" :label="String(tag)" color="neutral" variant="outline" size="sm" icon="i-lucide-tag" />
    </div>

    <div>
      <h3 class="text-xs font-medium text-muted mb-1.5">出典（frontmatter）</h3>
      <ul v-if="sources.length" class="space-y-1 text-sm">
        <li v-for="src in sources" :key="src" class="flex items-start gap-1.5 min-w-0">
          <template v-if="isExternalUrl(src)">
            <UIcon name="i-lucide-globe" class="size-4 mt-0.5 shrink-0 text-muted" />
            <ULink :to="src" target="_blank" rel="noopener" class="truncate text-primary hover:underline">{{ src }}</ULink>
          </template>
          <template v-else>
            <UIcon name="i-lucide-file-text" class="size-4 mt-0.5 shrink-0 text-muted" />
            <ULink :to="{ path: '/sources', query: { path: src } }" class="truncate text-primary hover:underline">{{ src }}</ULink>
          </template>
        </li>
      </ul>
      <p v-else class="text-sm text-muted">未設定</p>
    </div>

    <p v-if="path" class="text-xs text-dimmed font-mono break-all">{{ path }}</p>
  </UCard>
</template>

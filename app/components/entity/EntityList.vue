<script setup lang="ts">
import type { EntityKind, EntitySummary } from '#shared/types'

const props = defineProps<{ kind: EntityKind; query?: string }>()
const emit = defineEmits<{ select: [item: EntitySummary] }>()

const route = useRoute()
const { data, status, error, refresh } = useEntityList(() => props.kind)

const items = computed(() => {
  const q = (props.query ?? '').trim().toLowerCase()
  const list = [...(data.value ?? [])].sort((a, b) => a.name.localeCompare(b.name, 'ja'))
  if (!q) return list
  return list.filter(e => [e.name, e.name_th, e.name_en, e.slug].some(v => v?.toLowerCase().includes(q)))
})

const loading = computed(() => status.value === 'pending' && !(data.value?.length))

function isActive(item: EntitySummary) {
  return route.params.kind === item.kind && route.params.slug === item.slug
}
</script>

<template>
  <div class="flex flex-col gap-0.5" :aria-busy="loading">
    <template v-if="loading">
      <div v-for="i in 6" :key="i" class="flex items-center gap-2 px-2 py-2">
        <div class="flex-1 space-y-1.5">
          <USkeleton class="h-3.5 w-3/4" />
          <USkeleton class="h-3 w-1/2" />
        </div>
        <USkeleton class="h-5 w-12" />
      </div>
    </template>

    <div v-else-if="error" class="px-2 py-3 text-sm space-y-2">
      <p class="text-error">一覧を読み込めませんでした</p>
      <p class="text-xs text-muted break-all">{{ errorMessage(error) }}</p>
      <UButton size="xs" color="neutral" variant="outline" icon="i-lucide-rotate-ccw" label="再読み込み" @click="refresh()" />
    </div>

    <p v-else-if="!data?.length" class="px-2 py-3 text-sm text-muted">
      まだ登録がありません
    </p>

    <p v-else-if="!items.length" class="px-2 py-3 text-sm text-muted">
      「{{ query }}」に一致する項目はありません
    </p>

    <nav v-else :aria-label="`${kindLabel(kind)}一覧`">
      <ul class="flex flex-col gap-0.5">
        <li v-for="item in items" :key="item.slug">
          <ULink
            :to="`/${item.kind}/${item.slug}`"
            :aria-current="isActive(item) ? 'page' : undefined"
            class="group flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-primary"
            :class="isActive(item) ? 'bg-elevated text-highlighted' : 'text-default hover:bg-elevated/60'"
            @click="emit('select', item)"
          >
            <span class="min-w-0 flex-1">
              <span class="block truncate font-medium">{{ item.name }}</span>
              <span v-if="item.name_th || item.name_en" class="block truncate text-xs text-muted">{{ item.name_th || item.name_en }}</span>
            </span>
            <EntityStatusBadge :status="item.status" size="xs" />
          </ULink>
        </li>
      </ul>
    </nav>
  </div>
</template>

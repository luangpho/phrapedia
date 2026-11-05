<script setup lang="ts">
import type { EntityKind } from '#shared/types'

/**
 * Two-column work area: main content (default slot) + AI chat column.
 * On lg+ the chat is a fixed-width column on the right; below lg it opens in a slideover.
 */
const props = defineProps<{
  kind: EntityKind
  slug: string | null
  draft: string
  hint?: string
}>()
const emit = defineEmits<{ draft: [markdown: string, summary?: string] }>()
const open = defineModel<boolean>('open', { default: false })

const isLg = useMediaQueryRef('(min-width: 1024px)')
const mounted = ref(false)
onMounted(() => { mounted.value = true })

const showColumn = computed(() => mounted.value && open.value && isLg.value)
const showSlideover = computed({
  get: () => mounted.value && open.value && !isLg.value,
  set: (v: boolean) => { open.value = v },
})

function onDraft(markdown: string, summary?: string) {
  emit('draft', markdown, summary)
}
</script>

<template>
  <div class="flex flex-1 min-h-0 min-w-0">
    <div class="flex-1 min-w-0 overflow-y-auto p-4 sm:p-6">
      <slot />
    </div>

    <aside
      v-if="showColumn"
      class="flex flex-col min-h-0 w-[400px] xl:w-[460px] shrink-0 border-s border-default bg-elevated/25"
      aria-label="AI チャット"
    >
      <p v-if="hint" class="px-4 py-2.5 text-xs text-muted border-b border-default leading-5">
        <UIcon name="i-lucide-sparkles" class="size-3.5 align-[-2px] text-primary me-1" />{{ hint }}
      </p>
      <div class="flex-1 min-h-0 flex flex-col">
        <ChatPanel :kind="props.kind" :slug="props.slug" :draft="props.draft" @draft="onDraft" />
      </div>
    </aside>

    <USlideover v-model:open="showSlideover" title="AIに相談" :description="hint" side="right" :ui="{ body: 'p-0 sm:p-0 flex flex-col min-h-0' }">
      <template #body>
        <ChatPanel :kind="props.kind" :slug="props.slug" :draft="props.draft" @draft="onDraft" />
      </template>
    </USlideover>
  </div>
</template>

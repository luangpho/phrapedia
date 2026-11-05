<script setup lang="ts">
import type { EntityKind } from '#shared/types'

/**
 * Two-column work area: main content (default slot) + AI chat column.
 * On lg+ the chat is a fixed-width column on the right; below lg it opens as an overlay drawer.
 * The <ChatPanel> is mounted once and only hidden/shown (v-show), so closing the column or crossing the
 * breakpoint does not unmount it and does not abort an AI turn that is still running.
 */
const props = defineProps<{
  kind: EntityKind
  slug: string | null
  draft: string
  hint?: string
}>()
const emit = defineEmits<{
  draft: [markdown: string, summary?: string]
  saved: [saved: { kind: EntityKind; slug: string; path?: string }]
  restored: [lastDraft: { markdown: string; summary?: string } | null]
}>()
const open = defineModel<boolean>('open', { default: false })

const isLg = useMediaQueryRef('(min-width: 1024px)')
const mounted = ref(false)
onMounted(() => { mounted.value = true })

const showColumn = computed(() => mounted.value && open.value && isLg.value)
const showDrawer = computed(() => mounted.value && open.value && !isLg.value)

function close() {
  open.value = false
}

// Esc closes the drawer on small screens
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && showDrawer.value) close()
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="flex flex-1 min-h-0 min-w-0">
    <div class="flex-1 min-w-0 overflow-y-auto p-4 sm:p-6">
      <slot />
    </div>

    <!-- backdrop for the drawer (below lg) -->
    <Transition enter-active-class="transition-opacity duration-200" enter-from-class="opacity-0" leave-active-class="transition-opacity duration-150" leave-to-class="opacity-0">
      <div v-if="showDrawer" class="fixed inset-0 z-40 bg-elevated/75 backdrop-blur-sm lg:hidden" aria-hidden="true" @click="close" />
    </Transition>

    <aside
      v-show="showColumn || showDrawer"
      :class="[
        'flex flex-col min-h-0 shrink-0 border-s border-default bg-default',
        showDrawer
          ? 'fixed inset-y-0 end-0 z-50 w-full max-w-md shadow-xl'
          : 'w-[400px] xl:w-[460px] bg-elevated/25',
      ]"
      aria-label="AI チャット"
      :aria-modal="showDrawer ? 'true' : undefined"
      :role="showDrawer ? 'dialog' : undefined"
    >
      <div v-if="showDrawer" class="flex items-center justify-between gap-2 px-4 py-2 border-b border-default">
        <span class="text-sm font-semibold text-highlighted">AIに相談</span>
        <UButton icon="i-lucide-x" color="neutral" variant="ghost" size="sm" aria-label="チャットを閉じる" @click="close" />
      </div>
      <p v-if="hint" class="px-4 py-2.5 text-xs text-muted border-b border-default leading-5">
        <UIcon name="i-lucide-sparkles" class="size-3.5 align-[-2px] text-primary me-1" />{{ hint }}
      </p>
      <div class="flex-1 min-h-0 flex flex-col">
        <ChatPanel
          v-if="mounted"
          :kind="props.kind"
          :slug="props.slug"
          :draft="props.draft"
          @draft="(m, s) => emit('draft', m, s)"
          @saved="s => emit('saved', s)"
          @restored="d => emit('restored', d)"
        />
      </div>
    </aside>
  </div>
</template>

<script setup lang="ts">
const props = withDefaults(defineProps<{
  modelValue: string
  placeholder?: string
  /** Disable the editor (e.g. while saving) */
  disabled?: boolean
}>(), { placeholder: 'Markdown（先頭の --- で囲まれた部分はメタデータ）' })

const emit = defineEmits<{
  'update:modelValue': [value: string]
  'save': []
}>()

const mode = ref<'edit' | 'preview'>('edit')

const value = computed({
  get: () => props.modelValue,
  set: (v: string) => emit('update:modelValue', v),
})

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 's') {
    e.preventDefault()
    emit('save')
  }
}

const lineCount = computed(() => (props.modelValue.match(/\n/g)?.length ?? 0) + 1)
</script>

<template>
  <div class="flex flex-col gap-3" @keydown="onKeydown">
    <div class="flex items-center justify-between gap-2">
      <UFieldGroup size="sm">
        <UButton
          icon="i-lucide-pencil"
          label="編集"
          color="neutral"
          :variant="mode === 'edit' ? 'subtle' : 'outline'"
          :aria-pressed="mode === 'edit'"
          @click="mode = 'edit'"
        />
        <UButton
          icon="i-lucide-eye"
          label="プレビュー"
          color="neutral"
          :variant="mode === 'preview' ? 'subtle' : 'outline'"
          :aria-pressed="mode === 'preview'"
          @click="mode = 'preview'"
        />
      </UFieldGroup>
      <span class="text-xs text-muted">
        {{ lineCount }} 行 ・ <UKbd value="meta" size="sm" /> <UKbd value="S" size="sm" /> で保存
      </span>
    </div>

    <UTextarea
      v-show="mode === 'edit'"
      v-model="value"
      :rows="24"
      autoresize
      :maxrows="0"
      :disabled="disabled"
      :placeholder="placeholder"
      spellcheck="false"
      aria-label="Markdown エディタ"
      class="w-full"
      :ui="{ base: 'font-mono text-sm leading-6' }"
    />

    <div v-if="mode === 'preview'" class="rounded-md border border-default p-4 sm:p-6 min-h-64">
      <MarkdownView :source="modelValue" />
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ChatMessage, ChatPart, EntityKind } from '#shared/types'

/**
 * <ChatPanel> — AI assistant column next to the record editor.
 * The AI edits the working draft via update_draft; every draft update is emitted as `draft`
 * (the parent applies it to the editor). Saving stays a user action.
 */
const props = defineProps<{
  kind: EntityKind
  slug: string | null
  draft: string
}>()

const emit = defineEmits<{
  draft: [markdown: string, summary?: string]
  /** The AI saved a record through save_entity */
  saved: [saved: { kind: EntityKind; slug: string; path?: string }]
  /** A stored conversation was restored; carries its last AI draft (null when none) */
  restored: [lastDraft: { markdown: string; summary?: string } | null]
}>()

type ToolPart = Extract<ChatPart, { type: 'tool' }>
type DraftPart = Extract<ChatPart, { type: 'draft' }>

const input = ref('')

const { messages, uiMessages, status, error, loading, send, stop, retry, reset } = useChat({
  kind: () => props.kind,
  slug: () => props.slug,
  draft: () => props.draft,
  onDraft: (markdown, summary) => emit('draft', markdown, summary),
  onSaved: saved => emit('saved', saved),
  onRestored: last => emit('restored', last),
})

const { data: settings } = useSettings()
const providerLabel = computed(() => {
  const p = settings.value?.ai.provider
  if (p === 'anthropic') return 'Claude'
  if (p === 'gemini-cli') return settings.value?.capabilities?.geminiEngine?.replace(/ \(.*\)$/, '') || 'Google CLI'
  return null
})
const missingKey = computed(() => settings.value?.ai.provider === 'anthropic' && settings.value?.capabilities?.anthropicConfigured === false)

const byId = computed(() => {
  const map = new Map<string, ChatMessage>()
  for (const m of messages.value) map.set(m.id, m)
  return map
})

const busy = computed(() => status.value === 'submitted' || status.value === 'streaming')

const examples = computed(() => {
  if (props.kind === 'temples') {
    return [
      'ワット・ラカンについて一次資料をもとに下書きを作成して',
      'この寺院の歴史と所在地を一次資料で確認して追記して',
      '関連する高僧を一次資料から洗い出して',
    ]
  }
  return [
    'ルアンポー・トーについて一次資料をもとに下書きを作成して',
    'この高僧の所属寺院と代表的なプラクルアンを調べて追記して',
    '下書きの出典を確認して、未確認の情報を洗い出して',
  ]
})

const TOOL_LABELS: Record<string, string> = {
  search_sources: '一次資料を検索',
  list_sources: '一次資料の一覧',
  read_source: '一次資料を読む',
  web_research: 'Web 調査（Google CLI）',
  list_entities: 'レコード一覧',
  get_entity: 'レコードを取得',
  save_entity: 'レコードを保存',
  entity_template: 'テンプレート取得',
  sync_status: '同期状態',
  sync_data: 'Git 同期',
}

function toolLabel(name: string) {
  return TOOL_LABELS[name] ?? name
}

function preview(value: unknown, limit = 160): string {
  if (value === undefined || value === null) return ''
  const text = typeof value === 'string' ? value : JSON.stringify(value)
  return text.length > limit ? `${text.slice(0, limit)}…` : text
}

function pretty(value: unknown, limit = 4000): string {
  if (value === undefined || value === null) return ''
  const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2)
  return text.length > limit ? `${text.slice(0, limit)}\n…` : text
}

function partsOf(id: string): ChatPart[] {
  return byId.value.get(id)?.parts ?? []
}

function isStreamingText(id: string, index: number) {
  const parts = partsOf(id)
  return status.value === 'streaming' && messages.value[messages.value.length - 1]?.id === id && index === parts.length - 1
}

function onSubmit() {
  const text = input.value.trim()
  if (!text || busy.value) return
  input.value = ''
  void send(text)
}

function useExample(text: string) {
  input.value = text
  nextTick(() => {
    const el = document.querySelector<HTMLTextAreaElement>('[data-chat-panel-prompt] textarea')
    el?.focus()
  })
}

function applyDraft(part: DraftPart) {
  emit('draft', part.markdown, part.summary)
}

function onNewConversation() {
  input.value = ''
  reset()
}
</script>

<template>
  <section class="flex h-full min-h-0 flex-col" aria-label="AI アシスタント">
    <header class="flex shrink-0 items-center gap-2 border-b border-default px-3 py-2">
      <UIcon name="i-lucide-sparkles" class="size-4 text-primary" aria-hidden="true" />
      <h2 class="text-sm font-semibold text-highlighted">
        AI アシスタント
      </h2>
      <UBadge v-if="providerLabel" :label="providerLabel" color="neutral" variant="subtle" size="sm" />
      <div class="ms-auto flex items-center gap-1">
        <UTooltip text="新しい会話">
          <UButton
            icon="i-lucide-message-square-plus"
            label="新しい会話"
            color="neutral"
            variant="ghost"
            size="xs"
            @click="onNewConversation"
          />
        </UTooltip>
      </div>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto" data-chat-panel-scroll>
      <UAlert
        v-if="missingKey"
        class="m-3 w-auto"
        color="warning"
        variant="subtle"
        icon="i-lucide-key-round"
        title="Anthropic API キーが未設定です"
        description=".env の NUXT_ANTHROPIC_API_KEY を設定するか、設定画面でチャットエンジンを Google CLI（Antigravity / Gemini）に切り替えてください。"
      />

      <!-- loading -->
      <div v-if="loading && !messages.length" class="space-y-3 p-4" aria-busy="true" aria-label="会話履歴を読み込み中">
        <USkeleton class="ms-auto h-8 w-2/3" />
        <USkeleton class="h-4 w-full" />
        <USkeleton class="h-4 w-5/6" />
        <USkeleton class="h-4 w-1/2" />
      </div>

      <!-- empty -->
      <div v-else-if="!messages.length" class="flex flex-col gap-4 p-4">
        <div class="space-y-1">
          <p class="text-sm font-medium text-highlighted">
            一次資料をもとにレコードの下書きを作ります
          </p>
          <p class="text-xs text-muted">
            AI はまず一次資料を検索し、足りない部分だけ Web で調べます（Web 由来の情報は「未確認」と表示されます）。
            下書きはエディタに反映されますが、保存はあなたが行います。
          </p>
        </div>
        <ul class="flex flex-col items-start gap-2" aria-label="質問の例">
          <li v-for="ex in examples" :key="ex" class="max-w-full">
            <UButton
              :label="ex"
              color="neutral"
              variant="soft"
              size="sm"
              icon="i-lucide-corner-down-right"
              class="max-w-full text-start whitespace-normal"
              @click="useExample(ex)"
            />
          </li>
        </ul>
      </div>

      <UChatMessages
        v-else
        :messages="uiMessages"
        :status="status"
        should-auto-scroll
        :assistant="{ side: 'left', variant: 'naked' }"
        :user="{ side: 'right', variant: 'soft' }"
        class="py-3"
      >
        <template #content="{ message }">
          <template v-for="(part, i) in partsOf(message.id)" :key="`${message.id}-${i}`">
            <!-- text -->
            <template v-if="part.type === 'text'">
              <MarkdownView
                v-if="message.role === 'assistant'"
                :source="part.text"
                :strip-frontmatter="false"
                empty-text=""
                class="chat-markdown text-sm"
                :aria-busy="isStreamingText(message.id, i) || undefined"
              />
              <p v-else class="text-sm whitespace-pre-wrap">
                {{ part.text }}
              </p>
            </template>

            <!-- tool call -->
            <details
              v-else-if="part.type === 'tool'"
              class="group my-1.5 rounded-md border border-default bg-elevated/50 text-xs"
            >
              <summary
                class="flex cursor-pointer list-none items-center gap-2 rounded-md px-2.5 py-1.5 text-muted focus-visible:outline-2 focus-visible:outline-primary"
              >
                <UIcon
                  :name="(part as ToolPart).state === 'running' ? 'i-lucide-loader-circle' : (part as ToolPart).isError ? 'i-lucide-circle-x' : 'i-lucide-wrench'"
                  :class="['size-3.5 shrink-0', (part as ToolPart).state === 'running' && 'animate-spin', (part as ToolPart).isError && 'text-error']"
                  aria-hidden="true"
                />
                <span class="font-medium text-default">{{ toolLabel(part.name) }}</span>
                <span class="min-w-0 truncate font-mono">{{ preview(part.input, 80) }}</span>
                <span class="sr-only">{{ (part as ToolPart).state === 'running' ? '実行中' : (part as ToolPart).isError ? 'エラー' : '完了' }}</span>
                <UIcon name="i-lucide-chevron-down" class="ms-auto size-3.5 shrink-0 transition-transform group-open:rotate-180" aria-hidden="true" />
              </summary>
              <div class="space-y-2 border-t border-default px-2.5 py-2">
                <div>
                  <p class="mb-1 font-medium text-muted">
                    入力（{{ part.name }}）
                  </p>
                  <pre class="max-h-40 overflow-auto rounded bg-default p-2 font-mono whitespace-pre-wrap text-default">{{ pretty(part.input, 2000) }}</pre>
                </div>
                <div v-if="(part as ToolPart).state === 'done'">
                  <p class="mb-1 font-medium" :class="(part as ToolPart).isError ? 'text-error' : 'text-muted'">
                    {{ (part as ToolPart).isError ? 'エラー' : '結果' }}
                  </p>
                  <pre class="max-h-60 overflow-auto rounded bg-default p-2 font-mono whitespace-pre-wrap text-default">{{ pretty((part as ToolPart).output) }}</pre>
                </div>
                <p v-else class="text-muted">
                  実行中…
                </p>
              </div>
            </details>

            <!-- draft update -->
            <div
              v-else-if="part.type === 'draft'"
              class="my-2 flex items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-sm"
            >
              <UIcon name="i-lucide-file-pen-line" class="size-4 shrink-0 text-primary" aria-hidden="true" />
              <p class="min-w-0 flex-1 text-default">
                <span class="font-medium">ドラフトを更新しました</span><span v-if="(part as DraftPart).summary" class="text-muted">: {{ (part as DraftPart).summary }}</span>
              </p>
              <UButton
                label="適用"
                icon="i-lucide-check"
                size="xs"
                color="primary"
                variant="soft"
                :aria-label="`この下書きをエディタに適用${(part as DraftPart).summary ? `: ${(part as DraftPart).summary}` : ''}`"
                @click="applyDraft(part as DraftPart)"
              />
            </div>
          </template>
        </template>
      </UChatMessages>

      <UAlert
        v-if="status === 'error' && error"
        class="mx-3 mb-3 w-auto"
        color="error"
        variant="subtle"
        icon="i-lucide-circle-alert"
        title="エラーが発生しました"
        :description="error"
        :actions="[{ label: '再試行', icon: 'i-lucide-rotate-ccw', color: 'error', variant: 'outline', size: 'xs', onClick: () => { void retry() } }]"
      />
      <UAlert
        v-else-if="error && !messages.length && !loading"
        class="mx-3 mb-3 w-auto"
        color="error"
        variant="subtle"
        icon="i-lucide-circle-alert"
        :title="error"
      />
    </div>

    <div class="shrink-0 border-t border-default p-3" data-chat-panel-prompt>
      <UChatPrompt
        v-model="input"
        placeholder="AI に依頼（Enter で送信、Shift+Enter で改行）"
        :autofocus="false"
        :maxrows="8"
        variant="subtle"
        aria-label="AI への依頼"
        @submit="onSubmit"
      >
        <UChatPromptSubmit
          :status="status"
          aria-label="送信"
          @stop="stop"
          @reload="retry"
        />
      </UChatPrompt>
    </div>
  </section>
</template>

<style scoped>
/* compact prose inside chat bubbles (.phra-prose defaults to the 1rem article size) */
.chat-markdown {
  font-size: 0.875rem;
  line-height: 1.65;
}
.chat-markdown :deep(> :first-child) {
  margin-top: 0;
}
.chat-markdown :deep(> :last-child) {
  margin-bottom: 0;
}
</style>

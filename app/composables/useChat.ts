import type { MaybeRefOrGetter, Ref } from 'vue'
import type { ChatMessage, ChatPart, ChatSession, ChatStreamEvent, EntityKind } from '#shared/types'

export type ChatPanelStatus = 'ready' | 'submitted' | 'streaming' | 'error'

/* ------------------------------------------------------------------------------------------------
 * Pure helpers (no Vue / Nuxt dependencies — unit-testable in plain Node)
 * ---------------------------------------------------------------------------------------------- */

/**
 * Incremental Server-Sent Events parser. Feed decoded text chunks; returns the `data` payloads of every
 * complete event (frames are separated by a blank line; multi-line data is joined with "\n").
 */
export function createSseParser() {
  let buffer = ''
  function frames(text: string): string[] {
    const out: string[] = []
    for (const frame of text.split(/\r?\n\r?\n/)) {
      const data: string[] = []
      for (const line of frame.split(/\r?\n/)) {
        if (line.startsWith('data:')) data.push(line.slice(line.charAt(5) === ' ' ? 6 : 5))
      }
      if (data.length) out.push(data.join('\n'))
    }
    return out
  }
  return {
    push(chunk: string): string[] {
      buffer += chunk
      const idx = Math.max(buffer.lastIndexOf('\n\n'), buffer.lastIndexOf('\r\n\r\n'))
      if (idx < 0) return []
      const complete = buffer.slice(0, idx)
      buffer = buffer.slice(idx).replace(/^(\r?\n)+/, '')
      return frames(complete)
    },
    /** Flush whatever is left when the stream ends (a final frame without trailing blank line). */
    flush(): string[] {
      const rest = buffer
      buffer = ''
      return rest.trim() ? frames(rest) : []
    },
  }
}

/**
 * Apply one stream event to the (in-progress) assistant message. Mutates `message.parts` in place.
 * Returns what the caller has to react to.
 */
export function applyChatStreamEvent(message: ChatMessage, ev: ChatStreamEvent): { draft?: { markdown: string; summary?: string }; final?: ChatMessage; error?: string } {
  switch (ev.type) {
    case 'text-delta': {
      const last = message.parts[message.parts.length - 1]
      if (last?.type === 'text') last.text += ev.delta
      else message.parts.push({ type: 'text', text: ev.delta })
      return {}
    }
    case 'tool-start':
      message.parts.push({ type: 'tool', toolCallId: ev.toolCallId, name: ev.name, input: ev.input, state: 'running' })
      return {}
    case 'tool-result': {
      const part = message.parts.find(p => p.type === 'tool' && p.toolCallId === ev.toolCallId) as Extract<ChatPart, { type: 'tool' }> | undefined
      if (part) {
        part.output = ev.output
        part.isError = ev.isError
        part.state = 'done'
      } else {
        message.parts.push({ type: 'tool', toolCallId: ev.toolCallId, name: ev.name, input: undefined, output: ev.output, isError: ev.isError, state: 'done' })
      }
      return {}
    }
    case 'draft':
      message.parts.push({ type: 'draft', markdown: ev.markdown, summary: ev.summary })
      return { draft: { markdown: ev.markdown, summary: ev.summary } }
    case 'done':
      return { final: ev.message }
    case 'error':
      return { error: ev.message }
    default:
      return {}
  }
}

/** Minimal UIMessage-compatible shape consumed by <UChatMessages> (Vercel AI SDK `UIMessage`). */
export interface ChatUiMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  parts: Array<
    | { type: 'text'; text: string; state?: 'streaming' | 'done' }
    | { type: `data-${string}`; id?: string; data: unknown }
  >
}

/**
 * Convert our ChatMessage to the UIMessage shape. Text stays text; tool and draft parts become `data-tool` /
 * `data-draft` parts (rendered by our own #content slot). Messages without parts keep `parts: []` so that
 * UChatMessages shows its loading indicator for the pending assistant message.
 */
export function chatToUiMessages(messages: ChatMessage[], opts: { streamingId?: string } = {}): ChatUiMessage[] {
  return messages.map(m => ({
    id: m.id,
    role: m.role,
    parts: m.parts.map((p, i) => {
      if (p.type === 'text') {
        const streaming = opts.streamingId === m.id && i === m.parts.length - 1
        return { type: 'text' as const, text: p.text, state: streaming ? 'streaming' as const : 'done' as const }
      }
      if (p.type === 'tool') return { type: 'data-tool' as const, id: p.toolCallId, data: p }
      return { type: 'data-draft' as const, id: `${m.id}-draft-${i}`, data: p }
    }),
  }))
}

/* ------------------------------------------------------------------------------------------------
 * Composable
 * ---------------------------------------------------------------------------------------------- */

export interface UseChatOptions {
  kind: MaybeRefOrGetter<EntityKind>
  slug: MaybeRefOrGetter<string | null>
  /** Current editor content, sent with every message */
  draft: Ref<string> | (() => string)
  /** Called when the AI updates the draft (update_draft / Gemini draft block) */
  onDraft?: (markdown: string, summary?: string) => void
  /** Load the latest conversation on mount (default true) */
  autoLoad?: boolean
}

function localId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
}

function draftStorageKey(kind: EntityKind) {
  return `phrapedia:chat:${kind}:draft-session`
}

function readStored(key: string): string | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage.getItem(key)
  } catch {
    return null
  }
}

function writeStored(key: string, value: string | null) {
  try {
    if (typeof localStorage === 'undefined') return
    if (value) localStorage.setItem(key, value)
    else localStorage.removeItem(key)
  } catch {
    // storage unavailable — the conversation still works, it just is not restored after reload
  }
}

async function errorMessageFromResponse(res: Response): Promise<string> {
  try {
    const data = await res.json() as { message?: string; statusMessage?: string }
    return data.message || data.statusMessage || `HTTP ${res.status}`
  } catch {
    return `HTTP ${res.status} ${res.statusText}`
  }
}

export function useChat(options: UseChatOptions) {
  const messages = ref<ChatMessage[]>([])
  const status = ref<ChatPanelStatus>('ready')
  const sessionId = ref<string | null>(null)
  const error = ref<string | null>(null)
  /** true while the stored conversation is being fetched */
  const loading = ref(false)
  const streamingId = ref<string | undefined>()
  const lastUserText = ref<string | null>(null)
  const toast = useToast()
  let controller: AbortController | null = null
  let loadToken = 0

  const draftValue = () => (typeof options.draft === 'function' ? options.draft() : options.draft.value) ?? ''

  function rememberSession(id: string | null) {
    sessionId.value = id
    if (toValue(options.slug) === null) writeStored(draftStorageKey(toValue(options.kind)), id)
  }

  function finishRunningTools(msg: ChatMessage | undefined) {
    for (const p of msg?.parts ?? []) {
      if (p.type === 'tool' && p.state === 'running') {
        p.state = 'done'
        p.isError = true
        p.output = p.output ?? '（中断されました）'
      }
    }
  }

  function fail(message: string) {
    error.value = message
    status.value = 'error'
    toast.add({ title: 'AI アシスタントのエラー', description: message, color: 'error', icon: 'i-lucide-circle-alert' })
  }

  async function send(text: string) {
    const content = text.trim()
    if (!content || status.value === 'submitted' || status.value === 'streaming') return
    error.value = null
    lastUserText.value = content

    const userMsg: ChatMessage = { id: localId(), role: 'user', parts: [{ type: 'text', text: content }], createdAt: new Date().toISOString() }
    const assistantMsg: ChatMessage = { id: localId(), role: 'assistant', parts: [], createdAt: new Date().toISOString() }
    messages.value.push(userMsg, assistantMsg)
    // use the reactive proxies from here on so that in-place mutations re-render
    const assistant = messages.value[messages.value.length - 1]!
    const user = messages.value[messages.value.length - 2]!
    status.value = 'submitted'
    streamingId.value = assistant.id

    controller = new AbortController()
    const signal = controller.signal
    let gotTerminal = false
    try {
      const res = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'content-type': 'application/json', 'accept': 'text/event-stream' },
        body: JSON.stringify({
          sessionId: sessionId.value ?? undefined,
          kind: toValue(options.kind),
          slug: toValue(options.slug),
          message: content,
          draft: draftValue(),
        }),
        signal,
      })
      if (!res.ok || !res.body) throw new Error(await errorMessageFromResponse(res))

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      const parser = createSseParser()
      const handle = (payload: string) => {
        let ev: ChatStreamEvent
        try {
          ev = JSON.parse(payload) as ChatStreamEvent
        } catch {
          return
        }
        if (ev.type === 'session') {
          rememberSession(ev.sessionId)
          user.id = ev.userMessageId
          assistant.id = ev.assistantMessageId
          streamingId.value = ev.assistantMessageId
          return
        }
        if (ev.type === 'text-delta' || ev.type === 'tool-start' || ev.type === 'draft') {
          if (status.value === 'submitted') status.value = 'streaming'
        }
        const r = applyChatStreamEvent(assistant, ev)
        if (r.draft) options.onDraft?.(r.draft.markdown, r.draft.summary)
        if (r.error) {
          gotTerminal = true
          fail(r.error)
        }
        if (r.final) {
          gotTerminal = true
          if (r.final.parts.length) assistant.parts = r.final.parts
          assistant.id = r.final.id
          if (status.value !== 'error') status.value = 'ready'
        }
      }
      while (true) {
        const { value, done } = await reader.read()
        if (done) break
        for (const payload of parser.push(decoder.decode(value, { stream: true }))) handle(payload)
      }
      for (const payload of parser.flush()) handle(payload)
      if (!gotTerminal && (status.value as ChatPanelStatus) !== 'error') status.value = 'ready'
    } catch (err) {
      if (signal.aborted) {
        finishRunningTools(assistant)
        if (!assistant.parts.length) assistant.parts.push({ type: 'text', text: '（停止しました）' })
        status.value = 'ready'
      } else {
        finishRunningTools(assistant)
        fail((err as Error)?.message || 'AI アシスタントとの通信に失敗しました。')
      }
    } finally {
      controller = null
      streamingId.value = undefined
      // drop an empty assistant bubble (e.g. request rejected before streaming)
      if (!assistant.parts.length) messages.value = messages.value.filter(m => m !== assistant)
    }
  }

  function stop() {
    controller?.abort()
  }

  /** Re-send the last user message (after an error). */
  async function retry() {
    const text = lastUserText.value
    if (!text) return
    // remove the failed exchange so it is not shown twice
    const lastUser = [...messages.value].reverse().find(m => m.role === 'user')
    if (lastUser) {
      const idx = messages.value.indexOf(lastUser)
      messages.value = messages.value.slice(0, idx)
    }
    status.value = 'ready'
    await send(text)
  }

  function reset() {
    controller?.abort()
    loadToken++
    messages.value = []
    error.value = null
    status.value = 'ready'
    lastUserText.value = null
    rememberSession(null)
  }

  async function loadSession(id: string, token: number) {
    const data = await $fetch<{ session: ChatSession; messages: ChatMessage[] }>(`/api/chat/sessions/${encodeURIComponent(id)}`)
    if (token !== loadToken) return
    sessionId.value = data.session.id
    messages.value = data.messages
  }

  /** Load the newest conversation of the current record (or the remembered draft conversation). */
  async function loadLatestSession() {
    const token = ++loadToken
    const kind = toValue(options.kind)
    const slug = toValue(options.slug)
    loading.value = true
    error.value = null
    try {
      if (slug === null) {
        const stored = readStored(draftStorageKey(kind))
        if (stored) {
          try {
            await loadSession(stored, token)
          } catch {
            writeStored(draftStorageKey(kind), null)
          }
        }
        return
      }
      const sessions = await $fetch<ChatSession[]>('/api/chat/sessions', { query: { kind, slug } })
      if (token !== loadToken) return
      const newest = sessions[0]
      if (newest) await loadSession(newest.id, token)
      else {
        messages.value = []
        sessionId.value = null
      }
    } catch (err) {
      if (token !== loadToken) return
      const e = err as { data?: { message?: string }; message?: string }
      error.value = `会話履歴を読み込めませんでした: ${e?.data?.message || e?.message || ''}`
    } finally {
      if (token === loadToken) loading.value = false
    }
  }

  // Switch conversations when the record changes. A draft that just got saved (null → slug) keeps its
  // conversation, which is re-bound to the new record on the server.
  watch(
    () => [toValue(options.kind), toValue(options.slug)] as const,
    async ([kind, slug], [prevKind, prevSlug]) => {
      if (kind === prevKind && prevSlug === null && slug && sessionId.value) {
        const id = sessionId.value
        writeStored(draftStorageKey(kind), null)
        await $fetch(`/api/chat/sessions/${encodeURIComponent(id)}`, { method: 'PATCH', body: { slug } }).catch(() => {})
        return
      }
      controller?.abort()
      messages.value = []
      sessionId.value = null
      status.value = 'ready'
      await loadLatestSession()
    },
  )

  if (options.autoLoad !== false && getCurrentInstance()) {
    onMounted(() => {
      void loadLatestSession()
    })
    onBeforeUnmount(() => controller?.abort())
  }

  const uiMessages = computed(() => chatToUiMessages(messages.value, { streamingId: streamingId.value }))

  return { messages, uiMessages, status, sessionId, error, loading, send, stop, retry, reset, loadLatestSession }
}

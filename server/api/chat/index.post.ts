import type { ChatRequest, ChatStreamEvent } from '../../../shared/types/chat'
import { prepareChatTurn, runChatTurn } from '../../utils/agent'

/**
 * POST /api/chat — one chat turn streamed as Server-Sent Events (`data: <ChatStreamEvent JSON>`).
 * First event: {type:'session'}, last: {type:'done'} (or {type:'error'}).
 */
export default defineEventHandler(async (event) => {
  const body = await readBody<Partial<ChatRequest>>(event)
  if (!body || typeof body !== 'object') throw createError({ statusCode: 400, message: 'request body required' })
  const kind = assertKind(body.kind)
  const slug = body.slug ? assertSlug(body.slug) : null
  const message = typeof body.message === 'string' ? body.message.trim() : ''
  if (!message) throw createError({ statusCode: 400, message: 'message is required' })
  if (body.draft !== undefined && typeof body.draft !== 'string') throw createError({ statusCode: 400, message: 'draft must be a string' })
  const request: ChatRequest = {
    sessionId: typeof body.sessionId === 'string' && body.sessionId ? body.sessionId : undefined,
    kind,
    slug,
    message,
    draft: body.draft ?? '',
  }

  // autoclose is disabled: for POST requests the IncomingMessage 'close' event fires once the body has been read,
  // which would end the stream immediately. The response 'close' event is the reliable "client went away" signal.
  const stream = createEventStream(event, { autoclose: false })
  const controller = new AbortController()
  let closed = false
  const closeStream = async () => {
    if (closed) return
    closed = true
    await stream.close().catch(() => {})
  }
  event.node.res.on('close', () => {
    if (!event.node.res.writableFinished) controller.abort()
    void closeStream()
  })
  stream.onClosed(() => {
    closed = true
    controller.abort()
  })

  const send = (ev: ChatStreamEvent) => {
    if (closed) return
    void stream.push(JSON.stringify(ev)).catch(() => {})
  }

  ;(async () => {
    try {
      const prepared = await prepareChatTurn(request)
      send({ type: 'session', sessionId: prepared.session.id, userMessageId: prepared.userMessage.id, assistantMessageId: prepared.assistantId })
      const result = await runChatTurn(request, send, controller.signal, prepared)
      // On provider errors the {type:'error'} event was already sent; still deliver the stored message.
      send({ type: 'done', message: result.assistantMessage })
    } catch (err) {
      const e = err as { message?: string; statusMessage?: string; data?: { message?: string } }
      send({ type: 'error', message: e?.message || e?.statusMessage || 'チャットの処理中にエラーが発生しました。' })
    } finally {
      await closeStream()
    }
  })()

  return stream.send()
})

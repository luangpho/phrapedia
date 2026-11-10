/**
 * Thin API layer: every backend call of the app goes through here (Tauri `invoke` / `Channel`).
 * The contract (command names, arguments, AppError) is docs/NATIVE_API.md. No Nuxt auto-imports are used so the
 * module is unit-testable in plain Node. Importing it never throws; outside the Tauri WebView (plain browser)
 * every call rejects with a readable Japanese message.
 */
import type {
  AppSettings,
  ChatMessage,
  ChatRequest,
  ChatSession,
  ChatStreamEvent,
  EntityKind,
  EntityRecord,
  EntitySummary,
  SettingsView,
  SourceContent,
  SourceFile,
  SourceSearchHit,
  SyncResult,
  SyncStatus,
} from '#shared/types'

/* ------------------------------------------------------------------------------------------------
 * Errors
 * ---------------------------------------------------------------------------------------------- */

export type AppErrorCode =
  | 'invalid_argument' | 'not_found' | 'conflict' | 'path_unusable' | 'invalid_frontmatter' | 'io' | 'git'
  | 'cli_not_found' | 'cli_failed' | 'timeout' | 'aborted' | 'internal'
  /** Frontend only: the app runs in a plain browser (no Tauri) */
  | 'desktop_only'

/** The JSON every failing command rejects with (docs/NATIVE_API.md §1). */
export interface AppError {
  code: AppErrorCode
  /** Japanese; shown to the user as is */
  message: string
  detail?: string
}

export const DESKTOP_ONLY_MESSAGE = 'この機能はデスクトップアプリ専用です。Phrapedia のデスクトップアプリから開いてください。'

export function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null
    && typeof (e as { code?: unknown }).code === 'string'
    && typeof (e as { message?: unknown }).message === 'string'
}

export function appErrorMessage(e: unknown, fallback = 'エラーが発生しました'): string {
  if (isAppError(e)) return e.message || fallback
  if (e instanceof Error) return e.message || fallback
  if (typeof e === 'string') return e || fallback
  return fallback
}

const STATUS_BY_CODE: Partial<Record<AppErrorCode, number>> = {
  invalid_argument: 400,
  not_found: 404,
  conflict: 409,
  path_unusable: 409,
}

/** Error thrown by the API layer. Carries the AppError fields plus an HTTP-like `statusCode` (404 = not_found). */
export class ApiError extends Error implements AppError {
  code: AppErrorCode
  detail?: string
  statusCode?: number
  constructor(e: AppError) {
    super(e.message)
    this.name = 'ApiError'
    this.code = e.code
    this.detail = e.detail
    this.statusCode = STATUS_BY_CODE[e.code]
  }
}

/** Convert whatever `invoke` rejected with (AppError JSON, string, Error) into an ApiError. */
export function toApiError(e: unknown): ApiError {
  if (e instanceof ApiError) return e
  if (isAppError(e)) return new ApiError(e)
  if (e instanceof Error) return new ApiError({ code: 'internal', message: e.message })
  return new ApiError({ code: 'internal', message: typeof e === 'string' && e ? e : '不明なエラーが発生しました' })
}

/* ------------------------------------------------------------------------------------------------
 * Transport
 * ---------------------------------------------------------------------------------------------- */

/** True inside the Tauri WebView. */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function desktopOnly(): ApiError {
  return new ApiError({ code: 'desktop_only', message: DESKTOP_ONLY_MESSAGE })
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) throw desktopOnly()
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    return await invoke<T>(cmd, args)
  } catch (e) {
    throw toApiError(e)
  }
}

/* ------------------------------------------------------------------------------------------------
 * Types of the non-shared commands
 * ---------------------------------------------------------------------------------------------- */

export interface PathCheckView { ok: boolean; error: string | null }

export interface AppInfo {
  version: string
  settingsWarning: string | null
  datasetPath: string
  exportPath: string
  datasetCheck: PathCheckView
  exportCheck: PathCheckView
  stateDir: string
  /** Absolute path of the app binary (empty when unknown) */
  exePath: string
  configDir: string
  dataDir: string
}

export interface PathSettingsView {
  datasetPath: string
  /** Empty = same as the dataset path */
  exportPath: string
  effectiveExport: string
  datasetCheck: PathCheckView
  exportCheck: PathCheckView
  settingsFile: string
}

export interface SavePathsOutcome {
  ok: boolean
  datasetError: string | null
  exportError: string | null
  settings: PathSettingsView
}

export interface SaveEntityResult { record: EntityRecord; commit: SyncResult | null }
export interface DeleteEntityResult { ok: true; commit: SyncResult | null }
export interface ResearchResult { text: string; engine?: string; model?: string; cached: boolean; durationMs: number; stderr?: string }
export interface ResearchParams { query: string; context?: string; images?: string[]; ocrText?: string; noCache?: boolean }
export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] }

/* ------------------------------------------------------------------------------------------------
 * Chat
 * ---------------------------------------------------------------------------------------------- */

export interface ChatStreamHandle {
  /** Resolves when the turn is over (after `done` / `error` was delivered). Rejects on invalid input. */
  done: Promise<void>
  /** Detach the handler: events that still arrive are ignored (used when the turn is abandoned). */
  detach: () => void
}

/** Start a chat turn. `onEvent` receives every ChatStreamEvent in order. */
export function chatSend(request: ChatRequest, onEvent: (ev: ChatStreamEvent) => void): ChatStreamHandle {
  let handler: ((ev: ChatStreamEvent) => void) | null = onEvent
  const done = (async () => {
    if (!isTauriRuntime()) throw desktopOnly()
    try {
      const { Channel, invoke } = await import('@tauri-apps/api/core')
      const ch = new Channel<ChatStreamEvent>()
      ch.onmessage = (ev) => { handler?.(ev) }
      await invoke<void>('chat_send', { request, onEvent: ch })
    } catch (e) {
      throw toApiError(e)
    }
  })()
  return { done, detach: () => { handler = null } }
}

/* ------------------------------------------------------------------------------------------------
 * Source files (binary)
 * ---------------------------------------------------------------------------------------------- */

const MIME_BY_EXT: Record<string, string> = {
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', pdf: 'application/pdf',
}

export function mimeFromPath(path: string): string {
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase()
  return MIME_BY_EXT[ext] ?? 'application/octet-stream'
}

function toBlob(data: ArrayBuffer | ArrayLike<number>, type: string): Blob {
  return new Blob([data instanceof ArrayBuffer ? data : new Uint8Array(Array.from(data))], { type })
}

const blobUrlCache = new Map<string, Promise<string>>()

/** Blob URL of a primary-source file (cached per path for the app lifetime). */
export function sourceBlobUrl(path: string): Promise<string> {
  let p = blobUrlCache.get(path)
  if (!p) {
    p = call<ArrayBuffer | number[]>('read_source_bytes', { path })
      .then(buf => URL.createObjectURL(toBlob(buf, mimeFromPath(path))))
    // a failed read must not be cached forever
    p.catch(() => blobUrlCache.delete(path))
    blobUrlCache.set(path, p)
  }
  return p
}

/** Drop cached blob URLs (e.g. after the dataset path changed). */
export function clearSourceBlobCache(): void {
  for (const p of blobUrlCache.values()) p.then(u => URL.revokeObjectURL(u)).catch(() => {})
  blobUrlCache.clear()
}

/* ------------------------------------------------------------------------------------------------
 * Commands
 * ---------------------------------------------------------------------------------------------- */

export const api = {
  // app / paths
  appInfo: () => call<AppInfo>('app_info'),
  updaterEnabled: () => call<boolean>('updater_enabled'),
  getPathSettings: () => call<PathSettingsView>('get_path_settings'),
  savePathSettings: (datasetPath: string, exportPath: string) => call<SavePathsOutcome>('save_path_settings', { datasetPath, exportPath }),

  // settings
  getSettings: () => call<SettingsView>('get_settings'),
  updateSettings: (patch: DeepPartial<AppSettings>) => call<AppSettings>('update_settings', { patch }),
  reindexSources: () => call<{ indexed: number; removed: number; total: number }>('reindex_sources'),

  // entities
  listEntities: (kind: EntityKind) => call<EntitySummary[]>('list_entities', { kind }),
  entityTemplate: (kind: EntityKind, name?: string) => call<string>('entity_template', { kind, name: name || undefined }),
  createEntity: (kind: EntityKind, raw: string, message?: string) => call<SaveEntityResult>('create_entity', { kind, raw, message }),
  getEntity: (kind: EntityKind, slug: string) => call<EntityRecord>('get_entity', { kind, slug }),
  updateEntity: (kind: EntityKind, slug: string, raw: string, message?: string) => call<SaveEntityResult>('update_entity', { kind, slug, raw, message }),
  deleteEntity: (kind: EntityKind, slug: string) => call<DeleteEntityResult>('delete_entity', { kind, slug }),

  // sources
  listSources: (dir?: string) => call<SourceFile[]>('list_sources', { dir }),
  searchSources: (q: string, limit?: number) => call<SourceSearchHit[]>('search_sources', { q, limit }),
  readSource: (path: string, maxChars?: number) => call<SourceContent>('read_source', { path, maxChars }),

  // research
  webResearch: (params: ResearchParams) => call<ResearchResult>('web_research', { ...params }),

  // git sync
  syncStatus: () => call<SyncStatus>('sync_status'),
  syncRun: (action: SyncResult['action'], message?: string) => {
    switch (action) {
      case 'commit': return call<SyncResult>('commit_data', { message })
      case 'sync': return call<SyncResult>('sync_data', { message })
      case 'pull': return call<SyncResult>('pull_data')
      case 'push': return call<SyncResult>('push_data')
    }
  },

  // chat history
  listChatSessions: (kind: EntityKind, slug?: string | null) => call<ChatSession[]>('list_chat_sessions', { kind, slug: slug ?? null }),
  getChatSession: (id: string) => call<{ session: ChatSession; messages: ChatMessage[] }>('get_chat_session', { id }),
  deleteChatSession: (id: string) => call<{ ok: true }>('delete_chat_session', { id }),
  patchChatSession: (id: string, patch: { slug?: string | null; title?: string }) => call<ChatSession>('patch_chat_session', { id, patch }),
  deleteChatMessages: (id: string, ids: string[]) => call<{ ok: true; deleted: number }>('delete_chat_messages', { id, ids }),
  chatAbort: (turnId: string) => call<void>('chat_abort', { turnId }),
}

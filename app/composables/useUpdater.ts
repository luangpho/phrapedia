/**
 * Desktop (Tauri) only: in-app updater driven by the header update icon.
 *
 * Nothing is installed (and no dialog is shown) without a click: startup and a 6 h interval only run a silent
 * `check()`; a found update just turns the icon into "available" (badge + tooltip). The click confirms, downloads,
 * installs and relaunches. Tauri plugins are imported lazily, so a plain browser never evaluates them.
 */
import { ref, shallowRef } from 'vue'
import { isTauriRuntime } from '../utils/api'
import { confirmDialog } from '../utils/dialog'

export type UpdaterState =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'available'; version: string; notes: string }
  | { kind: 'downloading'; version: string; percent: number | null }
  | { kind: 'installing'; version: string }
  | { kind: 'error'; message: string }
  | { kind: 'upToDate' }

export interface UpdaterToast {
  title: string
  description?: string
  color: 'success' | 'error' | 'neutral'
}
export type Notify = (toast: UpdaterToast) => void

export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000
export const BREW_UPGRADE_COMMAND = 'brew upgrade --cask phrapedia'

/** EACCES / EPERM / read-only volume, or the plugin's own "Failed to move the new app into place" (macOS: the privileged retry was refused): /Applications/Phrapedia.app is owned by another user (Homebrew installed by someone else). */
const PERMISSION_RE = /permission denied|failed to move the new app into place|os error 13|os error 1\b|operation not permitted|not permitted|read-only|EACCES|EPERM/i

export function isPermissionError(message: string): boolean {
  return PERMISSION_RE.test(message)
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err)
}

interface PendingUpdate {
  downloadAndInstall: (cb: (ev: unknown) => void) => Promise<void>
}

// Module-level singleton: the layout button and the startup plugin share one state machine.
const state = shallowRef<UpdaterState>({ kind: 'idle' })
const enabled = ref(false)
const currentVersion = ref('')
let pendingUpdate: PendingUpdate | null = null
/** true while a check / click run (incl. the confirm dialog) is in flight, so repeated clicks cannot start a second run */
let busy = false
let started = false
let timer: ReturnType<typeof setInterval> | null = null

function isBusyState(): boolean {
  const k = state.value.kind
  return k === 'checking' || k === 'downloading' || k === 'installing'
}

async function runCheck(): Promise<{ found: boolean } | { error: string }> {
  state.value = { kind: 'checking' }
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (!update) {
      pendingUpdate = null
      return { found: false }
    }
    pendingUpdate = update as unknown as PendingUpdate
    state.value = { kind: 'available', version: update.version, notes: update.body ?? '' }
    return { found: true }
  } catch (err) {
    return { error: errText(err) }
  }
}

/** Silent check (startup / interval): never toasts, never opens a dialog. */
async function silentCheck(): Promise<void> {
  if (!enabled.value || busy || isBusyState()) return
  busy = true
  try {
    const keep = state.value
    const r = await runCheck()
    if ('error' in r) {
      console.info('[updater] silent check failed:', r.error)
      // keep an earlier 'available' (its Update object is still held); a transient failure must not drop the badge
      state.value = keep.kind === 'available' ? keep : { kind: 'idle' }
    } else if (!r.found) {
      state.value = { kind: 'idle' }
    }
  } finally {
    busy = false
  }
}

async function install(notify: Notify, version: string): Promise<void> {
  const update = pendingUpdate
  if (!update) return
  let downloaded = 0
  let total: number | null = null
  state.value = { kind: 'downloading', version, percent: null }
  try {
    await update.downloadAndInstall((raw) => {
      const ev = raw as { event: string; data?: { contentLength?: number; chunkLength?: number } }
      if (ev.event === 'Started') {
        total = ev.data?.contentLength ?? null
      } else if (ev.event === 'Progress') {
        downloaded += ev.data?.chunkLength ?? 0
        const percent = total && total > 0 ? Math.min(100, Math.floor((downloaded / total) * 100)) : null
        state.value = { kind: 'downloading', version, percent }
      } else if (ev.event === 'Finished') {
        state.value = { kind: 'installing', version }
      }
    })
    state.value = { kind: 'installing', version }
    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  } catch (err) {
    const message = errText(err)
    state.value = { kind: 'error', message }
    if (isPermissionError(message)) {
      notify({
        title: '権限不足のため更新できませんでした',
        description: `Homebrew を別のユーザーで入れた場合は、そのユーザーで「${BREW_UPGRADE_COMMAND}」を実行してください。（${message}）`,
        color: 'error',
      })
    } else {
      notify({ title: '更新に失敗しました', description: message, color: 'error' })
    }
  }
}

/** Header icon click. */
async function handleClick(notify: Notify): Promise<void> {
  if (!enabled.value || busy || isBusyState()) return
  busy = true
  try {
    if (state.value.kind !== 'available' || !pendingUpdate) {
      const r = await runCheck()
      if ('error' in r) {
        state.value = { kind: 'error', message: r.error }
        notify({ title: '更新の確認に失敗しました', description: r.error, color: 'error' })
        return
      }
      if (!r.found) {
        state.value = { kind: 'upToDate' }
        notify({ title: `最新です（現在 ${currentVersion.value || '不明'}）`, color: 'success' })
        return
      }
    }
    const cur = state.value
    if (cur.kind !== 'available') return
    const notes = cur.notes.trim()
    const message = `新しい版 ${cur.version}（現在 ${currentVersion.value || '不明'}）があります。\n${notes ? `\n${notes}\n` : ''}\n今すぐ更新して再起動しますか？`
    if (!(await confirmDialog(message, { title: 'Phrapedia の更新', okLabel: '更新して再起動', cancelLabel: 'あとで' }))) return
    await install(notify, cur.version)
  } finally {
    busy = false
  }
}

/** Startup hook (client plugin): decides whether the updater is usable, then starts the silent checks. */
async function init(): Promise<void> {
  if (started || !isTauriRuntime()) return
  started = true
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    enabled.value = (await invoke<boolean>('updater_enabled')) === true
    const { getVersion } = await import('@tauri-apps/api/app')
    currentVersion.value = await getVersion().catch(() => '')
  } catch (err) {
    console.info('[updater] disabled:', errText(err))
    enabled.value = false
  }
  if (!enabled.value) return
  void silentCheck()
  timer = setInterval(() => void silentCheck(), CHECK_INTERVAL_MS)
}

// HMR re-evaluates this module (resetting `timer` / `started`); stop the old interval so it does not leak.
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    if (timer) clearInterval(timer)
    timer = null
  })
}

export function useUpdater() {
  return { state, enabled, currentVersion, init, handleClick, silentCheck }
}

/** Test helper: reset the module singleton. */
export function __resetUpdaterForTests(): void {
  state.value = { kind: 'idle' }
  enabled.value = false
  currentVersion.value = ''
  pendingUpdate = null
  busy = false
  started = false
  if (timer) clearInterval(timer)
  timer = null
}

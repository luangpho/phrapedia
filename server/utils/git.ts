import { simpleGit } from 'simple-git'
import type { SimpleGit } from 'simple-git'
import type { SyncResult, SyncStatus } from '../../shared/types/sync'

/**
 * GitHub synchronisation. The dataset (data/) lives inside this repository, so "sync" means:
 * commit the data directory, pull --rebase from the remote branch, push.
 * Only the data directory is ever staged automatically.
 */
export function git(): SimpleGit {
  return simpleGit({ baseDir: projectRoot(), maxConcurrentProcesses: 1 })
}

/** Remote / branch names from settings are passed to git argv; refuse anything that could be read as an option. */
function safeName(value: string | undefined, what: string): string | undefined {
  if (!value) return undefined
  if (!isSafeGitName(value)) throw createError({ statusCode: 400, statusMessage: 'Invalid git name', message: `${what} に使えない文字が含まれています: ${value}` })
  return value
}

/** Drop user:password@ from a remote URL before it is shown in the UI. */
function redactUrl(url: string | undefined): string | undefined {
  if (!url) return url
  try {
    const u = new URL(url)
    if (u.username || u.password) {
      u.username = ''
      u.password = ''
    }
    return u.toString()
  } catch {
    return url.replace(/\/\/[^/@]+@/, '//')
  }
}

function isMissingRemoteRef(message: string): boolean {
  return /couldn't find remote ref|no such ref was fetched|Couldn't find remote ref/i.test(message)
}

async function dataRelDir(): Promise<string> {
  const abs = await dataDir()
  const root = projectRoot()
  if (!isInside(root, abs)) {
    throw createError({ statusCode: 400, statusMessage: 'Data directory outside repository', message: `${abs} is not inside ${root}; git sync only works when dataDir is inside the project repository.` })
  }
  const rel = abs.slice(root.length).replace(/^[\\/]+/, '')
  return toPosix(rel) || '.'
}

export async function syncStatus(): Promise<SyncStatus> {
  const g = git()
  try {
    const isRepo = await g.checkIsRepo()
    if (!isRepo) return { isRepo: false, ahead: 0, behind: 0, changes: [] }
    const settings = await getSettings()
    const rel = await dataRelDir()
    const status = await g.status(['--', rel])
    const remotes = await g.getRemotes(true)
    const remoteName = safeName(settings.git.remote, 'リモート名') || 'origin'
    const remote = remotes.find(r => r.name === remoteName) ?? remotes[0]
    const branch = safeName(settings.git.branch, 'ブランチ名') || status.current || undefined
    let ahead = status.ahead
    let behind = status.behind
    if (remote && branch) {
      try {
        const counts = await g.raw(['rev-list', '--left-right', '--count', `${branch}...${remote.name}/${branch}`])
        const [a, b] = counts.trim().split(/\s+/).map(Number)
        if (Number.isFinite(a) && Number.isFinite(b)) { ahead = a!; behind = b! }
      } catch {
        // remote branch may not exist yet
      }
    }
    let lastCommit: SyncStatus['lastCommit']
    try {
      const log = await g.log({ maxCount: 1 })
      if (log.latest) lastCommit = { hash: log.latest.hash.slice(0, 7), message: log.latest.message, date: log.latest.date, author: log.latest.author_name }
    } catch {
      // no commits yet
    }
    return {
      isRepo: true,
      branch,
      remote: remote?.name,
      remoteUrl: redactUrl(remote?.refs.fetch),
      ahead,
      behind,
      changes: status.files.map(f => ({ path: f.path, status: `${f.index}${f.working_dir}`.trim() || '??' })),
      lastCommit,
    }
  } catch (err) {
    return { isRepo: false, ahead: 0, behind: 0, changes: [], error: (err as Error).message }
  }
}

async function configureAuthor(g: SimpleGit): Promise<void> {
  const s = await getSettings()
  if (s.git.authorName) await g.addConfig('user.name', s.git.authorName, false, 'local')
  if (s.git.authorEmail) await g.addConfig('user.email', s.git.authorEmail, false, 'local')
}

/** Stage data/ and commit. Returns ok=false (not an error) when there is nothing to commit. */
export async function commitData(message?: string): Promise<SyncResult> {
  const g = git()
  const rel = await dataRelDir()
  await configureAuthor(g)
  await g.add([rel])
  const status = await g.status(['--', rel])
  const staged = status.files.filter(f => f.index && f.index !== ' ' && f.index !== '?')
  if (!staged.length) {
    return { ok: false, action: 'commit', message: 'コミットする変更はありません', status: await syncStatus() }
  }
  const msg = message?.trim() || `data: update ${staged.length} file(s) (${new Date().toISOString()})`
  const res = await g.commit(msg)
  return { ok: true, action: 'commit', message: `コミットしました: ${res.commit}`, commitHash: res.commit, status: await syncStatus() }
}

export async function pullData(): Promise<SyncResult> {
  const g = git()
  const s = await getSettings()
  const st = await syncStatus()
  if (!st.isRepo) return { ok: false, action: 'pull', message: 'Gitリポジトリではありません' }
  if (!st.remote) return { ok: false, action: 'pull', message: 'リモートが設定されていません', status: st }
  const branch = safeName(s.git.branch, 'ブランチ名') || st.branch
  if (!branch) return { ok: false, action: 'pull', message: 'ブランチを特定できません', status: st }
  try {
    // --autostash: files outside data/ may be modified (the app lives in the same repo); keep them out of the way.
    await g.pull(st.remote, branch, { '--rebase': 'true', '--autostash': null })
    return { ok: true, action: 'pull', message: `${st.remote}/${branch} から取得しました`, status: await syncStatus() }
  } catch (err) {
    const m = (err as Error).message
    if (isMissingRemoteRef(m)) {
      // First sync: the branch does not exist on the remote yet. Nothing to pull; push will create it.
      return { ok: true, action: 'pull', message: `リモートに ${branch} はまだありません（push で作成されます）`, status: st }
    }
    try { await g.rebase(['--abort']) } catch { /* no rebase in progress */ }
    return { ok: false, action: 'pull', message: `取得に失敗しました: ${m}`, status: await syncStatus() }
  }
}

export async function pushData(): Promise<SyncResult> {
  const g = git()
  const s = await getSettings()
  const st = await syncStatus()
  if (!st.isRepo) return { ok: false, action: 'push', message: 'Gitリポジトリではありません' }
  if (!st.remote) return { ok: false, action: 'push', message: 'リモートが設定されていません', status: st }
  const branch = safeName(s.git.branch, 'ブランチ名') || st.branch
  if (!branch) return { ok: false, action: 'push', message: 'ブランチを特定できません', status: st }
  try {
    await g.push(st.remote, branch, ['--set-upstream'])
    return { ok: true, action: 'push', message: `${st.remote}/${branch} へ送信しました`, status: await syncStatus() }
  } catch (err) {
    return { ok: false, action: 'push', message: `送信に失敗しました: ${(err as Error).message}`, status: await syncStatus() }
  }
}

/** commit → pull --rebase → push */
export async function syncData(message?: string): Promise<SyncResult> {
  const c = await commitData(message)
  const p = await pullData()
  if (!p.ok && p.status?.remote) return { ...p, action: 'sync' }
  const u = await pushData()
  const parts = [c.message, p.message, u.message].filter(Boolean)
  return { ok: u.ok, action: 'sync', message: parts.join(' / '), commitHash: c.commitHash, status: u.status }
}

/** Called after a record save when auto-commit is on. Never throws. */
export async function autoCommitIfEnabled(message: string): Promise<SyncResult | null> {
  try {
    const s = await getSettings()
    if (!s.git.autoCommit) return null
    const c = await commitData(message)
    if (c.ok && s.git.autoPush) return pushData()
    return c
  } catch (err) {
    console.error('[phrapedia] auto-commit failed:', err)
    return null
  }
}

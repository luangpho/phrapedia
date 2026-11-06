/**
 * Desktop (Tauri) sidecar safety net: when PHRAPEDIA_PARENT_PID is set, exit as soon as that process is gone
 * (e.g. the app was SIGKILLed and could not run its own cleanup). No effect in CLI / Render runs.
 */
export default defineNitroPlugin(() => {
  const pid = Number(process.env.PHRAPEDIA_PARENT_PID)
  if (!Number.isInteger(pid) || pid <= 1) return
  const timer = setInterval(() => {
    try {
      process.kill(pid, 0)
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ESRCH') process.exit(0)
    }
  }, 3000)
  timer.unref()
})

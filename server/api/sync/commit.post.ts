export default defineEventHandler(async (event) => {
  const body = await readBody<{ message?: string }>(event).catch(() => ({} as { message?: string }))
  return commitData(body?.message)
})

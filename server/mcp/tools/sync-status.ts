export default defineMcpTool({
  name: 'sync_status',
  title: 'Git sync status',
  description: 'Show the git status of the data directory: branch, remote, uncommitted record changes, ahead/behind counts.',
  annotations: { readOnlyHint: true },
  handler: async () => toToolJson(await syncStatus()),
})

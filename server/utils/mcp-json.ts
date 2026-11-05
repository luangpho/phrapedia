/** MCP tool handlers must return plain objects (Record<string, unknown>); this strips interface types and undefined values. */
export function toToolJson<T>(value: T): Record<string, unknown> {
  return JSON.parse(JSON.stringify(value ?? null)) as Record<string, unknown>
}

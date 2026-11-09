import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js'
import type { CallToolResult, Tool } from '@modelcontextprotocol/sdk/types.js'
import { getMcpTools, registerToolFromDefinition } from '@nuxtjs/mcp-toolkit/server'

/**
 * In-process MCP client over the very same tool definitions that the HTTP endpoint (/mcp) exposes.
 * The in-app chat agent uses this so that the AI in the browser and external MCP clients
 * (Claude Code, Gemini CLI, Cursor, ...) share one tool implementation.
 */
export interface McpBridge {
  /** JSON-schema tool descriptors (name, description, inputSchema) */
  tools: Tool[]
  callTool: (name: string, args: Record<string, unknown>) => Promise<CallToolResult>
  close: () => Promise<void>
}

export async function createMcpBridge(opts: { exclude?: string[] } = {}): Promise<McpBridge> {
  const server = new McpServer({ name: 'phrapedia-in-app', version: '0.1.0' })
  const defs = await getMcpTools()
  for (const def of defs) {
    if (opts.exclude?.includes(def.name ?? '')) continue
    registerToolFromDefinition(server, def)
  }
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair()
  await server.connect(serverTransport)
  const client = new Client({ name: 'phrapedia-chat-agent', version: '0.1.0' })
  await client.connect(clientTransport)
  const { tools } = await client.listTools()
  return {
    tools,
    callTool: async (name, args) => (await client.callTool({ name, arguments: args })) as CallToolResult,
    close: async () => {
      await client.close().catch(() => {})
      await server.close().catch(() => {})
    },
  }
}

/** Names of the MCP tools defined in server/mcp/tools (what /mcp exposes), minus `exclude`. */
export async function listMcpToolNames(opts: { exclude?: string[] } = {}): Promise<string[]> {
  const defs = await getMcpTools()
  return defs.map(d => d.name ?? '').filter(n => n && !opts.exclude?.includes(n))
}

/** Flatten a CallToolResult into plain text (and collect images) for LLM consumption / UI display. */
export function toolResultText(result: CallToolResult): { text: string; images: { data: string; mimeType: string }[]; isError: boolean } {
  const parts: string[] = []
  const images: { data: string; mimeType: string }[] = []
  for (const c of result.content ?? []) {
    if (c.type === 'text') parts.push(c.text)
    else if (c.type === 'image') images.push({ data: c.data, mimeType: c.mimeType })
    else parts.push(`[${c.type} content]`)
  }
  if (!parts.length && result.structuredContent) parts.push(JSON.stringify(result.structuredContent))
  return { text: parts.join('\n'), images, isError: Boolean(result.isError) }
}

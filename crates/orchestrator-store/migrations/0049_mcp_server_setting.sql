-- DPES-1105 phase 6b: the read-only MCP server for the orchestrator chat. OFF by default.
-- 0 = off, 1 = on. Only settings.set (user-only, approver gate) changes it.
ALTER TABLE orchestrator_settings ADD COLUMN mcp_server_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (mcp_server_enabled IN (0, 1));

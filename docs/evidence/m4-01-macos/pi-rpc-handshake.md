# M4-01 Pi RPC handshake evidence

Captured at `2026-11-08T06:10:51.387+00:00`.

- Pi version: `1.5.0`
- Executable: `/Users/mckerracher.joshua/.bun/bin/pi`
- Arguments: `--mode rpc --no-session --offline --no-extensions --no-mcp --no-skills --no-context-files`
- Input frame: `{"id":"m4-01-state","type":"get_state"}`
- Model prompt sent: no
- Exit status: `0`

Raw stdout frame:

```json
{"id":"m4-01-state","type":"response","command":"get_state","success":true,"data":{"model":{"id":"claude-opus-4-8","name":"Claude Opus 4.8","api":"anthropic-messages","provider":"anthropic","baseUrl":"https://api.anthropic.com","reasoning":true,"input":["text","image"],"cost":{"input":5,"output":25,"cacheRead":0.5,"cacheWrite":6.25},"contextWindow":1000000,"maxTokens":128000,"thinkingLevelMap":{"xhigh":"xhigh","max":"max"},"compat":{"forceAdaptiveThinking":true,"supportsTemperature":false,"supportsMidConvoSystemMessages":true,"supportsMidConvoToolChanges":true,"supportsStrictTools":true},"promptCache":{"short":300,"long":3600},"inputLimits":{"maxRequestBytes":33554432,"images":{"maxPerRequest":600,"resize":{"maxWidth":2000,"maxHeight":2000,"maxBytes":4718592,"jpegQuality":80}}},"type":"chat"},"thinkingLevel":"low","isStreaming":false,"isCompacting":false,"steeringMode":"one-at-a-time","followUpMode":"one-at-a-time","sessionId":"01a1b9c7-e881-75aa-8968-68dd4182ef8d","autoCompactionEnabled":true,"messageCount":0,"pendingMessageCount":0}}
```

Raw stderr was empty.

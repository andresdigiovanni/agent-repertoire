# Timeouts and long-running tools

Long-running tools are subject to two independent timeouts:

- **Client request timeout**: limits how long the caller waits for the MCP request. A timeout does not necessarily stop the server-side process, which may continue running and complete its work.
- **Server subprocess timeout**: `timeout_seconds` limits the tool's execution time and can be overridden per call.

Rules:

1. Before running network-bound or large-scale tools, check `timeout_seconds` and ensure the expected runtime fits within the client timeout.
2. For operations that may exceed the client timeout, increase the client timeout or execute the tool out-of-band.
3. After a client timeout, check for side effects before retrying, since the tool may have continued running.
4. When creating a tool, set `timeout_seconds` to cover its worst-case runtime. If runtime scales with input size, document the relevant cost in the tool description.

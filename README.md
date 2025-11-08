# miniagent

A minimal, ergonomic LLM agent in Rust inspired by [Mini-Agent](https://github.com/MiniMax-AI/Mini-Agent/), using:

- [siumai](https://github.com/YumchaLabs/siumai) (unified multi-provider LLM client)
- [rmcp](https://github.com/modelcontextprotocol/rust-sdk) (Rust MCP SDK) for loading external tools
- Async-first design, structured errors, logging, and token-aware summarization

## Features

- Multi-turn agent loop with tool calls (file IO, bash, skills, MCP)
- Token-aware context summarization (default tiktoken: cl100k_base)
- Configurable retry with exponential backoff (from config)
- Workspace-scoped execution; log per run at `~/.miniagent/log/`
- Claude Skills progressive disclosure: metadata in system prompt + `get_skill` on demand

## Quick Start

1) Install Rust (1.75+ recommended) and clone this repo.

2) Configure LLM
3) 
- First run auto-creates `~/.miniagent/config/` from bundled templates and exits with a hint to edit it.
- Or copy manually: `config/config-example.yaml` to either `~/.miniagent/config/config.yaml` (recommended) or `./config/config.yaml`.
- Edit your `api_key` and `provider`/`model`. For MiniMax, prefer `provider: minimaxi` (native). For OpenAI-compatible custom endpoints, set `base_url`.

1) Run
```bash
cargo run -- -w .
```

- Default tokenization uses tiktoken; to disable: `cargo run --no-default-features -- -w .`
- Use `/help` inside the REPL for available commands.

### Example Session

```shell
$ cargo run -- -w .

You > Load the internal-comms skill and summarize the guidelines

# The agent will call the tool get_skill(skill_name="internal-comms") automatically.
# You should see absolute paths rewritten in the returned content, like:
#   - `.../skills/internal-comms/examples/company-newsletter.md` (use read_file to access)

You > Read the company newsletter guideline and outline the sections
# The agent will likely call read_file with the rewritten absolute path and print the content.
```

## Configuration

- `llm`
  - `provider`: `anthropic`, `openai`, `minimaxi` (MiniMax native, recommended), or `openai-compatible` (requires `base_url`)
  - `api_key`: provider API key
  - `model`: e.g. `claude-3-5-sonnet-20241022`, `gpt-4o-mini`, `MiniMax-M2`
  - `base_url` (optional): custom endpoint for OpenAI-compatible servers
  - `retry`: `enabled`, `max_retries`, `initial_delay`, `max_delay`, `exponential_base`
- `agent`: `max_steps`, `token_limit` (default 80000), `completion_reserve` (default 2048), `workspace_dir`, `system_prompt_path`
- `tools`: enable/disable; `skills_dir`; `mcp_config_path`

Config precedence: `./miniagent/config/` → `~/.miniagent/config/` → `./config/`.
See `config/config-example.yaml` for a complete example.

### Environment Overrides

- Precedence: CLI > ENV > config file.
- Supported env vars:
  - `MINIAGENT_PROVIDER`, `MINIAGENT_MODEL`, `MINIAGENT_BASE_URL`, `MINIAGENT_API_KEY`
  - Provider-specific API keys (fallback): `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `MINIMAXI_API_KEY`
- Note: `provider=openai-compatible` requires a `base_url` (or `MINIAGENT_BASE_URL`).

## Skills (Claude Skills)
This repo includes the official Claude Skills under `skills/`. At build time we embed the entire directory into the binary (via `include_dir`, feature `embed-skills` enabled by default). On first run, if no on-disk skills are found, the embedded skills are extracted to `~/.miniagent/skills` automatically. The Agent uses Progressive Disclosure:

- Level 1: The names/descriptions of all discovered skills are injected into the system prompt.
- Level 2: The model loads full guidance using the tool `get_skill(skill_name)` when needed.
- Level 3+: Any relative file references in the skill content (e.g., under `examples/`, `scripts/`, `templates/`, `reference/`) are rewritten to absolute paths, with a hint to use `read_file`.

Examples of skill names you can load:
- `internal-comms`, `webapp-testing`, `mcp-builder`, `canvas-design`, `artifacts-builder`, `document-skills/pdf`, `document-skills/pptx`, `document-skills/docx`, `document-skills/xlsx`, `slack-gif-creator`, etc.

In the REPL, just describe your task; the model will decide when to call `get_skill`. You can also explicitly ask it to load a specific skill.

## Tools

- `read_file`, `write_file`, `edit_file`: file operations scoped to the workspace.
- `bash`: runs shell commands in the workspace directory.
- `record_note`, `recall_notes`: session notes in `<workspace>/.agent_memory.json`.
- `get_skill`: load full content of a skill by name.
- MCP tools: loaded at runtime from `config/mcp.json` (see below).

## MCP

- Edit `config/mcp.json` and set your server entry `disabled: false`.
- Example (git server with `uvx`):
```json
{
  "mcpServers": {
    "git": {
      "command": "uvx",
      "args": ["mcp-server-git"],
      "env": {},
      "disabled": true
    }
  }
}
```
- When `enable_mcp: true` and config exists, miniagent will spawn the servers, list their tools, and add them to the toolset.

### Quick Enable (Example)
1. Install the server binary (e.g., `uvx mcp-server-git`). Ensure it’s on PATH.
2. Edit `config/mcp.json` and set the entry’s `disabled: false`.
3. Run miniagent normally; the MCP tools will be listed at startup and usable by the agent.

## Summarization

- Agent keeps system and all user messages.
- For each user → next user segment (assistant/tool messages in-between), it asks the LLM to summarize.
- Triggered when estimated tokens exceed `token_limit - completion_reserve`.

Tip: Adjust `completion_reserve` (default 2048) to keep room for completions.

## Logging

- Logs per run are written to `~/.miniagent/log/agent_run_*.log`.
- Includes Request, Response, and Tool execution JSON payloads.

## Notes

- By default, tokenization is tiktoken (cl100k_base). If using models with different encodings (e.g., o200k_base), mapping can be extended later.
- MCP child-process servers require the respective binaries and environment.

## License

- Dual-licensed under MIT or Apache-2.0 at your option.
  - MIT: https://opensource.org/license/mit/
  - Apache-2.0: https://www.apache.org/licenses/LICENSE-2.0

## References

- This project is inspired by and references: https://github.com/MiniMax-AI/Mini-Agent/

## Build Features

- Default enabled features: `tiktoken`, `embed-skills`.
- Disable embedded skills (use on-disk skills only):
  - `cargo run --no-default-features --features "tiktoken"`
- Disable tiktoken too (use approximate estimator):
  - `cargo run --no-default-features`

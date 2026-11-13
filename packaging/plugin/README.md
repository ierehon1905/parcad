# The ParCAD plugin

One plugin directory, installable in Claude Code and in Codex. It carries the
MCP server and a skill, and no binaries: the server is `parcad mcp` from an
installed parcad, the Homebrew formula or the app, which both carry the same
`parcad`.

| file | read by |
|---|---|
| `.claude-plugin/plugin.json` | Claude Code; declares the server inline through `scripts/parcad-mcp` (not `bin/`, which Claude Code adds to the Bash PATH) |
| `.codex-plugin/plugin.json` | Codex; points at `codex.mcp.json` |
| `codex.mcp.json` | Codex only. Not `.mcp.json`: Claude Code loads that name on its own and would start the server twice. `env_vars` is required: Codex starts a server with PATH and little else, so without it `PARCAD_PROJECTS_DIR` is dropped and the agent reads the default folder. It runs `parcad` from PATH rather than the launcher: Codex 0.147 expands no plugin-root variable in an MCP command and resolves no relative path, so an app-only install is not found there |
| `scripts/parcad-mcp` | the one launcher, shared with the MCP bundle: `PARCAD_BIN`, PATH, Homebrew, then `ParCAD.app`, and a copy beside itself last — which only the bundle has. Names both installs when it finds none |
| `skills/parcad/SKILL.md` | both; tells the model to use these tools and what to do when they are absent |
| `skills/field-instrument/SKILL.md` | both; an optional style. A pointer only: the rules are `docs/styles/field-instrument.md`, served by `read_docs` so clients without skills get the same text |

The field-instrument style draws on the publicly documented design principles
of teenage engineering. ParCAD is not affiliated with or endorsed by them; the
style is described in its own terms and copies no product.

The marketplaces that list it are at the repository root:
`.claude-plugin/marketplace.json` for Claude Code, and
`.agents/plugins/marketplace.json` for Codex.

## Releasing

- Both `plugin.json` files carry the workspace version, and
  `plugin_manifests_carry_the_crate_version` fails when they don't match. An
  installed copy updates only when that version changes.
- The plugin needs a release whose `parcad` has the `mcp` subcommand. An older
  binary reads `mcp` as a file name and fails.
- Validate the plugin with `claude plugin validate packaging/plugin --strict`
  and the marketplace with `claude plugin validate . --strict`.

## Official directories

- **Claude's directory** (claude.ai/directory/manage) takes a local MCP server
  or a script file only from a Verified organisation, so this plugin was
  refused there on 2026-11-11. The directory lists
  `packaging/claude-plugin/` instead: the same skills, with the server
  pointed at `https://mcp.parcad.dev/mcp`, beside the hosted server's own
  connector listing, as Anthropic's docs ask a product with a remote server
  to do. Its `field-instrument` skill is a copy of this one's, held equal by
  `the_directory_plugins_carry_the_local_plugins_shared_skills`; its `parcad`
  skill differs because the hosted server has no install to fall back on.
  The directory reads it from `ierehon1905/parcad-claude-plugin`, because this
  repository is over its 10,000-file limit: run
  `tools/publish-claude-plugin.sh` after each release to copy the folder there.
- **ChatGPT and Codex**: OpenAI's directory takes a remote server only, from
  a verified developer, so it lists `packaging/chatgpt-plugin/`: the same
  skills, with the server pointed at `https://mcp.parcad.dev/mcp`. Its
  README says how to submit it. This plugin still installs in Codex from this
  repository: `codex plugin marketplace add ierehon1905/parcad` and
  `codex plugin add parcad@parcad`.
- **MCP Registry**: listed from the release's `.mcpb` bundle, not from this
  plugin; `packaging/mcpb/README.md` says how.

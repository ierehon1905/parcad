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

- **Claude Code**: the submission form lists a plugin in the community
  marketplace, `anthropics/claude-plugins-community`, installed as
  `parcad@claude-community`. An individual submits at
  <https://platform.claude.com/plugins/submit>; a Team or Enterprise
  organisation at claude.ai's directory settings. Approved plugins are pinned to
  a commit, and the pin follows new commits. The curated
  `claude-plugins-official` takes no applications.
- **Codex**: the public directory does not take local stdio servers without an
  arrangement with OpenAI, and asks for a verified developer identity and
  privacy and terms URLs. Until then, `codex plugin marketplace add
  ierehon1905/parcad` and `codex plugin add parcad@parcad` install from this
  repository.
- **MCP Registry**: listed from the release's `.mcpb` bundle, not from this
  plugin; `packaging/mcpb/README.md` says how.

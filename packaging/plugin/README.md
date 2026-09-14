# The ParCAD plugin

One plugin directory, installable in Claude Code and in Codex. It carries the
MCP server and a skill, and nothing else. The server is `parcad mcp` from the
Homebrew formula, so the plugin works only after the formula is installed.

| file | read by |
|---|---|
| `.claude-plugin/plugin.json` | Claude Code; declares the server inline through `bin/parcad-mcp` |
| `.codex-plugin/plugin.json` | Codex; points at `codex.mcp.json` |
| `codex.mcp.json` | Codex only. Not `.mcp.json`: Claude Code loads that name on its own and would start the server twice. `env_vars` is required: Codex starts a server with PATH and little else, so without it `PARCAD_PROJECTS_DIR` is dropped and the agent reads the default folder |
| `bin/parcad-mcp` | finds `parcad` when a client started from the Dock has no shell PATH, and names the install command when it is missing |
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

Each directory has its own review; submit after a release that includes `parcad mcp`.

- **Claude Code**: submit the form at `clau.de/plugin-directory-submission`.
  Approved plugins are pinned to a commit in Anthropic's catalog.
- **Codex**: submit through the OpenAI Platform flow (Plugins → Submit and publish).
  Until then, `codex plugin marketplace add ierehon1905/parcad` and
  `codex plugin add parcad@parcad` install from this repository.
- **MCP Registry** (`registry.modelcontextprotocol.io`): not done. It lists
  npm, PyPI, NuGet, OCI and MCPB packages, not a Homebrew formula. Listing
  there means publishing one of those too, most likely an `.mcpb` bundle of the
  release tarball.

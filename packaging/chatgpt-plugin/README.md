# ParCAD for ChatGPT

The plugin OpenAI's directory lists: the hosted server at
`https://mcp.parcad.dev/mcp` with ParCAD's two skills, in the Agent Plugins
layout OpenAI's portal reads. Nothing in it runs on the user's computer.

| file | what it is |
|---|---|
| `plugin.json` | name, version, the listing (`extensions.com.openai.interface`), and the review's test cases and release notes |
| `mcp.json` | the server, signed in with OAuth. ChatGPT identifies itself with its Client ID Metadata Document, which the edge accepts (`edge/`); no client id or secret is in the package |
| `skills/parcad` | how to model with the tools; the hosted copy of `packaging/claude-plugin`'s, without the client's name |
| `skills/field-instrument` | the optional style, a copy of `packaging/plugin`'s, held equal by a test |
| `assets/icon.png` | a link to `app/public/icon-512.png`, the 512 px mark |

`tools/pack-chatgpt-plugin.sh` writes the ZIP the portal takes, with the icon
copied in and this README left out.

## Submitting

OpenAI's guide is <https://developers.openai.com/apps-sdk/deploy/submission>.

- The portal reads the listing, test cases and release notes from
  `plugin.json`; imported test cases are read-only there, so a change is a new
  upload, and every upload needs a new `version`. That version is the
  package's own, apart from the workspace's: the tools ChatGPT offers follow
  the live server, which OpenAI rescans daily, and a listing change should not
  wait for a release.
- `mcp.json` declares the OAuth sign-in under the server's
  `extensions["com.openai"]`, where OpenAI's guide puts it. The Agent Plugins
  schema at agent-plugins.org refuses extra keys on a server, so a generic
  validator flags that block; OpenAI's portal is the reader that counts.
- Reviewer credentials go in the portal's Review details, never in the ZIP.
  The reviewer signs in with the edge's access code (`REVIEW_CODE`, see
  `edge/README.md`), which needs no email, SMS or authenticator.
- The domain check is the edge's `/.well-known/openai-apps-challenge`, from
  the secret `OPENAI_APPS_CHALLENGE`.
- OpenAI requires `readOnlyHint`, `destructiveHint` and `openWorldHint` on
  every tool; `every_tool_says_whether_it_only_looks` in
  `crates/parcad-host/src/mcp.rs` holds that.

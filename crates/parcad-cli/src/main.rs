//! Headless driver. Takes an intent graph as JSON and produces everything an
//! agent (or a person) would want to look at.
//!
//!     parcad <graph.json> [--out DIR] [--size PX] [--view NAME]
//!
//! And the application itself, without a window, plus its tools from the shell:
//!
//!     parcad serve [--port N]
//!     parcad serve --hosted --bind ADDR --tokens FILE --tenants-dir DIR --public-host NAME
//!     parcad token new <tenant-id> --tokens FILE
//!     parcad mcp [--port N]
//!     parcad tools
//!     parcad call <tool> [JSON] [--set key=value]...
//!
//! A `.js` script is accepted wherever a graph is: it is built in the same
//! sandbox MCP runs scripts in, so `bun tools/run.ts` is not a prerequisite.

mod call;
mod stdio;
mod ui;

use anyhow::{Context, Result};
use parcad_core::{
    graph::Doc,
    render,
    view::{Axis, Keep, Section, View},
};
use std::path::PathBuf;
use std::time::Duration;

struct Args {
    input: PathBuf,
    out: PathBuf,
    size: u32,
    view: Option<View>,
    /// Also produce the tag-region map for the chosen view.
    regions: bool,
    /// Cut the part open before drawing it.
    section: Option<Section>,
    /// Dump viewport-ready geometry as JSON to this path.
    geometry: Option<PathBuf>,
    /// Seconds the kernel may take before it is stopped; `PARCAD_OCCT_TIMEOUT`
    /// or 20 when absent. A busy machine is the usual reason to raise it.
    timeout: Option<f64>,
    /// Write STEP, the exact surfaces.
    step: Option<PathBuf>,
    /// Read a foreign STEP export and print its measured geometry as JSON,
    /// instead of evaluating a graph. The reverse of `--step`: what another
    /// CAD system built, measured so a recreation has numbers to hit.
    probe_step: Option<PathBuf>,
    /// Lay this second graph against the part and measure the fit instead of
    /// evaluating: interference volume, or clearance when there is none.
    fit: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut input = None;
    let mut out = PathBuf::from("out");

    let mut size = 512u32;
    let mut view = None;
    let mut regions = false;
    let mut section = None;
    let mut geometry = None;

    let mut timeout = None;
    let mut step = None;
    let mut probe_step = None;
    let mut fit = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => out = it.next().context("--out needs a directory")?.into(),
            // Two flags from when there were two kernels. Both still parse
            // and neither does anything: the exact kernel is the only one, and
            // its mesh has no depth to set.
            "--depth" => {
                let _ = it.next().context("--depth needs a number")?;
                eprintln!("note: --depth is ignored; the exact kernel meshes to a deflection, not a grid");
            }
            "--size" => {
                size = it
                    .next()
                    .context("--size needs a number")?
                    .parse()
                    .context("--size must be an integer")?
            }
            "--view" => {
                let name = it.next().context("--view needs a name")?;
                view = Some(View::parse(&name).with_context(|| {
                    format!(
                        "unknown view {name:?}; expected one of: {}",
                        View::ALL.map(|v| v.name()).join(", ")
                    )
                })?);
            }
            "--regions" => regions = true,
            "--section" => {
                let spec = it
                    .next()
                    .context("--section needs a plane, e.g. z or y@5:above")?;
                section = Some(parse_section(&spec)?);
            }
            "--geometry" => {
                geometry = Some(PathBuf::from(it.next().context("--geometry needs a path")?))
            }
            "--brep" => eprintln!("note: --brep is the only kernel now and needs no flag"),
            "--timeout" => {
                timeout = Some(
                    it.next()
                        .context("--timeout needs a number of seconds")?
                        .parse::<f64>()
                        .context("--timeout must be a number of seconds")?,
                )
            }
            "--step" => step = Some(PathBuf::from(it.next().context("--step needs a path")?)),
            "--fit" => {
                fit = Some(PathBuf::from(
                    it.next().context("--fit needs a reference graph")?,
                ))
            }
            "--probe-step" => {
                probe_step = Some(PathBuf::from(
                    it.next().context("--probe-step needs a .step file")?,
                ))
            }
            "--version" => {
                println!("parcad {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: parcad serve [--port N]              # host the UI and MCP, no window\n\
                     \x20      parcad serve --hosted ...           # one server for many accounts; parcad serve --help\n\
                     \x20      parcad token new <tenant-id> --tokens FILE\n\
                     \x20      parcad mcp [--port N]                # MCP over stdio, for a client to launch\n\
                     \x20      parcad tools                        # what the running host offers\n\
                     \x20      parcad call <tool> [JSON] [--set key=value | key=@file | key:=json]\n\
                     \x20      parcad <graph.json | part.js> [--out DIR] [--size PX]\n\
                     \x20              [--view NAME] [--regions] [--section PLANE]\n\
                     \x20              [--geometry PATH] [--step PATH] [--timeout SECS]\n\
                     \x20              [--fit REFERENCE.json]   # measure the fit instead\n\
                     \x20      parcad --probe-step FILE.step   # measure a foreign export"
                );
                std::process::exit(0);
            }
            other if other.starts_with('-') => anyhow::bail!("unknown flag {other:?}"),
            other => input = Some(PathBuf::from(other)),
        }
    }

    if let Some(probe) = probe_step {
        return Ok(Args {
            input: input.unwrap_or_default(),
            out,
            size,
            view,
            regions,
            section,
            geometry,
            timeout,
            step,
            probe_step: Some(probe),
            fit: None,
        });
    }

    Ok(Args {
        input: input.context("expected a graph JSON file; try --help")?,
        out,
        size,
        view,
        regions,
        section,
        geometry,
        timeout,
        step,
        probe_step: None,
        fit,
    })
}

/// `AXIS[@MM][:below|above]` — `z`, `y@5`, `x@0:below`.
///
/// Defaulting both the position and the kept side is the point: `--section z`
/// is the section anyone actually wants, and the two suffixes are there for the
/// times it is not.
fn parse_section(spec: &str) -> Result<Section> {
    let (plane, keep) = match spec.split_once(':') {
        Some((plane, side)) => (
            plane,
            Some(Keep::parse(side).with_context(|| {
                format!("unknown side {side:?} in --section; expected below or above")
            })?),
        ),
        None => (spec, None),
    };
    let (axis, at_mm) = match plane.split_once('@') {
        Some((axis, at)) => (
            axis,
            Some(
                at.parse()
                    .with_context(|| format!("{at:?} in --section is not a position in mm"))?,
            ),
        ),
        None => (plane, None),
    };

    let axis = Axis::parse(axis).with_context(|| {
        format!(
            "unknown axis {axis:?} in --section; expected one of: {}",
            Axis::ALL.map(|a| a.name()).join(", ")
        )
    })?;
    Ok(Section { axis, at_mm, keep })
}

/// A part from disk: an intent graph as JSON, or a `.js` script built into one
/// in the same sandbox MCP uses, so the two never disagree about what a script
/// means.
fn load_doc(path: &std::path::Path) -> Result<Doc> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "js") {
        let graph = parcad_host::script::build_graph(&text)
            .map_err(|e| anyhow::anyhow!("building {}: {e}", path.display()))?;
        return parcad_core::envelope::parse_doc(graph)
            .map_err(|e| anyhow::anyhow!("the graph {} built: {e}", path.display()));
    }
    let graph = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as JSON", path.display()))?;
    parcad_core::envelope::parse_doc(graph)
        .map_err(|e| anyhow::anyhow!("reading {} as an intent graph: {e}", path.display()))
}

const SERVE_USAGE: &str = "usage: parcad serve [--port N] [--bind ADDR]\n\
       parcad serve --hosted --tokens FILE --tenants-dir DIR --public-host NAME [--bind ADDR] [--port N]\n\
\n\
  --port N           the port, 4242 or PARCAD_HTTP_PORT\n\
  --bind ADDR        the address, 127.0.0.1; anything else needs --hosted\n\
  --hosted           one server for many accounts, each behind a bearer token: no UI, no editor\n\
  --tokens FILE      the tokens file; `parcad token new <tenant-id> --tokens FILE` adds a line\n\
  --tenants-dir DIR  where each account's parts live, DIR/<tenant-id>\n\
  --public-host NAME where clients reach the server, behind whatever terminates TLS: cad.example.com\n\
                     is https://cad.example.com; http://127.0.0.1:4398 for a plain-HTTP host under test\n\
  --link-lifetime N  seconds a reply's url fetches its file with no bearer, 86400 (24 h)\n\
  --link-secret FILE what signs those urls: 32 random bytes, made on the first start; the tokens\n\
                     file's name plus .secret unless given. A new secret ends every link handed out\n\
  --builds N         builds in flight at once across every account, the machine's cores by default\n\
  --limit NAME=N     move one per-account limit (N, or off); PARCAD_LIMIT_<NAME> in the environment\n\
                     too, which a flag overrides. Each, with its hosted default:\n\
{limits}\n\
  --edge-key FILE     the Ed25519 public key of an edge in front of the server (an OAuth front):\n\
                     a pass it signs opens the account it names, with no token here\n\
  --edge-origin NAME the host the edge forwards to, which its passes are addressed to\n\
  --revoked FILE     account ids to refuse however they arrive, one a line; ends their links too\n\
  --worker-memory-mb N  the memory each kernel worker may allocate (Linux enforces it), or off;\n\
                     PARCAD_WORKER_MEMORY_MB. By default the container's memory, less 1 GB for the\n\
                     host, shared among --builds";

fn serve_usage() -> String {
    SERVE_USAGE.replace("{limits}", &parcad_host::limits::help(&parcad_host::limits::Limits::hosted()))
}

/// What `parcad serve` was asked for, checked whole before anything binds.
struct Serve {
    address: std::net::SocketAddr,
    /// The host's cap on builds in flight: the worker pool caps nothing.
    builds: usize,
    hosted: Option<HostedFlags>,
}

struct HostedFlags {
    tokens: PathBuf,
    tenants: PathBuf,
    public_host: parcad_host::tenant::PublicHost,
    link_secret: PathBuf,
    link_lifetime: std::time::Duration,
    /// The edge whose passes open accounts, and the file its key came from.
    edge: Option<(parcad_host::edge::Edge, PathBuf)>,
    /// Accounts refused however they arrive.
    revoked: Vec<parcad_host::tenant::TenantId>,
    limits: parcad_host::limits::Limits,
    /// Bytes each worker may allocate; `None` is no cap.
    worker_memory: Option<u64>,
}

fn serve_options(args: impl Iterator<Item = String>) -> Result<Serve> {
    let mut port = parcad_host::http::port();
    let mut bind = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let mut builds = std::thread::available_parallelism().map_or(1, |cores| cores.get());
    let mut hosted = false;
    let mut tokens = None;
    let mut tenants = None;
    let mut public_host = None;
    let mut link_secret = None;
    let mut link_lifetime = None;
    let mut edge_key = None;
    let mut edge_origin = None;
    let mut revoked_file = None;
    let mut limit_flags: Vec<String> = Vec::new();
    let mut worker_memory: Option<String> = std::env::var("PARCAD_WORKER_MEMORY_MB").ok();
    let mut args = args;
    while let Some(arg) = args.next() {
        let mut value = |what: &str| args.next().with_context(|| format!("{arg} needs {what}"));
        match arg.as_str() {
            "--port" => port = value("a number")?.parse().context("--port must be a port number")?,
            "--bind" => bind = value("an address")?.parse().context("--bind must be an IP address")?,
            "--builds" => builds = value("a count")?.parse().context("--builds must be a count of builds")?,
            "--hosted" => hosted = true,
            "--tokens" => tokens = Some(PathBuf::from(value("a file")?)),
            "--tenants-dir" => tenants = Some(PathBuf::from(value("a folder")?)),
            "--public-host" => {
                public_host = Some(parcad_host::tenant::PublicHost::parse(&value("a host name")?).map_err(anyhow::Error::msg)?)
            }
            "--link-secret" => link_secret = Some(PathBuf::from(value("a file")?)),
            "--link-lifetime" => {
                let seconds: u64 = value("a count of seconds")?.parse().context("--link-lifetime must be a count of seconds")?;
                if seconds == 0 {
                    anyhow::bail!("--link-lifetime must be at least 1 second, or every link would be expired as it is handed out");
                }
                link_lifetime = Some(std::time::Duration::from_secs(seconds));
            }
            "--edge-key" => edge_key = Some(PathBuf::from(value("a file")?)),
            "--edge-origin" => {
                edge_origin = Some(parcad_host::tenant::PublicHost::parse(&value("a host name")?).map_err(anyhow::Error::msg)?)
            }
            "--revoked" => revoked_file = Some(PathBuf::from(value("a file")?)),
            "--limit" => limit_flags.push(value("NAME=VALUE")?),
            "--worker-memory-mb" => worker_memory = Some(value("a count of MB, or off")?),
            "-h" | "--help" => {
                eprintln!("{}", serve_usage());
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument {other:?}\n{}", serve_usage()),
        }
    }
    let address = std::net::SocketAddr::new(bind, port);
    if builds == 0 {
        anyhow::bail!("--builds must be at least 1, or no part would ever build");
    }
    if !hosted {
        if !bind.is_loopback() {
            anyhow::bail!(
                "parcad would evaluate anyone's scripts on a routable address ({address}); \
                 add --hosted with --tokens FILE and --tenants-dir DIR, or leave --bind at 127.0.0.1"
            );
        }
        if tokens.is_some() || tenants.is_some() || public_host.is_some() || link_secret.is_some() || link_lifetime.is_some() {
            anyhow::bail!("--tokens, --tenants-dir, --public-host, --link-secret and --link-lifetime are read only with --hosted");
        }
        if edge_key.is_some() || edge_origin.is_some() || revoked_file.is_some() {
            anyhow::bail!("--edge-key, --edge-origin and --revoked are read only with --hosted");
        }
        if !limit_flags.is_empty() {
            anyhow::bail!("--limit bounds the accounts of a hosted server, and is read only with --hosted");
        }
        return Ok(Serve {
            address,
            builds,
            hosted: None,
        });
    }
    let limits = parcad_host::limits::Limits::configured(&limit_flags, |name| std::env::var(name).ok())
        .map_err(anyhow::Error::msg)?;
    let worker_memory = match worker_memory.as_deref().map(str::trim) {
        None => parcad_occt::default_worker_memory(builds),
        Some("off") => None,
        Some(mb) => {
            let mb: u64 = mb.parse().ok().filter(|mb| *mb > 0).with_context(|| {
                format!("--worker-memory-mb must be a whole number of MB above 0, or off, not {mb:?}")
            })?;
            Some(mb << 20)
        }
    };
    let revoked = revoked_file.as_deref().map(read_revoked).transpose()?.unwrap_or_default();
    match (tokens, tenants, public_host) {
        (Some(tokens), Some(tenants), Some(public_host)) => Ok(Serve {
            address,
            builds,
            hosted: Some(HostedFlags {
                link_secret: link_secret.unwrap_or_else(|| secret_beside(&tokens)),
                link_lifetime: link_lifetime.unwrap_or(parcad_host::signing::Links::DEFAULT_LIFETIME),
                edge: match (edge_key, edge_origin) {
                    (None, None) => None,
                    (Some(key), Some(origin)) => {
                        let text = std::fs::read_to_string(&key)
                            .with_context(|| format!("reading the edge key {}", key.display()))?;
                        let edge = parcad_host::edge::Edge::new(&text, &public_host, origin)
                            .map_err(|e| anyhow::anyhow!("{}: {e}", key.display()))?;
                        Some((edge, key))
                    }
                    (Some(_), None) => anyhow::bail!(
                        "--edge-key needs --edge-origin, the host the edge forwards to (origin.example.com), \
                         which is what its passes are addressed to"
                    ),
                    (None, Some(_)) => anyhow::bail!(
                        "--edge-origin needs --edge-key, the file holding the edge's Ed25519 public key"
                    ),
                },
                revoked,
                tokens,
                tenants,
                public_host,
                limits,
                worker_memory,
            }),
        }),
        (tokens, tenants, public_host) => {
            let missing: Vec<&str> = [
                tokens.is_none().then_some("--tokens FILE"),
                tenants.is_none().then_some("--tenants-dir DIR"),
                public_host.is_none().then_some("--public-host NAME"),
            ]
            .into_iter()
            .flatten()
            .collect();
            anyhow::bail!("--hosted needs {}", missing.join(", "))
        }
    }
}

/// The accounts `--revoked` names: one id a line, blank lines and `#`
/// comments allowed, as in the tokens file.
fn read_revoked(path: &std::path::Path) -> Result<Vec<parcad_host::tenant::TenantId>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading the revoked accounts {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(at, line)| (at, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(at, line)| {
            parcad_host::tenant::TenantId::new(line).map_err(|e| anyhow::anyhow!("{}:{}: {e}", path.display(), at + 1))
        })
        .collect()
}

/// Where the link secret lives unless `--link-secret` says: the tokens file's
/// whole name plus `.secret`, so `tokens.txt` gets `tokens.txt.secret`.
fn secret_beside(tokens: &std::path::Path) -> PathBuf {
    let mut name = tokens.as_os_str().to_owned();
    name.push(".secret");
    PathBuf::from(name)
}

/// The application without its window: seed the project folder, host the UI,
/// the API and MCP on the port, and stay up until stopped.
///
/// This is what `brew services start parcad` runs. It is the same router and
/// the same `service` the desktop app uses — a browser on the port is the whole
/// application, and a model on `/mcp` sees the same parts. With `--hosted` it
/// is one server for many accounts instead: no UI, every request a bearer's.
fn serve(args: impl Iterator<Item = String>) -> Result<()> {
    let options = serve_options(args)?;
    let Some(flags) = options.hosted else {
        parcad_host::service::cap_builds(options.builds).map_err(anyhow::Error::msg)?;
        return host(options.address.port());
    };
    let secret = parcad_host::signing::Secret::load_or_create(&flags.link_secret).map_err(anyhow::Error::msg)?;
    let links = parcad_host::signing::Links::new(flags.public_host, secret, flags.link_lifetime);
    let policy = parcad_host::tenant::Policy {
        limits: flags.limits,
        server: std::sync::Arc::new(parcad_host::limits::Gate::new(Some(options.builds))),
        shipped: parcad_host::projects::shipped_dir(),
    };
    let mut accounts = parcad_host::tenant::Accounts::read(&flags.tokens, &flags.tenants, links)
        .map_err(anyhow::Error::msg)?
        .governed(policy)
        .revoking(flags.revoked.clone());
    if let Some((edge, key)) = &flags.edge {
        accounts = accounts.trusting(edge.clone());
        eprintln!(
            "parcad: passes from an edge open accounts too: addressed to {}, signed with the key in {}",
            edge.origin(),
            key.display()
        );
    }
    std::fs::create_dir_all(&flags.tenants)
        .with_context(|| format!("creating the tenants folder {}", flags.tenants.display()))?;
    parcad_occt::confine_workers(parcad_occt::WorkerLimits {
        memory_bytes: flags.worker_memory,
        oom_first: true,
    });
    eprintln!(
        "parcad: {} tokens from {}; accounts under {}; {} builds at once, {} per account; each worker {}; \
         shipped parts from {}; links signed with {}",
        accounts.tokens(),
        flags.tokens.display(),
        flags.tenants.display(),
        options.builds,
        flags.limits.builds_at_once.map_or("any".to_string(), |n| n.to_string()),
        flags.worker_memory.map_or("uncapped".to_string(), |bytes| format!("capped at {} MB", bytes >> 20)),
        accounts.policy().shipped.as_ref().map_or("nowhere".to_string(), |dir| dir.display().to_string()),
        flags.link_secret.display()
    );
    parcad_host::tenant::require();
    listen(options.address, Some(accounts))
}

/// Seed the project folder and host the application on `port` until the
/// listener stops. `parcad serve` is this on the main thread; `parcad mcp` runs
/// it on a thread of its own when it finds no host to relay to.
fn host(port: u16) -> Result<()> {
    // Seed before the host comes up: the frontend asks for the project list
    // as it loads, and an empty first launch would look like a fresh install
    // with nothing in it.
    if let Err(e) = parcad_host::projects::seed() {
        eprintln!(
            "parcad: could not prepare the project folder {}: {e}\n\
             Point PARCAD_PROJECTS_DIR at a folder this process may write.",
            parcad_host::projects::dir().display()
        );
    }
    eprintln!(
        "parcad: parts in {}",
        parcad_host::projects::dir().display()
    );
    if !ui::carries_ui() {
        eprintln!(
            "parcad: this binary was built without app/dist, so it hosts the API and MCP \
             but no UI. Build the frontend and rebuild: cd app && bun run build && \
             cargo build --release -p parcad-cli"
        );
    }
    listen(parcad_host::http::loopback(port), None)
}

fn listen(address: std::net::SocketAddr, accounts: Option<parcad_host::tenant::Accounts>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    runtime
        .block_on(parcad_host::http::serve(
            address,
            std::sync::Arc::new(ui::Embedded),
            accounts,
        ))
        .map_err(anyhow::Error::msg)
}

/// `parcad token new <tenant-id> --tokens FILE`: a token for one account,
/// printed once on stdout; the file gets its hash.
fn token(args: impl Iterator<Item = String>) -> Result<()> {
    const USAGE: &str = "usage: parcad token new <tenant-id> --tokens FILE";
    let mut args = args;
    if args.next().as_deref() != Some("new") {
        anyhow::bail!("{USAGE}");
    }
    let mut id = None;
    let mut tokens = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tokens" => tokens = Some(PathBuf::from(args.next().context("--tokens needs a file")?)),
            other if other.starts_with('-') => anyhow::bail!("unknown argument {other:?}\n{USAGE}"),
            other => id = Some(other.to_string()),
        }
    }
    let (Some(id), Some(tokens)) = (id, tokens) else {
        anyhow::bail!("{USAGE}");
    };
    let tenant = parcad_host::tenant::TenantId::new(&id).map_err(anyhow::Error::msg)?;
    let token = parcad_host::tenant::issue(tenant, &tokens).map_err(anyhow::Error::msg)?;
    println!("{token}");
    eprintln!(
        "parcad: a token for {id} is on stdout, shown once; {} holds its hash. \
         A running `parcad serve --hosted` reads the file at start: restart it to take the token.",
        tokens.display()
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => return serve(args),
        Some("token") => return token(args),
        Some("mcp") => return stdio::run(args),
        Some("tools") => return call::tools(args),
        Some("call") => return call::call(args),
        _ => {}
    }
    let args = parse_args()?;

    // Probe mode: measure a foreign export instead of evaluating a graph. The
    // JSON goes to stdout for a pipeline; the one-line summary goes to stderr
    // for a person, so the two never mix.
    if let Some(path) = &args.probe_step {
        let probe = parcad_occt::probe_step(path, &parcad_occt::Options::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for (i, solid) in probe.solids.iter().enumerate() {
            let size: Vec<String> = (0..3)
                .map(|a| format!("{:.2}", solid.bbox_max[a] - solid.bbox_min[a]))
                .collect();
            eprintln!(
                "solid {i}: volume {:.2} mm³, area {:.2} mm², bbox {}, {} faces ({})",
                solid.volume_mm3,
                solid.area_mm2,
                size.join(" x "),
                solid.faces.len(),
                solid
                    .face_types
                    .iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if probe.free_faces > 0 {
            eprintln!("{} faces belong to no solid", probe.free_faces);
        }
        println!("{}", serde_json::to_string_pretty(&probe)?);
        return Ok(());
    }

    let doc = load_doc(&args.input)?;

    // Fit mode: the part against the object it holds, measured on the exact
    // solids. JSON to stdout, the sentence to stderr, like the probe.
    if let Some(path) = &args.fit {
        let reference = load_doc(path)?;
        let mut opts = parcad_occt::Options::default();
        if let Some(secs) = args.timeout {
            opts.timeout = std::time::Duration::from_secs_f64(secs);
        }
        let report =
            parcad_occt::check_fit(&doc, &reference, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
        match (&report.clearance_mm, &report.closest_mm, &report.contact_mm2) {
            (_, _, Some(area)) => eprintln!(
                "fit      {}: over {area:.3} mm² in {} patch(es) — 0 mm² is a point or an edge, not a seat",
                report.verdict,
                report.contact_patches.unwrap_or(0)
            ),
            (Some(gap), Some([a, b]), _) => eprintln!(
                "fit      {}: clearance {gap:.3} mm, between ({:.2}, {:.2}, {:.2}) on the part and ({:.2}, {:.2}, {:.2}) on the reference",
                report.verdict, a[0], a[1], a[2], b[0], b[1], b[2]
            ),
            _ => eprintln!(
                "fit      {}: {:.3} mm³ shared{} — material that would have to go for the reference to fit",
                report.verdict,
                report.interference_mm3,
                report.depth_mm.map_or(String::new(), |deep| format!(", {deep:.3} mm deep"))
            ),
        }
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;
    run_brep(&args, &doc)
}

/// "1 body", "1 body, 1 void", or the count that says the part is in pieces.
fn bodies_text(m: &parcad_core::mesh::MeshStats) -> String {
    let bodies = if m.bodies == 1 {
        "1 body".to_string()
    } else {
        format!("{} SEPARATE BODIES", m.bodies)
    };
    match m.voids {
        0 => bodies,
        1 => format!("{bodies}, 1 void"),
        n => format!("{bodies}, {n} voids"),
    }
}

/// "on 26469 mm² at z 0.00, 1 patch, 74% of the footprint" — the line that
/// tells a stubbed underside from a slab.
fn stands_on_text(c: &parcad_core::mesh::BedContact) -> String {
    format!(
        "on {:.0} mm² at z {:.2}, {} patch{}, {:.0}% of the footprint",
        c.area_mm2,
        c.z_mm,
        c.patches,
        if c.patches == 1 { "" } else { "es" },
        c.footprint_fraction * 100.0
    )
}

/// The B-rep path: geometry, measurements, exports, renders off the exact
/// tessellation, and a region map off the kernel's face lineage.
fn run_brep(args: &Args, doc: &Doc) -> Result<()> {
    let stl_path = args.out.join("part.stl");
    let opts = parcad_occt::Options {
        step_path: args.step.clone(),
        timeout: args
            .timeout
            .map(Duration::from_secs_f64)
            .unwrap_or_else(parcad_occt::default_timeout),
        ..Default::default()
    };

    let started = std::time::Instant::now();
    let s = parcad_occt::evaluate(doc, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    let kernel_ms = started.elapsed().as_millis();

    // The part's own triangles: a reference body is drawn below, and is in
    // no measurement, no file and no frame.
    let (vertices, triangles) = s.part_mesh();

    // Weld before measuring: OCCT triangulates face by face, so every shared
    // edge arrives as two coincident copies and the raw index graph looks full
    // of holes on a part that is perfectly closed.
    let tess = parcad_core::mesh::Tessellation {
        vertices,
        triangles,
        resolution_mm: s.deflection_mm,
    }
    .weld(1e-3);
    let stats = tess.stats();
    let bounds = parcad_core::measure::Aabb::from_points(&tess.vertices)
        .context("the kernel returned a mesh with no vertices")?;
    let (mass, mass_from_mesh) =
        parcad_occt::protocol::mass_or(s.mass.as_ref(), parcad_core::measure::mass_properties(&tess.vertices, &tess.triangles));

    // The STL is these triangles, welded and binary — the same file the app
    // exports — rather than OCCT's own ASCII writer's view of the shape.
    let mut f = std::fs::File::create(&stl_path)
        .with_context(|| format!("creating {}", stl_path.display()))?;
    tess.write_stl(&mut f)?;

    // Views from the mesh, by the same rasteriser the agent's renders use.
    let owners = parcad_occt::drawing::TriangleOwners::of(&s);
    let surface = render::Surface {
        positions: &s.positions,
        normals: &s.normals,
        indices: &s.indices,
        faces: &owners.faces,
        bodies: &owners.bodies,
        reference_bodies: &owners.reference_bodies,
    };
    let opts = render::RenderOptions {
        size: args.size,
        section: args.section,
        ..Default::default()
    };
    let render_started = std::time::Instant::now();
    let image_path = match args.view {
        Some(v) => {
            let img = render::render_surface_view(&surface, bounds, v, &opts)?;
            let path = args.out.join(format!("{}.png", v.name()));
            img.write_png(&path)?;
            path
        }
        None => {
            let sheet = render::contact_sheet_of(&surface, bounds, &opts)?;
            let path = args.out.join("views.png");
            sheet.image.write_png(&path)?;
            path
        }
    };
    let render_ms = render_started.elapsed().as_millis();

    // Tag regions: which named node owns which face of the visible surface,
    // from the kernel's own face lineage.
    let mut region_path = None;
    if args.regions {
        let v = args.view.unwrap_or(parcad_core::view::View::Iso);
        let names = parcad_occt::drawing::tag_names(doc);
        let owner_of_face = parcad_occt::drawing::owner_of_face(&s.faces, &names);
        let buffer = render::raster(&surface, bounds, v, &opts)?;
        let map = parcad_core::tags::regions_by_face(&buffer, &owner_of_face, &names, &opts)?;
        let path = args.out.join(format!("regions-{}.png", v.name()));
        map.image.write_png(&path)?;
        std::fs::write(
            args.out.join("regions.json"),
            serde_json::to_string_pretty(&map.legend)?,
        )?;
        println!("tags in the {} view", v.name());
        for e in &map.legend {
            if e.visible {
                println!(
                    "  {:<10} {}  {:>5.1}% of visible surface",
                    e.tag,
                    e.color,
                    e.fraction * 100.0
                );
            } else {
                println!("  {:<10} not visible from here", e.tag);
            }
        }
        if map.unclaimed_pixels > 0 {
            println!("  {} pixels claimed by no tag", map.unclaimed_pixels);
        }
        println!();
        region_path = Some(path);
    }

    if let Some(path) = &args.geometry {
        let payload = serde_json::json!({
            "positions": s.positions,
            "normals": s.normals,
            "indices": s.indices,
            "edges": s.edges,
            "topology": s.topology,
            "backend": "brep",
            "report": {
                "units": doc.units,
                "bounds": bounds,
                "size": bounds.size(),
                "framing_bounds": bounds,
                "mass": mass,
                "mesh": stats,
                "tags": doc.tags().into_iter().map(|(_, t)| t.to_string()).collect::<Vec<_>>(),
                "live_nodes": doc.topo_order()?.len(),
                "total_nodes": doc.nodes.len(),
            },
            "timings": {
                "lower_and_mesh_ms": s.timings.build_ms + s.timings.mesh_ms,
                "normals_ms": 0,
                "kernel_ms": kernel_ms,
            },
        });
        std::fs::write(path, serde_json::to_string(&payload)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    let size = bounds.size();
    println!("backend  b-rep (OpenCASCADE)");
    println!("size     {:.2} x {:.2} x {:.2} mm", size.x, size.y, size.z);
    println!(
        "bounds   x {:.2}..{:.2}  y {:.2}..{:.2}  z {:.2}..{:.2}",
        bounds.min.x, bounds.max.x, bounds.min.y, bounds.max.y, bounds.min.z, bounds.max.z
    );
    if let Some(contact) = tess.bed_contact() {
        println!("stands   {}", stands_on_text(&contact));
    }
    println!("prints   {}", parcad_core::measure::beds_text(size));
    for c in &s.collisions {
        println!(
            "cuts     {} cuts {} by {:.3} mm³ at ({:.2}, {:.2}, {:.2}) besides {}{}",
            c.cut, c.feature, c.removed_mm3, c.at[0], c.at[1], c.at[2], c.target,
            c.body.as_deref().map(|b| format!(" in body {b}")).unwrap_or_default()
        );
    }
    println!(
        "volume   {:.2} mm³   area {:.2} mm²{}",
        mass.volume_mm3,
        mass.area_mm2,
        if mass_from_mesh { "   (the mesh's: the B-rep's integrals cannot be trusted on this part)" } else { "" }
    );
    println!(
        "topology {} faces, {} edges ({} unique curves)",
        s.topology.faces,
        s.topology.edges,
        s.edges.len()
    );
    println!(
        "mesh     {} triangles within {:.3} mm of the true surface, {}, {}",
        stats.triangles,
        stats.resolution_mm,
        if stats.watertight {
            "watertight".to_string()
        } else {
            format!("NOT watertight ({} bad edges)", stats.non_manifold_edges)
        },
        bodies_text(&stats)
    );
    if let Some(deviation) = s.deviation_mm {
        println!("fit      fitted and function curves within {deviation:.4} mm of their points at worst");
    }
    if let Some(bound) = doc.stated_curve_bound() {
        println!(
            "curve    function curves within {:.2e} mm of their functions everywhere, {}",
            bound.mm,
            if bound.certified { "certified by the script" } else { "estimated by the script from samples" }
        );
    }
    if let Some(wall) = s.loft_wall_mm {
        println!("wall     the walled loft measures {:.4} to {:.4} mm between its skins", wall.min, wall.max);
    }
    if let Some(sag) = s.facet_sag_mm {
        println!("facets   ruled loft walls lie up to {sag:.4} mm from the smooth loft through their sections");
    }
    if let Some(jump) = s.curvature_jump_per_mm {
        println!("smooth   curvature steps by at most {jump:.1e}/mm where a smooth() blend meets its faces");
    }
    for body in parcad_occt::measure_bodies(&s) {
        let size = body.bounds.size();
        println!(
            "body     {}: {:.2} x {:.2} x {:.2} mm, {:.2} mm³, {} faces, {}{}",
            body.name,
            size.x,
            size.y,
            size.z,
            body.mass.volume_mm3,
            body.faces,
            if body.stats.watertight { "watertight" } else { "NOT watertight" },
            if body.stats.bodies == 1 {
                String::new()
            } else {
                format!(", in {} PIECES", body.stats.bodies)
            },
        );
    }
    for fit in &s.between {
        match (fit.contact_mm2, fit.clearance_mm) {
            (Some(area), _) => println!(
                "between  {} and {}: {}, over {area:.3} mm² in {} patch(es)",
                fit.a,
                fit.b,
                fit.verdict,
                fit.contact_patches.unwrap_or(0)
            ),
            (None, Some(gap)) => println!(
                "between  {} and {}: {}, clearance {gap:.3} mm",
                fit.a, fit.b, fit.verdict
            ),
            (None, None) => println!(
                "between  {} and {}: {}, {:.3} mm³ shared{}",
                fit.a,
                fit.b,
                fit.verdict,
                fit.interference_mm3,
                fit.depth_mm.map_or(String::new(), |deep| format!(", {deep:.3} mm deep"))
            ),
        }
    }
    println!(
        "timing   build {} ms, mesh {} ms, export {} ms, wall {} ms, render {} ms",
        s.timings.build_ms, s.timings.mesh_ms, s.timings.export_ms, kernel_ms, render_ms
    );
    println!("  stl      {}", stl_path.display());
    println!("  image    {}", image_path.display());
    if let Some(p) = &s.step_path {
        println!("  step     {}", p.display());
    }
    if let Some(p) = &args.geometry {
        println!("  geometry {}", p.display());
    }
    if let Some(p) = region_path {
        println!("  regions  {}", p.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Serve> {
        serve_options(args.iter().map(|arg| arg.to_string()))
    }

    /// A kernel that evaluates anyone's script never sits on a routable
    /// address without an account on every request.
    #[test]
    fn a_routable_address_needs_hosted_mode_and_hosted_mode_needs_its_three_flags() {
        let refused = options(&["--bind", "0.0.0.0", "--port", "4242"]).err().expect("refused").to_string();
        assert!(refused.contains("anyone's scripts"), "{refused}");
        assert!(refused.contains("add --hosted with --tokens FILE and --tenants-dir DIR"), "{refused}");
        assert!(options(&["--bind", "::1", "--port", "4242"]).is_ok(), "loopback of either family is local");

        let missing = options(&["--hosted", "--bind", "0.0.0.0", "--tokens", "t"]).err().expect("refused").to_string();
        assert!(missing.contains("--tenants-dir DIR") && missing.contains("--public-host NAME"), "{missing}");
        assert!(!missing.contains("--tokens FILE"), "only what is missing is named: {missing}");
        let stray = options(&["--tokens", "t", "--port", "4242"]).err().expect("refused").to_string();
        assert!(stray.contains("only with --hosted"), "{stray}");
        let stray = options(&["--link-lifetime", "60", "--port", "4242"]).err().expect("refused").to_string();
        assert!(stray.contains("only with --hosted"), "{stray}");

        let hosted = options(&[
            "--hosted", "--bind", "0.0.0.0", "--port", "4242", "--tokens", "/etc/parcad/tokens",
            "--tenants-dir", "/data/tenants", "--public-host", "cad.example.test",
        ])
        .unwrap();
        assert_eq!(hosted.address.to_string(), "0.0.0.0:4242");
        assert!(hosted.builds >= 1, "the machine's cores by default");
        let flags = hosted.hosted.expect("hosted");
        assert_eq!(flags.tokens, PathBuf::from("/etc/parcad/tokens"));
        assert_eq!(flags.tenants, PathBuf::from("/data/tenants"));
        assert_eq!(flags.public_host.to_string(), "https://cad.example.test", "https unless written otherwise");
        assert_eq!(flags.public_host.authority(), "cad.example.test");
        assert_eq!(flags.link_secret, PathBuf::from("/etc/parcad/tokens.secret"), "beside the tokens file");
        assert_eq!(flags.link_lifetime, std::time::Duration::from_secs(86_400), "a day");

        let plain_http = options(&[
            "--hosted", "--tokens", "t", "--tenants-dir", "d", "--public-host", "http://127.0.0.1:4398",
            "--link-secret", "/data/link-secret", "--link-lifetime", "3600",
        ])
        .unwrap()
        .hosted
        .expect("hosted");
        assert_eq!(plain_http.public_host.to_string(), "http://127.0.0.1:4398");
        assert_eq!(plain_http.link_secret, PathBuf::from("/data/link-secret"));
        assert_eq!(plain_http.link_lifetime, std::time::Duration::from_secs(3600));
        let never = options(&["--hosted", "--tokens", "t", "--tenants-dir", "d", "--public-host", "h", "--link-lifetime", "0"])
            .err()
            .expect("refused")
            .to_string();
        assert!(never.contains("at least 1 second"), "{never}");
        let refused = options(&["--hosted", "--public-host", "https://cad.example.test/mcp"]).err().expect("refused").to_string();
        assert!(refused.contains("it carries a path"), "{refused}");
        assert!(refused.contains("http://127.0.0.1:4398"), "the refusal names the accepted forms: {refused}");

        assert_eq!(options(&["--builds", "3", "--port", "4242"]).unwrap().builds, 3);
        let none = options(&["--builds", "0", "--port", "4242"]).err().expect("refused").to_string();
        assert!(none.contains("at least 1"), "{none}");
    }

    /// Every per-account limit moves with `--limit NAME=VALUE`, read only
    /// with --hosted; the worker's memory cap with --worker-memory-mb, where
    /// `off` is none.
    #[test]
    fn a_hosted_servers_limits_move_by_flag_and_need_hosted_mode() {
        let hosted = |extra: &[&str]| {
            let mut args = vec!["--hosted", "--tokens", "t", "--tenants-dir", "d", "--public-host", "h"];
            args.extend_from_slice(extra);
            options(&args)
        };
        let defaults = hosted(&[]).unwrap().hosted.expect("hosted");
        assert_eq!(defaults.limits, parcad_host::limits::Limits::hosted());
        let moved = hosted(&["--limit", "calls-per-minute=30", "--limit", "storage-mb=20", "--worker-memory-mb", "900"])
            .unwrap()
            .hosted
            .expect("hosted");
        assert_eq!(moved.limits.calls_per_minute, Some(30));
        assert_eq!(moved.limits.storage_bytes, Some(20 << 20));
        assert_eq!(moved.worker_memory, Some(900 << 20));
        assert_eq!(hosted(&["--worker-memory-mb", "off"]).unwrap().hosted.unwrap().worker_memory, None);
        let wrong = hosted(&["--limit", "calls=3"]).err().expect("refused").to_string();
        assert!(wrong.contains("no limit is called \"calls\""), "{wrong}");
        let zero = hosted(&["--worker-memory-mb", "0"]).err().expect("refused").to_string();
        assert!(zero.contains("above 0, or off"), "{zero}");
        let local = options(&["--limit", "calls-per-minute=30", "--port", "4242"]).err().expect("refused").to_string();
        assert!(local.contains("read only with --hosted"), "{local}");
        let usage = serve_usage();
        assert!(usage.contains("calls-per-minute=60") && usage.contains("--worker-memory-mb"), "{usage}");
        assert!(!usage.contains("{limits}"), "{usage}");
    }

    /// An edge is its key and its origin together, read with --hosted only;
    /// a revoked list is account ids, refused line by line.
    #[test]
    fn an_edge_needs_its_key_and_its_origin_and_only_hosted() {
        let dir = std::env::temp_dir().join(format!("parcad-edge-flags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = dir.join("edge.pub");
        std::fs::write(&key, "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc\n").unwrap();
        let revoked = dir.join("revoked");
        std::fs::write(&revoked, "# gone\nu-k3x9q2mf7a\n\nacme\n").unwrap();
        let key = key.to_str().unwrap();
        let revoked = revoked.to_str().unwrap();
        let hosted = ["--hosted", "--tokens", "t", "--tenants-dir", "d", "--public-host", "mcp.example.test"];
        let with = |extra: &[&str]| options(&[&hosted[..], extra].concat());

        let serve = with(&["--edge-key", key, "--edge-origin", "origin.example.test", "--revoked", revoked]).unwrap();
        let flags = serve.hosted.expect("hosted");
        assert_eq!(flags.edge.expect("an edge").0.origin().to_string(), "https://origin.example.test");
        assert_eq!(flags.revoked.iter().map(|id| id.as_str()).collect::<Vec<_>>(), ["u-k3x9q2mf7a", "acme"]);

        let alone = with(&["--edge-key", key]).err().expect("refused").to_string();
        assert!(alone.contains("--edge-origin"), "{alone}");
        let origin_alone = with(&["--edge-origin", "origin.example.test"]).err().expect("refused").to_string();
        assert!(origin_alone.contains("--edge-key"), "{origin_alone}");
        let local = options(&["--edge-key", key, "--edge-origin", "origin.example.test"]).err().expect("refused").to_string();
        assert!(local.contains("only with --hosted"), "{local}");
        std::fs::write(dir.join("bad"), "Not An Id\n").unwrap();
        let bad = with(&["--revoked", dir.join("bad").to_str().unwrap()]).err().expect("refused").to_string();
        assert!(bad.contains(":1:") && bad.contains("not a tenant id"), "{bad}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

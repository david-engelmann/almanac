//! The almanac command line interface.
//!
//! Four commands. `generate` writes the dual path discovery
//! catalog and the baseline from a repo's real artifacts and the
//! live surface. `validate` checks a catalog beyond schema.
//! `check` compares the published catalog, the baseline and the
//! live surface and exits nonzero on drift. `migrate` takes a
//! legacy ai-catalog.json publisher to the dual path pair and the
//! gate. The drift gate is the product, generation is its
//! onboarding, because the generation run records the baseline the
//! gate compares against.
//!
//! Exit codes. 0 means no drift at or above the fail severity,
//! 1 means drift, 2 means the command could not run.

mod probe;
mod probe_http;
mod site;
mod sse;

use std::path::PathBuf;
use std::process::ExitCode;

use almanac_core::Severity;
use site::ProbeTarget;

const USAGE: &str = "almanac, a drift gate for agent discovery catalogs

Commands
  generate  Write the dual path catalog and the baseline from repo
            artifacts and the live surface
  validate  Check a catalog beyond schema, URN anchoring, entry
            shape, representative query counts, path coherence
  check     Compare published catalog, baseline and live surface
            and exit nonzero on drift at or above the fail severity
  migrate   Verify a legacy ai-catalog.json against the live
            surface and emit the dual path pair and the gate

Target a surface with --url https://host/mcp or with --command and
the server command last. Sites are directories, --site DIR.";

#[derive(Default)]
struct Flags {
    site: Option<String>,
    url: Option<String>,
    domain: Option<String>,
    name: Option<String>,
    skill: Option<String>,
    fail_at: Option<String>,
    queries: Vec<String>,
    command: Vec<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--command" {
            flags.command = args[i + 1..].to_vec();
            break;
        }
        let value = |i: usize| -> Result<String, String> {
            let v = args
                .get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))?;
            if v.starts_with("--") {
                return Err(format!("{arg} expects a value, got {v}"));
            }
            Ok(v)
        };
        match arg {
            "--site" => flags.site = Some(value(i)?),
            "--url" => flags.url = Some(value(i)?),
            "--domain" => flags.domain = Some(value(i)?),
            "--name" => flags.name = Some(value(i)?),
            "--skill" => flags.skill = Some(value(i)?),
            "--fail-at" => flags.fail_at = Some(value(i)?),
            "--queries" => {
                flags.queries = value(i)?.split(',').map(|q| q.trim().to_string()).collect()
            }
            other => return Err(format!("unknown argument {other}")),
        }
        i += 2;
    }
    Ok(flags)
}

/// The target the operator named, when they named one. Check
/// runs without one, resolving the endpoint from the catalog or
/// falling back to repo mode. Generate requires one.
fn target(flags: &Flags) -> Result<Option<ProbeTarget>, String> {
    match (&flags.url, flags.command.is_empty()) {
        (Some(url), true) => Ok(Some(ProbeTarget::Http(url.clone()))),
        (None, false) => Ok(Some(ProbeTarget::Stdio(flags.command.clone()))),
        (Some(_), false) => Err("give either --url or --command, not both".into()),
        (None, true) => Ok(None),
    }
}

fn fail_at(flags: &Flags) -> Result<Severity, String> {
    match flags.fail_at.as_deref() {
        None | Some("major") => Ok(Severity::Major),
        Some("minor") => Ok(Severity::Minor),
        Some("critical") => Ok(Severity::Critical),
        Some(other) => Err(format!("unknown fail severity {other}")),
    }
}

fn run() -> Result<i32, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        println!("{USAGE}");
        return Ok(0);
    };
    match command.as_str() {
        "--help" | "-h" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        "generate" => {
            let flags = parse_flags(&args[1..])?;
            let site = PathBuf::from(flags.site.as_deref().unwrap_or("."));
            let domain = flags
                .domain
                .as_deref()
                .ok_or("--domain is required for generate")?;
            let name = flags.name.as_deref().unwrap_or(domain);
            let target =
                target(&flags)?.ok_or("generate needs a surface target, --url or --command")?;
            let skill = flags.skill.as_deref().map(PathBuf::from);
            site::generate(
                &site,
                domain,
                name,
                &target,
                skill.as_deref(),
                &flags.queries,
            )?;
            println!("catalog and baseline written under {}", site.display());
            Ok(0)
        }
        "check" => {
            let flags = parse_flags(&args[1..])?;
            let site = PathBuf::from(flags.site.as_deref().unwrap_or("."));
            let target = target(&flags)?;
            site::check(&site, target.as_ref(), fail_at(&flags)?)
        }
        "validate" | "migrate" => Err(format!(
            "almanac {command} is not yet implemented. Generate and check land first, \
             in that order, because the gate is the product and generation is its onboarding"
        )),
        other => Err(format!("unknown command {other}\n\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

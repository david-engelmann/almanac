//! The almanac command line.
//!
//! Four commands, fixed by the design record in docs/DESIGN.md.
//! The scaffold carries the command surface and the usage text.
//! Each command reports that it is not yet implemented until its
//! build milestone lands, starting with the Streamable HTTP probe
//! transport and the authenticated surface path.

use std::process::ExitCode;

const USAGE: &str = "\
almanac, a drift gate for agent discovery catalogs

Commands
  generate  Write the dual path catalog and the baseline from repo
            artifacts and the live surface
  validate  Check a catalog beyond schema, URN anchoring, entry
            shape, representative query counts, path coherence
  check     Compare published catalog, baseline and live surface
            and exit nonzero on drift at or above the fail severity
  migrate   Verify a legacy ai-catalog.json against the live
            surface and emit the dual path pair and the gate
";

const COMMANDS: [&str; 4] = ["generate", "validate", "check", "migrate"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
        Some("--help") | Some("-h") | Some("help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(cmd) if COMMANDS.contains(&cmd) => {
            eprintln!(
                "almanac {cmd} is not yet implemented. The design gate \
                 is confirmed and the first build milestone is the \
                 Streamable HTTP probe transport."
            );
            ExitCode::from(2)
        }
        Some(other) => {
            eprintln!("almanac does not know the command {other}\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

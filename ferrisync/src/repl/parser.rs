use anyhow::{bail, Context, Result};
use shlex::split as shlex_split;

use crate::commands::args::{SyncArgs, WatchArgs};
use crate::commands::DEFAULT_PORT;

use super::commands::ReplCommand;

/// Tokenize and parse one input line. Returns `Ok(None)` for blank input.
pub fn parse_line(line: &str) -> Result<Option<ReplCommand>> {
    let Some(words) = shlex_split(line.trim()) else {
        bail!("unbalanced quotes");
    };
    let Some((cmd, args)) = words.split_first() else {
        return Ok(None);
    };

    let command = match cmd.as_str() {
        "help" | "?" => ReplCommand::Help,
        "exit" | "quit" | "q" => ReplCommand::Exit,
        "y" | "yes" => ReplCommand::Yes,
        "n" | "no" => ReplCommand::No,
        "status" => ReplCommand::Status,
        "devices" => ReplCommand::Devices,
        "folders" => ReplCommand::Folders,
        "activity" => ReplCommand::Activity,
        "conflicts" => ReplCommand::Conflicts,
        "doctor" => ReplCommand::Doctor,
        "sessions" => ReplCommand::Sessions,
        "rename" => {
            let name = args.join(" ");
            if name.trim().is_empty() {
                bail!("usage: rename <new name> (e.g. rename Mr Desktop)");
            }
            ReplCommand::Rename { name }
        }
        "watches" => ReplCommand::Watches,
        "discover" => {
            let seconds = match args.first() {
                None => 3,
                Some(s) => s
                    .parse()
                    .with_context(|| format!("invalid duration '{s}' (expected seconds)"))?,
            };
            ReplCommand::Discover { seconds }
        }
        "pair" => {
            let ip = args
                .first()
                .context("usage: pair <ip> [--port <port>]")?
                .clone();
            let port = match flag_value(args, "--port")? {
                None => DEFAULT_PORT,
                Some(p) => p.parse().with_context(|| format!("invalid port '{p}'"))?,
            };
            ReplCommand::Pair { ip, port }
        }
        "add" => {
            let path = args
                .first()
                .context("usage: add <folder> [--name <name>]")?
                .clone();
            let mut name: Option<String> = None;
            let mut it = args.iter().skip(1);
            while let Some(tok) = it.next() {
                if tok == "--name" {
                    if name.is_some() {
                        bail!("duplicate --name");
                    }
                    name = Some(it.next().context("missing value for --name")?.clone());
                } else {
                    bail!("usage: add <folder> [--name <name>]");
                }
            }
            ReplCommand::Add { path, name }
        }
        "sync" => {
            let mut folder: Option<String> = None;
            let mut device: Option<String> = None;
            let mut wait: u64 = 0;
            let mut dry_run = false;
            let mut it = args.iter();
            while let Some(tok) = it.next() {
                if tok == "--device" {
                    if device.is_some() {
                        bail!("duplicate --device");
                    }
                    device = Some(it.next().context("missing value for --device")?.clone());
                } else if tok == "--wait" {
                    if wait != 0 {
                        bail!("duplicate --wait");
                    }
                    let v = it.next().context("missing value for --wait")?;
                    wait = v
                        .parse()
                        .with_context(|| format!("invalid wait seconds '{v}'"))?;
                } else if tok == "--dry-run" {
                    dry_run = true;
                } else if tok.starts_with("--") {
                    bail!("unknown flag '{tok}' for sync");
                } else if folder.is_none() {
                    folder = Some(tok.clone());
                } else {
                    bail!("unexpected argument '{tok}'");
                }
            }
            match (folder, device) {
                (None, None) => ReplCommand::Sync(SyncArgs {
                    folder: None,
                    device: None,
                    wait: 0,
                    dry_run,
                }),
                (Some(folder), Some(device)) => ReplCommand::Sync(SyncArgs {
                    folder: Some(folder),
                    device: Some(device),
                    wait,
                    dry_run,
                }),
                _ => bail!("usage: sync [<folder> --device <ip[:port]|name|uuid> [--wait secs] [--dry-run]]"),
            }
        }
        "unsync" => {
            let mut folder: Option<String> = None;
            let mut device: Option<String> = None;
            let mut yes = false;
            let mut it = args.iter();
            while let Some(tok) = it.next() {
                if tok == "--yes" {
                    if yes {
                        bail!("duplicate --yes");
                    }
                    yes = true;
                } else if tok == "--device" {
                    if device.is_some() {
                        bail!("duplicate --device");
                    }
                    device = Some(it.next().context("missing value for --device")?.clone());
                } else if tok.starts_with("--") {
                    bail!("unknown flag '{tok}' for unsync");
                } else if folder.is_none() {
                    folder = Some(tok.clone());
                } else {
                    bail!("unexpected argument '{tok}'");
                }
            }
            if yes && (folder.is_some() || device.is_some()) {
                bail!("--yes clears everything; it cannot be combined with a folder or --device");
            }
            ReplCommand::Unsync {
                folder,
                device,
                yes,
            }
        }
        "watch" => {
            let folder = args
                .first()
                .context("usage: watch <folder> --device <ip[:port]>")?
                .clone();
            let device = required_flag(args, "--device")?;
            ReplCommand::Watch(WatchArgs { folder, device })
        }
        "unwatch" => {
            let id = args
                .first()
                .context("usage: unwatch <id>")?
                .parse()
                .with_context(|| "watch id must be a number")?;
            ReplCommand::Unwatch { id }
        }
        "serve" => {
            let folder = args
                .first()
                .context("usage: serve <folder> [--port <port>]")?
                .clone();
            let port = match flag_value(args, "--port")? {
                None => DEFAULT_PORT,
                Some(p) => p.parse().with_context(|| format!("invalid port '{p}'"))?,
            };
            ReplCommand::Serve { folder, port }
        }
        "serves" => ReplCommand::Serves,
        "unserve" => {
            let id = args
                .first()
                .context("usage: unserve <id>")?
                .parse()
                .with_context(|| "server id must be a number")?;
            ReplCommand::Unserve { id }
        }
        "reset" => {
            let mut yes = false;
            for tok in args.iter() {
                if tok == "--yes" {
                    if yes {
                        bail!("duplicate --yes");
                    }
                    yes = true;
                } else {
                    bail!("usage: reset [--yes]");
                }
            }
            ReplCommand::Reset { yes }
        }
        "pendings" => ReplCommand::Pendings,
        "confirm" => {
            let n = args
                .first()
                .context("usage: confirm <n> (see 'pendings')")?
                .parse()
                .with_context(|| "pairing number must be a number")?;
            ReplCommand::Confirm { n }
        }
        "deny" => {
            let n = args
                .first()
                .context("usage: deny <n> (see 'pendings')")?
                .parse()
                .with_context(|| "pairing number must be a number")?;
            ReplCommand::Deny { n }
        }
        other => bail!("unknown command: {other} (try 'help')"),
    };
    Ok(Some(command))
}

fn flag_value(args: &[String], name: &str) -> Result<Option<String>> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => match args.get(i + 1) {
            Some(v) => Ok(Some(v.clone())),
            None => bail!("missing value for {name}"),
        },
    }
}

fn required_flag(args: &[String], name: &str) -> Result<String> {
    flag_value(args, name)?.with_context(|| format!("missing required flag {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Option<ReplCommand> {
        parse_line(line).unwrap()
    }

    #[test]
    fn blank_input_is_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
    }

    #[test]
    fn simple_commands() {
        assert_eq!(parse("help"), Some(ReplCommand::Help));
        assert_eq!(parse("?"), Some(ReplCommand::Help));
        assert_eq!(parse("status"), Some(ReplCommand::Status));
        assert_eq!(parse("devices"), Some(ReplCommand::Devices));
        assert_eq!(parse("folders"), Some(ReplCommand::Folders));
        assert_eq!(parse("activity"), Some(ReplCommand::Activity));
        assert_eq!(parse("conflicts"), Some(ReplCommand::Conflicts));
        assert_eq!(parse("doctor"), Some(ReplCommand::Doctor));
        assert_eq!(parse("exit"), Some(ReplCommand::Exit));
        assert_eq!(parse("quit"), Some(ReplCommand::Exit));
        assert_eq!(parse("q"), Some(ReplCommand::Exit));
        assert_eq!(parse("watches"), Some(ReplCommand::Watches));
    }

    #[test]
    fn unknown_command_errors() {
        assert!(parse_line("frobnicate").is_err());
    }

    #[test]
    fn comments_are_stripped() {
        assert_eq!(parse("#"), None);
        assert_eq!(parse("status # check state"), Some(ReplCommand::Status));
    }

    #[test]
    fn unbalanced_quotes_error() {
        assert!(parse_line("sync \"/home/x/My Docs").is_err());
    }

    #[test]
    fn quoted_paths_are_one_token() {
        assert_eq!(
            parse(r#"sync "/home/x/My Docs" --device 192.168.1.5"#),
            Some(ReplCommand::Sync(SyncArgs {
                folder: Some("/home/x/My Docs".into()),
                device: Some("192.168.1.5".into()),
                wait: 0,
                dry_run: false,
            }))
        );
    }

    #[test]
    fn add_parses_path_and_optional_name() {
        assert_eq!(
            parse("add ~/Documents"),
            Some(ReplCommand::Add {
                path: "~/Documents".into(),
                name: None,
            })
        );
        assert_eq!(
            parse("add ~/Documents --name Docs"),
            Some(ReplCommand::Add {
                path: "~/Documents".into(),
                name: Some("Docs".into()),
            })
        );
        assert!(parse_line("add").is_err());
        assert!(parse_line("add ~/x --bogus").is_err());
        assert!(parse_line("add ~/x --name").is_err());
        assert!(parse_line("add a b").is_err());
    }

    #[test]
    fn pair_defaults_and_port_flag() {
        assert_eq!(
            parse("pair 192.168.1.42"),
            Some(ReplCommand::Pair {
                ip: "192.168.1.42".into(),
                port: DEFAULT_PORT,
            })
        );
        assert_eq!(
            parse("pair 192.168.1.42 --port 9000"),
            Some(ReplCommand::Pair {
                ip: "192.168.1.42".into(),
                port: 9000,
            })
        );
        assert!(parse_line("pair").is_err());
        assert!(parse_line("pair 1.2.3.4 --port").is_err());
        assert!(parse_line("pair 1.2.3.4 --port abc").is_err());
    }

    #[test]
    fn sync_bare_and_explicit_forms() {
        assert_eq!(
            parse("sync"),
            Some(ReplCommand::Sync(SyncArgs {
                folder: None,
                device: None,
                wait: 0,
                dry_run: false,
            }))
        );
        assert_eq!(
            parse("sync ~/Documents --device 10.0.0.2:7000"),
            Some(ReplCommand::Sync(SyncArgs {
                folder: Some("~/Documents".into()),
                device: Some("10.0.0.2:7000".into()),
                wait: 0,
                dry_run: false,
            }))
        );
    }

    #[test]
    fn sync_wait_flag_parses() {
        assert_eq!(
            parse("sync test --device localhost --wait 60"),
            Some(ReplCommand::Sync(SyncArgs {
                folder: Some("test".into()),
                device: Some("localhost".into()),
                wait: 60,
                dry_run: false,
            }))
        );
        assert!(parse_line("sync test --device localhost --wait abc").is_err());
        assert!(parse_line("sync test --wait").is_err());
        assert!(matches!(
            parse("sync"),
            Some(ReplCommand::Sync(SyncArgs { wait: 0, .. }))
        ));
    }

    #[test]
    fn sync_dry_run_flag_parses() {
        assert_eq!(
            parse("sync ~/Documents --device mac --dry-run"),
            Some(ReplCommand::Sync(SyncArgs {
                folder: Some("~/Documents".into()),
                device: Some("mac".into()),
                wait: 0,
                dry_run: true,
            }))
        );
    }

    #[test]
    fn sync_rejects_partial_args() {
        assert!(parse_line("sync ~/Documents").is_err());
        assert!(parse_line("sync --device 10.0.0.2").is_err());
        assert!(parse_line("sync ~/Documents --device").is_err());
    }

    #[test]
    fn unsync_folder_with_optional_device() {
        assert_eq!(
            parse("unsync test"),
            Some(ReplCommand::Unsync {
                folder: Some("test".into()),
                device: None,
                yes: false,
            })
        );
        assert_eq!(
            parse("unsync test --device a5c13877"),
            Some(ReplCommand::Unsync {
                folder: Some("test".into()),
                device: Some("a5c13877".into()),
                yes: false,
            })
        );
        assert!(parse_line("unsync test extra").is_err());
        assert!(parse_line("unsync --device").is_err());
        assert!(parse_line("unsync --bogus").is_err());
    }

    #[test]
    fn unsync_full_reset_requires_confirmation() {
        assert_eq!(
            parse("unsync"),
            Some(ReplCommand::Unsync {
                folder: None,
                device: None,
                yes: false,
            })
        );
        assert_eq!(
            parse("unsync --yes"),
            Some(ReplCommand::Unsync {
                folder: None,
                device: None,
                yes: true,
            })
        );
        // --yes cannot be scoped.
        assert!(parse_line("unsync test --yes").is_err());
        assert!(parse_line("unsync --yes --yes").is_err());
    }

    #[test]
    fn watch_requires_device_flag() {
        assert!(parse_line("watch ~/Photos").is_err());
        assert_eq!(
            parse("watch ~/Photos --device 10.0.0.2"),
            Some(ReplCommand::Watch(WatchArgs {
                folder: "~/Photos".into(),
                device: "10.0.0.2".into(),
            }))
        );
    }

    #[test]
    fn discover_default_and_custom_seconds() {
        assert_eq!(
            parse("discover"),
            Some(ReplCommand::Discover { seconds: 3 })
        );
        assert_eq!(
            parse("discover 10"),
            Some(ReplCommand::Discover { seconds: 10 })
        );
        assert!(parse_line("discover soon").is_err());
    }

    #[test]
    fn unwatch_needs_a_number() {
        assert!(parse_line("unwatch abc").is_err());
        assert!(parse_line("unwatch").is_err());
        assert_eq!(parse("unwatch 3"), Some(ReplCommand::Unwatch { id: 3 }));
    }

    #[test]
    fn serve_defaults_and_port_flag() {
        assert_eq!(
            parse("serve ~/Sync"),
            Some(ReplCommand::Serve {
                folder: "~/Sync".into(),
                port: DEFAULT_PORT,
            })
        );
        assert_eq!(
            parse(r#"serve "~/My Docs" --port 7000"#),
            Some(ReplCommand::Serve {
                folder: "~/My Docs".into(),
                port: 7000,
            })
        );
        assert!(parse_line("serve").is_err());
        assert!(parse_line("serve ~/x --port").is_err());
        assert!(parse_line("serve ~/x --port abc").is_err());
    }

    #[test]
    fn serves_and_unserve() {
        assert_eq!(parse("serves"), Some(ReplCommand::Serves));
        assert!(parse_line("unserve").is_err());
        assert!(parse_line("unserve abc").is_err());
        assert_eq!(parse("unserve 2"), Some(ReplCommand::Unserve { id: 2 }));
    }

    #[test]
    fn pendings_confirm_deny() {
        assert_eq!(parse("pendings"), Some(ReplCommand::Pendings));
        assert_eq!(parse("confirm 1"), Some(ReplCommand::Confirm { n: 1 }));
        assert_eq!(parse("deny 3"), Some(ReplCommand::Deny { n: 3 }));
        assert!(parse_line("confirm").is_err());
        assert!(parse_line("confirm abc").is_err());
        assert!(parse_line("deny").is_err());
    }

    #[test]
    fn yes_no_shortcuts() {
        assert_eq!(parse("y"), Some(ReplCommand::Yes));
        assert_eq!(parse("yes"), Some(ReplCommand::Yes));
        assert_eq!(parse("n"), Some(ReplCommand::No));
        assert_eq!(parse("no"), Some(ReplCommand::No));
    }

    #[test]
    fn reset_requires_optional_yes_flag() {
        assert_eq!(parse("reset"), Some(ReplCommand::Reset { yes: false }));
        assert_eq!(parse("reset --yes"), Some(ReplCommand::Reset { yes: true }));
        assert!(parse_line("reset --bogus").is_err());
        assert!(parse_line("reset /some/path").is_err());
        assert!(parse_line("reset --yes --yes").is_err());
    }

    #[test]
    fn rename_parses_multiword_names() {
        assert_eq!(
            parse("rename Mr Desktop"),
            Some(ReplCommand::Rename {
                name: "Mr Desktop".into()
            })
        );
        assert_eq!(
            parse("rename   spaced   out  "),
            Some(ReplCommand::Rename {
                name: "spaced out".into()
            })
        );
    }

    #[test]
    fn bare_rename_is_an_error_with_usage_hint() {
        let err = parse_line("rename").unwrap_err();
        assert!(err.to_string().contains("usage: rename"));
        let err = parse_line("rename   ").unwrap_err();
        assert!(err.to_string().contains("usage: rename"));
    }
}

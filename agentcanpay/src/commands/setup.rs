use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use super::CommandError;
use crate::{cli::SetupArgs, output::Output};

/// The skill ships inside the binary rather than being fetched.
///
/// It is the one document that cannot regenerate itself, so tying it to the
/// binary is what keeps the two from disagreeing: the copy an agent reads is
/// always the one that was written for the commands it can actually call.
const SKILL: &str = include_str!("../../../SKILL.md");

const SKILL_NAME: &str = "agentcanpay";

/// Where the canonical copy lives, relative to the user's home directory.
///
/// `.agents/skills` is the cross-client convention: a client that follows it
/// discovers skills installed by any other, so one file serves all of them
/// and there is nothing to translate per agent.
const CANONICAL: &str = ".agents/skills";

/// Clients that scan only their own directory, and so need a link.
///
/// Hand-maintained, like the RPC table: anything that reads `.agents/skills`
/// needs no entry here, and an entry is only worth adding for a client whose
/// own path is documented. `marker` is the directory whose presence means
/// the client is installed — we never create it, because doing so would be
/// writing configuration for software the user does not have.
const CLIENTS: &[Client] = &[Client {
    name: "Claude Code",
    marker: ".claude",
    skills: ".claude/skills",
}];

struct Client {
    name: &'static str,
    marker: &'static str,
    skills: &'static str,
}

/// What happened at one client's path.
pub enum Action {
    Linked,
    Copied,
    AlreadyCurrent,
    NotDetected,
    Planned,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Linked => "linked",
            Self::Copied => "copied",
            Self::AlreadyCurrent => "already current",
            Self::NotDetected => "not detected",
            Self::Planned => "would link",
        }
    }
}

pub struct ClientResult {
    pub name: &'static str,
    pub path: PathBuf,
    pub action: Action,
}

pub fn run(args: &SetupArgs, out: &Output) -> Result<(), CommandError> {
    if args.print {
        print!("{SKILL}");
        return Ok(());
    }
    if args.list {
        return list(out);
    }
    install(args, out)
}

fn install(args: &SetupArgs, out: &Output) -> Result<(), CommandError> {
    let home = home()?;
    let dir = home.join(CANONICAL).join(SKILL_NAME);
    let file = dir.join("SKILL.md");

    if !args.dry_run {
        fs::create_dir_all(&dir).map_err(|e| skill_err(&dir, &e))?;
        fs::write(&file, SKILL).map_err(|e| skill_err(&file, &e))?;
    }

    let results = CLIENTS
        .iter()
        .map(|c| link_client(c, &home, &dir, args.dry_run))
        .collect::<Vec<_>>();

    out.setup_install(&file, &results, args.dry_run);
    Ok(())
}

fn list(out: &Output) -> Result<(), CommandError> {
    let home = home()?;
    let dir = home.join(CANONICAL).join(SKILL_NAME);

    let results = CLIENTS
        .iter()
        .map(|c| {
            let path = home.join(c.skills).join(SKILL_NAME);
            let action = if !home.join(c.marker).is_dir() {
                Action::NotDetected
            } else if path.exists() {
                Action::AlreadyCurrent
            } else {
                Action::Planned
            };
            ClientResult {
                name: c.name,
                path,
                action,
            }
        })
        .collect::<Vec<_>>();

    out.setup_list(&dir.join("SKILL.md"), &results);
    Ok(())
}

fn link_client(client: &Client, home: &Path, canonical: &Path, dry_run: bool) -> ClientResult {
    let path = home.join(client.skills).join(SKILL_NAME);

    let action = if home.join(client.marker).is_dir() {
        if dry_run {
            Action::Planned
        } else {
            point_at(canonical, &path).unwrap_or(Action::NotDetected)
        }
    } else {
        Action::NotDetected
    };

    ClientResult {
        name: client.name,
        path,
        action,
    }
}

/// Points a client's skill directory at the canonical one.
///
/// A path that is already a real directory is written into rather than
/// replaced: it carries this skill's name, but it may hold files a user put
/// there, and a link is not worth destroying them for.
fn point_at(canonical: &Path, path: &Path) -> io::Result<Action> {
    if path.is_symlink() {
        if path.read_link().is_ok_and(|t| t == canonical) {
            return Ok(Action::AlreadyCurrent);
        }
        fs::remove_file(path)?;
    } else if path.is_dir() {
        fs::write(path.join("SKILL.md"), SKILL)?;
        return Ok(Action::Copied);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    symlink_dir(canonical, path)?;
    Ok(Action::Linked)
}

/// Windows only creates symlinks for a process with developer mode or
/// elevation, so there it copies instead: two files that `update` rewrites
/// together beat an install that fails on an ordinary account.
#[cfg(windows)]
fn symlink_dir(_canonical: &Path, path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::write(path.join("SKILL.md"), SKILL)
}

#[cfg(unix)]
fn symlink_dir(canonical: &Path, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(canonical, path)
}

fn home() -> Result<PathBuf, CommandError> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| CommandError::Setup(format!("{var} is not set")))
}

fn skill_err(path: &Path, e: &io::Error) -> CommandError {
    CommandError::Setup(format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{Action, SKILL, SKILL_NAME, point_at};

    /// The embedded copy is what an agent ends up reading, so it has to be a
    /// valid skill: a client that cannot parse the frontmatter skips it
    /// entirely, and the failure is silent.
    #[test]
    fn the_embedded_skill_has_usable_frontmatter() {
        assert!(SKILL.starts_with("---\n"));
        let fm = SKILL
            .split("---")
            .nth(1)
            .expect("frontmatter is delimited by ---");
        assert!(fm.contains(&format!("name: {SKILL_NAME}")));

        let description = fm
            .split("description:")
            .nth(1)
            .expect("a skill without a description is never disclosed");
        assert!(!description.trim().is_empty());
        // Clients cap the description; over the limit it is rejected.
        assert!(description.len() < 1024);
    }

    /// The name is what the containing directory is called, and clients warn
    /// or skip when the two disagree.
    #[test]
    fn the_skill_name_is_a_valid_directory_name() {
        assert!(
            SKILL_NAME
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        );
        assert!(SKILL_NAME.len() <= 64);
    }

    #[cfg(unix)]
    #[test]
    fn linking_is_idempotent_and_never_eats_an_existing_directory() {
        let tmp = std::env::temp_dir().join(format!("acp-skill-{}", std::process::id()));
        let canonical = tmp.join("canonical");
        std::fs::create_dir_all(&canonical).expect("temp dir");

        // First run links, second run recognises its own link.
        let link = tmp.join("client");
        assert!(matches!(
            point_at(&canonical, &link).expect("link"),
            Action::Linked
        ));
        assert!(matches!(
            point_at(&canonical, &link).expect("relink"),
            Action::AlreadyCurrent
        ));

        // A real directory is written into, not replaced.
        let real = tmp.join("real");
        std::fs::create_dir_all(&real).expect("real dir");
        std::fs::write(real.join("notes.md"), "user file").expect("user file");
        assert!(matches!(
            point_at(&canonical, &real).expect("copy"),
            Action::Copied
        ));
        assert!(real.join("notes.md").exists());
        assert!(real.join("SKILL.md").exists());

        std::fs::remove_dir_all(&tmp).ok();
    }
}

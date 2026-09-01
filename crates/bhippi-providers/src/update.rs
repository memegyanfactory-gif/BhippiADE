//! Keeping vendor CLIs current without making the user wait for it.
//!
//! The sweep this replaces reinstalled every enabled CLI once a day whether or not
//! anything had changed. `npm install -g` on an already-current package still spends
//! thirty seconds resolving the tree, so the daily cost was minutes of background work
//! to achieve nothing, and a user who launched the app during it saw their machine busy
//! for no visible reason.
//!
//! Asking the registry what the latest version is costs about a second, so the check is
//! now: read the installed version, ask the registry, and only install when they differ.
//! Everything here fails soft — an unreachable registry means "leave it alone", never
//! "reinstall it to be safe", because reinstalling on an unknown is how a flaky network
//! turns into a reinstall on every launch.

use crate::catalog::{InstallSpec, ProviderSpec};
use crate::command::resolve_command;
use std::time::Duration;

/// How long the registry has to answer before the check is abandoned.
const QUERY_TIMEOUT: Duration = Duration::from_secs(20);

/// What a check concluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Installed and matching the registry — nothing to do.
    Current { version: String },
    /// A newer version exists.
    Stale { installed: String, latest: String },
    /// The question could not be answered; the caller must do nothing.
    Unknown { why: String },
}

impl Verdict {
    /// Whether an install is worth the minutes it takes.
    #[must_use]
    pub const fn should_install(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }
}

/// The package name an install recipe would fetch.
///
/// Recipes are explicit argv (INV-003), so the package is whichever argument is not a
/// flag and not the subcommand — for `npm install -g @anthropic-ai/claude-code` that is
/// the last element, but reading it positionally would break the first time a recipe
/// grows a trailing flag.
#[must_use]
pub fn package_of(recipe: &InstallSpec) -> Option<&'static str> {
    recipe
        .args
        .iter()
        .rev()
        .find(|arg| !arg.starts_with('-') && !matches!(**arg, "install" | "i" | "add" | "update"))
        .copied()
}

/// Compares the installed version against the registry's latest.
///
/// `installed` is whatever `--version` printed, which is rarely a bare semver — vendors
/// wrap it in their own words ("claude-code/2.1.246 win32") — so the comparison is on the
/// version-looking token inside it rather than on the whole string.
pub async fn check(spec: &ProviderSpec, installed: Option<&str>) -> Verdict {
    let Some(recipe) = spec.install else {
        return Verdict::Unknown {
            why: "nothing to install".to_owned(),
        };
    };
    let Some(installed) = installed.and_then(version_token) else {
        return Verdict::Unknown {
            why: "the installed version could not be read".to_owned(),
        };
    };
    let Some(package) = package_of(&recipe) else {
        return Verdict::Unknown {
            why: "the install recipe names no package".to_owned(),
        };
    };
    match latest(recipe.program, package).await {
        Some(latest) if latest == installed => Verdict::Current { version: installed },
        Some(latest) => Verdict::Stale { installed, latest },
        None => Verdict::Unknown {
            why: "the registry did not answer".to_owned(),
        },
    }
}

/// Asks the package manager what the newest published version is.
async fn latest(program: &str, package: &str) -> Option<String> {
    let resolved = resolve_command(program)?;
    let mut command = resolved.command();
    command.args(["view", package, "version"]);
    let output = tokio::time::timeout(QUERY_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    version_token(&String::from_utf8_lossy(&output.stdout))
}

/// The first thing in `text` that looks like a version number.
#[must_use]
pub fn version_token(text: &str) -> Option<String> {
    text.split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .find(|candidate| {
            let mut parts = candidate.split('.');
            // Two dots and three numeric parts is what every vendor here publishes; a
            // bare "2" out of some unrelated sentence must not be mistaken for one.
            parts.clone().count() >= 3 && parts.all(|part| !part.is_empty())
        })
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{check, package_of, version_token, Verdict};

    fn claude() -> &'static crate::catalog::ProviderSpec {
        crate::spec("claude").unwrap_or_else(|| panic!("the catalogue must know Claude Code"))
    }

    /// Vendors wrap their version in their own words; the comparison has to see through it.
    #[test]
    fn a_version_is_found_inside_whatever_the_vendor_wraps_it_in() {
        assert_eq!(version_token("2.1.246"), Some("2.1.246".to_owned()));
        assert_eq!(
            version_token("claude-code/2.1.246 win32-x64 node-v22.3.0"),
            Some("2.1.246".to_owned())
        );
        assert_eq!(
            version_token("codex-cli 0.48.1\n"),
            Some("0.48.1".to_owned())
        );
        // A lone integer is not a version, however tempting.
        assert_eq!(version_token("version 2"), None);
        assert_eq!(version_token("no numbers here"), None);
    }

    #[test]
    fn the_package_is_read_from_the_recipe_not_guessed() {
        let Some(recipe) = claude().install else {
            panic!("Claude Code must have an install recipe");
        };
        assert_eq!(package_of(&recipe), Some("@anthropic-ai/claude-code"));
        for entry in crate::CATALOG {
            if let Some(recipe) = entry.install {
                assert!(
                    package_of(&recipe).is_some(),
                    "{} names no package",
                    entry.id
                );
            }
        }
    }

    /// The failure mode that matters: an unreadable version must never be treated as
    /// stale, or a flaky check reinstalls every CLI on every launch.
    #[tokio::test]
    async fn an_unknown_version_is_never_treated_as_stale() {
        let verdict = check(claude(), None).await;
        assert!(matches!(verdict, Verdict::Unknown { .. }), "{verdict:?}");
        assert!(!verdict.should_install());

        let verdict = check(claude(), Some("who knows")).await;
        assert!(!verdict.should_install(), "{verdict:?}");
    }

    #[test]
    fn only_a_stale_verdict_is_worth_the_install() {
        assert!(Verdict::Stale {
            installed: "1.0.0".to_owned(),
            latest: "1.1.0".to_owned(),
        }
        .should_install());
        assert!(!Verdict::Current {
            version: "1.0.0".to_owned()
        }
        .should_install());
    }
}

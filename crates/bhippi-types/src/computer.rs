//! The bounds and the risk vocabulary of the Computer Use loop (ADR-0048).
//!
//! These live here rather than beside the loop for the reason ADR-0044 §4 gives: a cap that
//! is a `const` next to the code it bounds is a cap somebody edits while fixing something
//! else. They are measured in one place and changed in one place.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The hard cap on executed actions in one Computer Use turn.
///
/// Reaching it does not fail the turn: [`ADR-0048 §7`] requires one final summary round with
/// the vocabulary withdrawn, so the user gets the verified state rather than only the word
/// "stopped".
pub const COMPUTER_MAX_ACTIONS_PER_TURN: usize = 24;

/// How many consecutive unexecutable replies the loop will correct before giving up.
///
/// A repair costs one provider round and does nothing to the machine, so it does not consume
/// the action budget. Three is enough for a model that mistyped a verb and not enough for one
/// that has lost the protocol entirely.
pub const COMPUTER_MAX_REPAIRS: usize = 3;

/// Rounds of observation carried verbatim. Older rounds collapse to one line each, so a
/// long run does not re-send the same boilerplate twenty times.
pub const COMPUTER_VERBATIM_ROUNDS: usize = 2;

/// Longest wait for the screen to stop changing after an action, in milliseconds.
pub const COMPUTER_SETTLE_TIMEOUT_MS: u64 = 2_500;

/// Gap between settle probes, in milliseconds.
pub const COMPUTER_SETTLE_INTERVAL_MS: u64 = 220;

/// Longest a `wait` action may ask for, in milliseconds.
pub const COMPUTER_MAX_WAIT_MS: u32 = 10_000;

/// One continuous stroke stays bounded, even with a large model-generated path.
pub const COMPUTER_MAX_PATH_POINTS: usize = 128;
pub const COMPUTER_MAX_PATH_DURATION_MS: u32 = 4_000;
pub const COMPUTER_PATH_SAMPLE_MS: u32 = 8;
pub const COMPUTER_DRAG_DURATION_MS: u32 = 500;

/// Longest reason string kept from a model. A reason is a clause, not a paragraph.
pub const COMPUTER_MAX_REASON_CHARS: usize = 120;

/// What one action can do, which decides whether it needs an answer from the user first.
///
/// The classification is exhaustive over the action vocabulary by construction: the
/// `class()` match in `bhippi-app::computer` has no wildcard arm, so a new action cannot be
/// added without deciding what it costs.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ComputerActionClass {
    /// Looks and reports. Never gated: refusing to let a model see the screen it was given
    /// is not a safety property, it is a broken loop.
    Observe,
    /// Moves the pointer, clicks, types or scrolls. Gated on Full PC Access; with it off the
    /// loop asks rather than failing.
    Input,
    /// Reaches outside the current window — starts a program, opens a URL, brings another
    /// window forward, or presses a chord that closes or runs something. Always confirmed
    /// once per target, even with Full PC Access on.
    Consequential,
}

impl ComputerActionClass {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Input => "input",
            Self::Consequential => "consequential",
        }
    }

    /// True when this class may run with no answer from the user, given Full PC Access.
    #[must_use]
    pub const fn allowed_with_full_access(self) -> bool {
        matches!(self, Self::Observe | Self::Input)
    }
}

/// Which surface a Computer Use turn is aimed at (ADR-0048 §Scope).
///
/// This is not a runtime flag on one vocabulary: [`Self::GameWindow`] has a strictly smaller
/// set of legal actions, and the parser refuses the rest. INV-089's "no code path from an
/// engine observation to a desktop-wide action" is therefore a property of the type rather
/// than a branch somebody can forget to write.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ComputerScope {
    /// The whole virtual desktop: the `/computer` command and the ADR-0018 gate.
    #[default]
    Desktop,
    /// One window Bhippi launched — the game (ADR-0044). No verb in this scope can name
    /// another window, a program or a URL.
    GameWindow,
}

impl ComputerScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::GameWindow => "game_window",
        }
    }

    /// True when a verb that names something outside the target surface is legal here.
    #[must_use]
    pub const fn allows_reaching_out(self) -> bool {
        matches!(self, Self::Desktop)
    }
}

/// How a Computer Use turn ended, as the report states it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ComputerOutcome {
    /// The model said the task was done and said what it observed.
    Completed,
    /// The user answered no to a gate. Not a failure: they were asked and they decided.
    Declined,
    /// The action budget ran out. The turn carries the verified state and stays unresolved.
    CapReached,
    /// The model could not produce an executable action within the repair budget.
    ProtocolLost,
    /// Esc/Esc or the Stop button.
    Stopped,
    /// Something under the loop failed — a capture, an injection, the provider.
    Failed,
}

impl ComputerOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Declined => "declined",
            Self::CapReached => "cap_reached",
            Self::ProtocolLost => "protocol_lost",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    /// True when the turn may present itself as having achieved the task.
    ///
    /// Only one variant may. ADR-0044 §4 is explicit that a cap must never end in a success
    /// claim, and the same reasoning covers a lost protocol and a decline.
    #[must_use]
    pub const fn claims_success(self) -> bool {
        matches!(self, Self::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_completed_turn_may_claim_success() {
        for outcome in [
            ComputerOutcome::Declined,
            ComputerOutcome::CapReached,
            ComputerOutcome::ProtocolLost,
            ComputerOutcome::Stopped,
            ComputerOutcome::Failed,
        ] {
            assert!(
                !outcome.claims_success(),
                "{} must not read as success",
                outcome.as_str()
            );
        }
        assert!(ComputerOutcome::Completed.claims_success());
    }

    #[test]
    fn a_consequential_action_is_confirmed_even_with_full_access() {
        assert!(ComputerActionClass::Observe.allowed_with_full_access());
        assert!(ComputerActionClass::Input.allowed_with_full_access());
        assert!(!ComputerActionClass::Consequential.allowed_with_full_access());
    }

    #[test]
    fn only_the_desktop_scope_may_reach_outside_its_surface() {
        assert!(ComputerScope::Desktop.allows_reaching_out());
        assert!(!ComputerScope::GameWindow.allows_reaching_out());
    }

    /// These hold at compile time, which is the right place for a relationship between two
    /// constants: someone lowering the action cap below the repair budget should not get a
    /// red test, they should get a build that refuses.
    #[test]
    fn the_budgets_are_bounded_and_ordered() {
        const {
            assert!(COMPUTER_MAX_ACTIONS_PER_TURN > 0);
            // A repair costs a round and does nothing; it stays far cheaper than the run.
            assert!(COMPUTER_MAX_REPAIRS < COMPUTER_MAX_ACTIONS_PER_TURN);
            assert!(COMPUTER_VERBATIM_ROUNDS >= 1);
            assert!(COMPUTER_SETTLE_INTERVAL_MS < COMPUTER_SETTLE_TIMEOUT_MS);
        }
    }
}

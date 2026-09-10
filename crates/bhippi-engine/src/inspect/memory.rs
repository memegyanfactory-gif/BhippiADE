//! What the Inspector remembers between scans (ADR-0056 §9, §23–§26).
//!
//! Without memory an inspector is a stranger every morning: it reports the thing you decided
//! not to care about, it cannot tell you that yesterday's bug is fixed, and "what did my
//! change break" is a question it cannot answer. The ledger is the smallest thing that fixes
//! all three — one row per finding identity, with when it was first seen, when it was last
//! seen, and whether a person has told it to stop.
//!
//! It stores **no finding text**. A row is an id, a code and three timestamps; the wording,
//! the evidence and the severity all come from the scan that is running now. A ledger that
//! cached descriptions would keep showing yesterday's explanation of today's problem.

use bhippi_types::FindingStatus;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

use super::finding::Finding;

/// The ledger's own schema, so an older file is refused rather than half-read.
pub const LEDGER_SCHEMA: &str = "bhippi-inspect-ledger@1";

/// One remembered finding identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct LedgerEntry {
    pub id: String,
    pub code: String,
    pub status: FindingStatus,
    /// RFC 3339, the first scan that saw it.
    pub first_seen: String,
    /// RFC 3339, the most recent scan that saw it.
    pub last_seen: String,
    /// RFC 3339, when a scan stopped seeing it. Cleared if it comes back.
    #[serde(default)]
    pub resolved_at: Option<String>,
}

/// Everything remembered about one project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct FindingLedger {
    pub schema: String,
    pub entries: Vec<LedgerEntry>,
}

impl Default for FindingLedger {
    fn default() -> Self {
        Self {
            schema: LEDGER_SCHEMA.to_owned(),
            entries: Vec::new(),
        }
    }
}

impl FindingLedger {
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LedgerEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Mark one finding ignored. The only status a person sets by hand.
    pub fn ignore(&mut self, id: &str, code: &str, now: &str) {
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.status = FindingStatus::Ignored;
                entry.resolved_at = None;
            }
            None => self.entries.push(LedgerEntry {
                id: id.to_owned(),
                code: code.to_owned(),
                status: FindingStatus::Ignored,
                first_seen: now.to_owned(),
                last_seen: now.to_owned(),
                resolved_at: None,
            }),
        }
        self.entries.sort_by(|left, right| left.id.cmp(&right.id));
    }

    /// Stop ignoring one finding.
    pub fn unignore(&mut self, id: &str) -> bool {
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) if entry.status == FindingStatus::Ignored => {
                entry.status = FindingStatus::Open;
                true
            }
            _ => false,
        }
    }
}

/// What changed between the last scan and this one.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Changes {
    /// Findings this scan saw for the first time.
    pub new_ids: Vec<String>,
    /// Findings that were resolved and have come back.
    pub returned_ids: Vec<String>,
    /// Findings the previous scan saw and this one does not, with the date they went away.
    pub resolved: Vec<LedgerEntry>,
}

impl Changes {
    /// True when nothing at all moved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.new_ids.is_empty() && self.returned_ids.is_empty() && self.resolved.is_empty()
    }

    /// The line the drawer prints after an *Inspect changes* run (§26).
    #[must_use]
    pub fn summary(&self) -> String {
        if self.is_empty() {
            return "Nothing changed since the last scan.".to_owned();
        }
        let mut parts = Vec::new();
        if !self.new_ids.is_empty() {
            parts.push(format!("{} new", self.new_ids.len()));
        }
        if !self.returned_ids.is_empty() {
            parts.push(format!("{} returned", self.returned_ids.len()));
        }
        if !self.resolved.is_empty() {
            parts.push(format!("{} resolved", self.resolved.len()));
        }
        parts.join(" · ")
    }
}

/// A scan's findings, reconciled against what the project already remembered.
#[derive(Clone, Debug)]
pub struct Reconciled {
    /// The findings to report and to score. Ignored ones are not in here.
    pub findings: Vec<Finding>,
    /// The findings a person has told the Inspector to stop raising.
    pub ignored: Vec<Finding>,
    pub ledger: FindingLedger,
    pub changes: Changes,
}

/// Fold a scan's findings into the ledger.
///
/// `now` is RFC 3339 and is supplied by the caller: this crate does not read the clock, so a
/// test can assert on the exact dates a reconcile writes.
#[must_use]
pub fn reconcile(ledger: &FindingLedger, findings: Vec<Finding>, now: &str) -> Reconciled {
    let mut entries: BTreeMap<String, LedgerEntry> = ledger
        .entries
        .iter()
        .map(|entry| (entry.id.clone(), entry.clone()))
        .collect();
    let mut changes = Changes::default();
    let seen: Vec<String> = findings.iter().map(|finding| finding.id.clone()).collect();

    let mut open = Vec::new();
    let mut ignored = Vec::new();
    for mut finding in findings {
        match entries.get_mut(&finding.id) {
            Some(entry) => {
                entry.last_seen = now.to_owned();
                if entry.status == FindingStatus::Resolved {
                    // It came back. That is worth saying out loud (§24).
                    entry.status = FindingStatus::Open;
                    entry.resolved_at = None;
                    changes.returned_ids.push(finding.id.clone());
                }
                finding.status = entry.status;
            }
            None => {
                changes.new_ids.push(finding.id.clone());
                entries.insert(
                    finding.id.clone(),
                    LedgerEntry {
                        id: finding.id.clone(),
                        code: finding.code.clone(),
                        status: FindingStatus::Open,
                        first_seen: now.to_owned(),
                        last_seen: now.to_owned(),
                        resolved_at: None,
                    },
                );
            }
        }
        if finding.status == FindingStatus::Ignored {
            ignored.push(finding);
        } else {
            open.push(finding);
        }
    }

    // Anything the ledger knows and this scan did not see is fixed — unless a person had
    // already ignored it, in which case its absence says nothing about whether it was fixed.
    for entry in entries.values_mut() {
        if seen.contains(&entry.id) || entry.status == FindingStatus::Ignored {
            continue;
        }
        if entry.status != FindingStatus::Resolved {
            entry.status = FindingStatus::Resolved;
            entry.resolved_at = Some(now.to_owned());
            changes.resolved.push(entry.clone());
        }
    }

    changes.new_ids.sort();
    changes.returned_ids.sort();
    changes
        .resolved
        .sort_by(|left, right| left.id.cmp(&right.id));

    Reconciled {
        findings: open,
        ignored,
        ledger: FindingLedger {
            schema: LEDGER_SCHEMA.to_owned(),
            entries: entries.into_values().collect(),
        },
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::finding::{Finding, Location};
    use bhippi_types::{InspectorId, Severity};

    const DAY_ONE: &str = "2026-09-09T10:00:00Z";
    const DAY_TWO: &str = "2026-09-10T10:00:00Z";

    fn finding(code: &str, node: &str) -> Finding {
        Finding::draft(
            InspectorId::Gameplay,
            code,
            Severity::High,
            90,
            "something",
            Location::node("scenes/main.tscn", node),
        )
        .cause("c")
        .impact("i")
        .recommend("r")
        .build()
        .expect("the draft is complete")
    }

    #[test]
    fn a_first_scan_records_everything_as_new() {
        let door = finding("BHP-INS-302", "Door");
        let result = reconcile(&FindingLedger::default(), vec![door.clone()], DAY_ONE);
        assert_eq!(result.changes.new_ids, vec![door.id.clone()]);
        assert!(result.changes.resolved.is_empty());
        let entry = result
            .ledger
            .get(&door.id)
            .expect("the ledger remembers it");
        assert_eq!(entry.first_seen, DAY_ONE);
        assert_eq!(entry.status, FindingStatus::Open);
    }

    #[test]
    fn a_finding_that_goes_away_is_resolved_with_the_date_it_went_away() {
        let door = finding("BHP-INS-302", "Door");
        let first = reconcile(&FindingLedger::default(), vec![door.clone()], DAY_ONE);
        let second = reconcile(&first.ledger, Vec::new(), DAY_TWO);

        assert_eq!(second.changes.resolved.len(), 1);
        assert_eq!(
            second.changes.resolved[0].resolved_at.as_deref(),
            Some(DAY_TWO)
        );
        assert_eq!(second.changes.summary(), "1 resolved");

        // And a third scan does not report the same resolution again.
        let third = reconcile(&second.ledger, Vec::new(), DAY_TWO);
        assert!(third.changes.is_empty());
        assert_eq!(
            third.changes.summary(),
            "Nothing changed since the last scan."
        );
    }

    #[test]
    fn a_problem_that_comes_back_comes_back_as_the_same_row_and_says_so() {
        let door = finding("BHP-INS-302", "Door");
        let first = reconcile(&FindingLedger::default(), vec![door.clone()], DAY_ONE);
        let gone = reconcile(&first.ledger, Vec::new(), DAY_TWO);
        let back = reconcile(&gone.ledger, vec![door.clone()], DAY_TWO);

        assert_eq!(back.changes.returned_ids, vec![door.id.clone()]);
        assert!(back.changes.new_ids.is_empty());
        let entry = back.ledger.get(&door.id).expect("the ledger remembers it");
        assert_eq!(entry.status, FindingStatus::Open);
        assert_eq!(entry.resolved_at, None);
        assert_eq!(
            entry.first_seen, DAY_ONE,
            "the first sighting is not rewritten"
        );
    }

    #[test]
    fn an_ignored_finding_stays_out_of_the_reported_set_and_is_never_called_resolved() {
        let door = finding("BHP-INS-302", "Door");
        let mut ledger = FindingLedger::default();
        ledger.ignore(&door.id, &door.code, DAY_ONE);

        let result = reconcile(&ledger, vec![door.clone()], DAY_TWO);
        assert!(result.findings.is_empty());
        assert_eq!(result.ignored.len(), 1);
        assert_eq!(result.ignored[0].status, FindingStatus::Ignored);

        // It disappears from the project; that is not a fix anybody made.
        let later = reconcile(&result.ledger, Vec::new(), DAY_TWO);
        assert!(later.changes.resolved.is_empty());
    }

    #[test]
    fn un_ignoring_puts_it_back_in_the_reported_set() {
        let door = finding("BHP-INS-302", "Door");
        let mut ledger = FindingLedger::default();
        ledger.ignore(&door.id, &door.code, DAY_ONE);
        assert!(ledger.unignore(&door.id));
        assert!(!ledger.unignore("f_nothing"));

        let result = reconcile(&ledger, vec![door], DAY_TWO);
        assert_eq!(result.findings.len(), 1);
        assert!(result.ignored.is_empty());
    }

    #[test]
    fn the_summary_names_all_three_kinds_of_movement() {
        let changes = Changes {
            new_ids: vec!["a".to_owned(), "b".to_owned()],
            returned_ids: vec!["c".to_owned()],
            resolved: Vec::new(),
        };
        assert_eq!(changes.summary(), "2 new · 1 returned");
    }
}

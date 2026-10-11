//! Decision history, independent of CLI parsing and derived analysis facts.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionChange {
    Name {
        #[serde(with = "hex_scalar")]
        address: u64,
        before: Option<String>,
        after: Option<String>,
    },
    Comment {
        #[serde(with = "hex_scalar")]
        address: u64,
        before: Option<Comment>,
        after: Option<Comment>,
    },
    Signature {
        #[serde(with = "hex_scalar")]
        address: u64,
        before: Option<Signature>,
        after: Option<Signature>,
    },
    Breakpoints {
        #[serde(with = "hex_list")]
        before: Vec<u64>,
        #[serde(with = "hex_list")]
        after: Vec<u64>,
    },
    Watchpoints {
        before: Vec<Watch>,
        after: Vec<Watch>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: u64,
    pub changes: Vec<DecisionChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditHistory {
    pub(crate) revisions: Vec<Revision>,
    pub(crate) cursor: usize,
    pub(crate) next_id: u64,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self {
            revisions: Vec::new(),
            cursor: 0,
            next_id: 1,
        }
    }
}

impl EditHistory {
    pub fn is_empty(&self) -> bool {
        self.revisions.is_empty()
    }
    pub fn revisions(&self) -> &[Revision] {
        &self.revisions
    }
    /// Number of currently applied revisions; the remainder can be redone.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.cursor > self.revisions.len()
            || self.next_id == 0
            || self
                .revisions
                .iter()
                .any(|r| r.id == 0 || r.id >= self.next_id || r.changes.is_empty())
            || self
                .revisions
                .windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(ProjectError::History(
                "invalid revision order or cursor".into(),
            ));
        }
        Ok(())
    }
}

impl Project {
    /// Compare decisions only, from `self` to `other`. Paths and history are
    /// provenance, not decisions. Different binary identities cannot compare.
    pub fn diff(&self, other: &Self) -> Result<Vec<DecisionChange>, ProjectError> {
        if self.binary_hash != other.binary_hash {
            return Err(ProjectError::WrongBinary {
                expected: self.binary_hash.clone(),
                actual: other.binary_hash.clone(),
            });
        }
        let mut changes = Vec::new();
        macro_rules! map_changes {
            ($field:ident, $variant:ident) => {
                for address in self
                    .$field
                    .keys()
                    .chain(other.$field.keys())
                    .copied()
                    .collect::<BTreeSet<_>>()
                {
                    let before = self.$field.get(&address).cloned();
                    let after = other.$field.get(&address).cloned();
                    if before != after {
                        changes.push(DecisionChange::$variant {
                            address,
                            before,
                            after,
                        });
                    }
                }
            };
        }
        map_changes!(names, Name);
        map_changes!(comments, Comment);
        map_changes!(signatures, Signature);
        if self.breakpoints != other.breakpoints {
            changes.push(DecisionChange::Breakpoints {
                before: self.breakpoints.clone(),
                after: other.breakpoints.clone(),
            });
        }
        if self.watchpoints != other.watchpoints {
            changes.push(DecisionChange::Watchpoints {
                before: self.watchpoints.clone(),
                after: other.watchpoints.clone(),
            });
        }
        Ok(changes)
    }

    /// Record one completed edit. An unchanged edit preserves the redo branch.
    /// The caller holds ProjectLock across read/edit/record/write.
    pub fn record_edit(&mut self, before: &Self) -> Result<Option<u64>, ProjectError> {
        self.history.validate()?;
        if self.history != before.history {
            return Err(ProjectError::History(
                "history changed during an edit".into(),
            ));
        }
        let changes = before.diff(self)?;
        if changes.is_empty() {
            return Ok(None);
        }
        let id = self.history.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| ProjectError::History("revision id exhausted".into()))?;
        self.history.revisions.truncate(self.history.cursor);
        self.history.revisions.push(Revision { id, changes });
        self.history.cursor += 1;
        self.history.next_id = next_id;
        self.schema_version = SCHEMA_VERSION;
        Ok(Some(id))
    }

    pub fn undo(&mut self) -> Result<u64, ProjectError> {
        self.move_history(false)
    }
    pub fn redo(&mut self) -> Result<u64, ProjectError> {
        self.move_history(true)
    }

    fn move_history(&mut self, forward: bool) -> Result<u64, ProjectError> {
        self.history.validate()?;
        let index = if forward {
            self.history.cursor
        } else {
            self.history
                .cursor
                .checked_sub(1)
                .ok_or_else(|| ProjectError::History("nothing to undo".into()))?
        };
        let revision = self
            .history
            .revisions
            .get(index)
            .ok_or_else(|| ProjectError::History("nothing to redo".into()))?
            .clone();
        // Preflight the entire revision on a copy. A manual edit must never
        // cause an undo to overwrite it, nor leave a partially applied batch.
        let mut trial = self.clone();
        for change in &revision.changes {
            trial.apply_change(change, forward)?;
        }
        trial.history.cursor = if forward { index + 1 } else { index };
        trial.schema_version = SCHEMA_VERSION;
        *self = trial;
        Ok(revision.id)
    }

    fn apply_change(&mut self, change: &DecisionChange, forward: bool) -> Result<(), ProjectError> {
        fn sides<'a, T>(before: &'a T, after: &'a T, forward: bool) -> (&'a T, &'a T) {
            if forward {
                (before, after)
            } else {
                (after, before)
            }
        }
        fn replace<T: Clone + PartialEq>(
            current: &mut T,
            expected: &T,
            next: &T,
        ) -> Result<(), ProjectError> {
            if current != expected {
                return Err(ProjectError::History(
                    "decision was edited outside this revision; undo/redo refused".into(),
                ));
            }
            *current = next.clone();
            Ok(())
        }
        macro_rules! map_change {
            ($field:ident, $addr:expr, $before:expr, $after:expr) => {{
                let (expected, next) = sides($before, $after, forward);
                let mut current = self.$field.get($addr).cloned();
                replace(&mut current, expected, next)?;
                if let Some(value) = current {
                    self.$field.insert(*$addr, value);
                } else {
                    self.$field.remove($addr);
                }
            }};
        }
        match change {
            DecisionChange::Name {
                address,
                before,
                after,
            } => map_change!(names, address, before, after),
            DecisionChange::Comment {
                address,
                before,
                after,
            } => map_change!(comments, address, before, after),
            DecisionChange::Signature {
                address,
                before,
                after,
            } => map_change!(signatures, address, before, after),
            DecisionChange::Breakpoints { before, after } => {
                let (expected, next) = sides(before, after, forward);
                replace(&mut self.breakpoints, expected, next)?;
            }
            DecisionChange::Watchpoints { before, after } => {
                let (expected, next) = sides(before, after, forward);
                replace(&mut self.watchpoints, expected, next)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Project {
        serde_json::from_str(r#"{"schema_version":1,"binary_hash":"fixture"}"#).unwrap()
    }

    #[test]
    fn history_roundtrip_undo_redo_preserves_all_decisions() {
        let original = empty();
        let mut project = original.clone();
        project.set_name(0x401000, "parse");
        project.set_comment(0x401008, "bounds check");
        project.set_signature(
            0x401000,
            signature_syntax::parse("int (char *buf)").unwrap(),
        );
        project.add_breakpoint(0x401008);
        project.add_watchpoint(Watch {
            address: 0x500000,
            size: 8,
            on_read: true,
            on_write: false,
        });
        assert_eq!(project.record_edit(&original).unwrap(), Some(1));
        assert_eq!(project.schema_version, 2);
        assert_eq!(project.history.revisions()[0].changes.len(), 5);
        let saved = project.clone();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.json");
        project.write(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"address\": \"0x401000\""));
        project = Project::read(&path).unwrap().unwrap();
        assert_eq!(project, saved);
        assert_eq!(project.undo().unwrap(), 1);
        assert!(project.diff(&original).unwrap().is_empty());
        project.write(&path).unwrap();
        project = Project::read(&path).unwrap().unwrap();
        assert_eq!(project.redo().unwrap(), 1);
        assert_eq!(project, saved);
    }

    #[test]
    fn no_op_keeps_redo_and_a_new_edit_discards_it_without_reusing_ids() {
        let mut project = empty();
        for name in ["first", "second"] {
            let before = project.clone();
            project.set_name(0x401000, name);
            project.record_edit(&before).unwrap();
        }
        assert_eq!(project.undo().unwrap(), 2);
        let before = project.clone();
        assert_eq!(project.record_edit(&before).unwrap(), None);
        assert_eq!(project.redo().unwrap(), 2);
        assert_eq!(project.undo().unwrap(), 2);
        let before = project.clone();
        project.set_comment(0x401008, "new branch");
        assert_eq!(project.record_edit(&before).unwrap(), Some(3));
        assert_eq!(
            project
                .history
                .revisions()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(project.redo().is_err());
        assert_eq!(project.undo().unwrap(), 3);
        assert_eq!(project.undo().unwrap(), 1);
        assert!(project.undo().is_err());
        assert!(project.names.is_empty());
    }

    #[test]
    fn a_conflict_in_a_later_change_does_not_partially_undo() {
        let mut project = empty();
        let before = project.clone();
        project.set_name(0x401000, "parse");
        project.set_comment(0x401008, "recorded");
        project.record_edit(&before).unwrap();
        project.set_comment(0x401008, "manual edit");
        let conflicted = project.clone();
        assert!(project.undo().is_err());
        assert_eq!(project, conflicted);
    }

    #[test]
    fn redo_conflicts_and_history_changes_during_edit_are_refused() {
        let mut project = empty();
        let original = project.clone();
        project.set_name(0x401000, "parse");
        project.record_edit(&original).unwrap();
        project.undo().unwrap();
        project.set_name(0x401000, "manual");
        let before = project.clone();
        assert!(project.redo().is_err());
        assert_eq!(project, before);
        project.history.cursor = 1;
        assert!(project.record_edit(&before).is_err());
    }

    #[test]
    fn removal_and_legacy_stop_order_are_reversible() {
        let mut project = empty();
        project.set_name(0x401000, "legacy");
        project.breakpoints = vec![3, 1, 3];
        let before = project.clone();
        project.clear_name(0x401000);
        project.remove_breakpoint(3);
        project.record_edit(&before).unwrap();
        let after = project.clone();
        project.undo().unwrap();
        assert!(project.diff(&before).unwrap().is_empty());
        project.redo().unwrap();
        assert_eq!(project, after);
    }

    #[test]
    fn diff_is_deterministic_and_requires_the_same_binary() {
        let mut original = empty();
        original.set_name(8, "old");
        let mut changed = original.clone();
        changed.binary_path = "moved.bin".into();
        assert!(original.diff(&changed).unwrap().is_empty());
        changed.set_name(8, "new");
        changed.set_name(4, "added");
        let diff = original.diff(&changed).unwrap();
        assert!(matches!(diff[0], DecisionChange::Name { address: 4, .. }));
        assert!(matches!(diff[1], DecisionChange::Name { address: 8, .. }));
        assert_eq!(original.diff(&changed).unwrap(), diff);
        changed.binary_hash = "other".into();
        assert!(matches!(
            original.diff(&changed),
            Err(ProjectError::WrongBinary { .. })
        ));
    }

    #[test]
    fn malformed_history_is_rejected_on_read_and_write() {
        let mut project = empty();
        let before = project.clone();
        project.set_name(8, "new");
        project.record_edit(&before).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.json");
        for (cursor, id, next_id) in [(2, 1, 2), (1, 0, 2), (1, 2, 2), (1, 1, 0)] {
            let mut invalid = project.clone();
            invalid.history.cursor = cursor;
            invalid.history.revisions[0].id = id;
            invalid.history.next_id = next_id;
            assert!(invalid.write(&path).is_err());
            assert!(!path.exists());
            std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(matches!(
                Project::read(&path),
                Err(ProjectError::History(_))
            ));
            std::fs::remove_file(&path).unwrap();
        }
    }
}

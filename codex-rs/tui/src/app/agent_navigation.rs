//! Multi-agent picker navigation and labeling state for the TUI app.
//!
//! This module exists to keep the pure parts of multi-agent navigation out of [`crate::app::App`].
//! It owns the stable spawn-order cache used by the `/subagents` picker, keyboard next/previous
//! navigation, and the contextual footer label for the thread currently being watched.
//!
//! Responsibilities here are intentionally narrow:
//! - remember picker entries and their first-seen order
//! - remember which V2 child threads are owned by their parent agent
//! - answer traversal questions like "what is the next thread?"
//! - derive user-facing picker/footer text from cached thread metadata
//!
//! Responsibilities that stay in `App`:
//! - discovering threads from the backend
//! - deciding which thread is currently displayed
//! - mutating UI state such as switching threads or updating the footer widget
//!
//! The key invariant is that traversal follows first-seen spawn order rather than thread-id sort
//! order. Once a thread id is observed it keeps its place in the cycle even if the entry is later
//! updated or marked closed.

use crate::bottom_pane::AgentSelectorEntry;
use crate::multi_agents::AgentPickerThreadEntry;
use crate::multi_agents::SubAgentActivityDisplay;
use crate::multi_agents::format_agent_picker_item_name;
use codex_protocol::ThreadId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;
use uuid::Uuid;

/// Small state container for multi-agent picker ordering and labeling.
///
/// `App` owns thread lifecycle and UI side effects. This type keeps the pure rules for stable
/// spawn-order traversal, picker copy, and active-agent labels together and separately testable.
///
/// The core invariant is that `order` records first-seen thread ids exactly once, while `threads`
/// stores the latest metadata for those ids. Mutation is intentionally funneled through `upsert`,
/// `mark_closed`, and `clear` so those two collections do not drift semantically even if they are
/// temporarily out of sync during teardown races.
#[derive(Debug, Default)]
pub(crate) struct AgentNavigationState {
    /// Latest picker metadata for each tracked thread id.
    threads: HashMap<ThreadId, AgentPickerThreadEntry>,
    /// Stable first-seen traversal order for picker rows and keyboard cycling.
    order: Vec<ThreadId>,
    /// Threads with observed terminal liveness that must not be revived by delayed activity.
    stopped_threads: HashSet<ThreadId>,
    /// Latest TUI-observed running, idle, or closed transition for each thread.
    status_changed_at: HashMap<ThreadId, Instant>,
    /// Spawned child threads whose instructions are owned by their parent agent.
    parent_owned_threads: HashSet<ThreadId>,
    /// Coalesces root refreshes while rejecting replies from a previous session.
    picker_refresh: Option<(ThreadId, Uuid)>,
}

/// Direction of keyboard traversal through the stable picker order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentNavigationDirection {
    /// Move toward the entry that was seen earlier in spawn order, wrapping at the front.
    Previous,
    /// Move toward the entry that was seen later in spawn order, wrapping at the end.
    Next,
}

impl AgentNavigationState {
    pub(crate) fn begin_picker_refresh(&mut self, thread_id: ThreadId) -> Option<Uuid> {
        if self.picker_refresh.is_some() {
            return None;
        }
        let request_id = Uuid::new_v4();
        self.picker_refresh = Some((thread_id, request_id));
        Some(request_id)
    }

    pub(crate) fn finish_picker_refresh(&mut self, thread_id: ThreadId, request_id: Uuid) -> bool {
        if self.picker_refresh != Some((thread_id, request_id)) {
            return false;
        }
        self.picker_refresh = None;
        true
    }

    /// Returns the cached picker entry for a specific thread id.
    ///
    /// Callers use this when they already know which thread they care about and need the last
    /// metadata captured for picker or footer rendering. If a caller assumes every tracked thread
    /// must be present here, shutdown races can turn that assumption into a panic elsewhere, so
    /// this stays optional.
    pub(crate) fn get(&self, thread_id: &ThreadId) -> Option<&AgentPickerThreadEntry> {
        self.threads.get(thread_id)
    }

    pub(crate) fn is_parent_owned(&self, thread_id: ThreadId) -> bool {
        self.parent_owned_threads.contains(&thread_id)
    }

    /// Marks a spawned child thread as view-only for direct user instructions.
    pub(crate) fn mark_parent_owned(&mut self, thread_id: ThreadId) {
        self.parent_owned_threads.insert(thread_id);
    }

    /// Returns whether the picker cache currently knows about any threads.
    ///
    /// This is the cheapest way for `App` to decide whether opening the picker should show "No
    /// agents available yet." rather than constructing picker rows from an empty state.
    pub(crate) fn is_empty(&self) -> bool {
        self.threads.is_empty()
    }

    /// Inserts or updates a picker entry while preserving first-seen traversal order.
    ///
    /// The key invariant of this module is enforced here: a thread id is appended to `order` only
    /// the first time it is seen. Later updates may change nickname, role, or closed state, but
    /// they must not move the thread in the cycle or keyboard navigation would feel unstable.
    pub(crate) fn upsert(
        &mut self,
        thread_id: ThreadId,
        agent_nickname: Option<String>,
        agent_role: Option<String>,
        is_closed: bool,
    ) {
        if !self.threads.contains_key(&thread_id) {
            self.order.push(thread_id);
        }
        let (previous_agent_path, previous_semantic_name, previous_is_running, previous_is_closed) =
            self.threads
                .get(&thread_id)
                .map(|entry| {
                    (
                        entry.agent_path.clone(),
                        entry.agent_semantic_name.clone(),
                        entry.is_running,
                        entry.is_closed,
                    )
                })
                .unwrap_or((None, None, false, false));
        let is_running = previous_is_running && !is_closed;
        if previous_is_closed != is_closed || previous_is_running != is_running {
            self.status_changed_at.insert(thread_id, Instant::now());
        } else {
            self.status_changed_at
                .entry(thread_id)
                .or_insert_with(Instant::now);
        }
        self.threads.insert(
            thread_id,
            AgentPickerThreadEntry {
                agent_nickname,
                agent_role,
                agent_semantic_name: previous_semantic_name,
                agent_path: previous_agent_path,
                is_running,
                is_closed,
            },
        );
    }

    pub(crate) fn record_sub_agent_activity(&mut self, activity: SubAgentActivityDisplay) {
        if !self.threads.contains_key(&activity.thread_id) {
            self.order.push(activity.thread_id);
        }
        let entry =
            self.threads
                .entry(activity.thread_id)
                .or_insert_with(|| AgentPickerThreadEntry {
                    agent_nickname: None,
                    agent_role: None,
                    agent_semantic_name: None,
                    agent_path: None,
                    is_running: false,
                    is_closed: false,
                });
        entry.agent_path = Some(activity.agent_path);
        let is_running = activity.is_running_hint
            && !entry.is_closed
            && !self.stopped_threads.contains(&activity.thread_id);
        if entry.is_running != is_running {
            entry.is_running = is_running;
            self.status_changed_at
                .insert(activity.thread_id, Instant::now());
        } else {
            self.status_changed_at
                .entry(activity.thread_id)
                .or_insert_with(Instant::now);
        }
        if !is_running {
            self.stopped_threads.insert(activity.thread_id);
        }
    }

    pub(crate) fn mark_running(&mut self, thread_id: ThreadId) {
        if self
            .threads
            .get(&thread_id)
            .is_some_and(|entry| entry.is_closed)
        {
            return;
        }
        self.stopped_threads.remove(&thread_id);
        self.set_running(thread_id, /*is_running*/ true);
    }

    pub(crate) fn mark_stopped(&mut self, thread_id: ThreadId) {
        self.stopped_threads.insert(thread_id);
        self.set_running(thread_id, /*is_running*/ false);
    }

    pub(crate) fn set_running(&mut self, thread_id: ThreadId, is_running: bool) {
        if let Some(entry) = self.threads.get_mut(&thread_id) {
            if entry.is_running != is_running {
                entry.is_running = is_running;
                self.status_changed_at.insert(thread_id, Instant::now());
            } else {
                self.status_changed_at
                    .entry(thread_id)
                    .or_insert_with(Instant::now);
            }
        }
    }

    pub(crate) fn set_agent_path(&mut self, thread_id: ThreadId, agent_path: Option<String>) {
        if let Some(agent_path) = agent_path
            && let Some(entry) = self.threads.get_mut(&thread_id)
        {
            entry.agent_path = Some(agent_path);
        }
    }

    pub(crate) fn set_agent_semantic_name(
        &mut self,
        thread_id: ThreadId,
        semantic_name: Option<String>,
    ) {
        if let Some(entry) = self.threads.get_mut(&thread_id) {
            entry.agent_semantic_name = semantic_name;
        }
    }

    /// Marks a thread as closed without removing it from the traversal cache.
    ///
    /// Closed threads stay in the picker and in spawn order so users can still review them and so
    /// next/previous navigation does not reshuffle around disappearing entries. If a caller "cleans
    /// this up" by deleting the entry instead, wraparound navigation will silently change shape
    /// mid-session.
    pub(crate) fn mark_closed(&mut self, thread_id: ThreadId) {
        if let Some(entry) = self.threads.get_mut(&thread_id) {
            if !entry.is_closed || entry.is_running {
                entry.is_closed = true;
                entry.is_running = false;
                self.status_changed_at.insert(thread_id, Instant::now());
            }
        } else {
            self.upsert(
                thread_id, /*agent_nickname*/ None, /*agent_role*/ None,
                /*is_closed*/ true,
            );
        }
    }

    /// Drops all cached picker state.
    ///
    /// This is used when `App` tears down thread event state and needs the picker cache to return
    /// to a pristine single-session state.
    pub(crate) fn clear(&mut self) {
        self.threads.clear();
        self.order.clear();
        self.stopped_threads.clear();
        self.status_changed_at.clear();
        self.parent_owned_threads.clear();
        self.picker_refresh = None;
    }

    /// Removes a tracked thread entirely from picker metadata and traversal order.
    ///
    /// This is reserved for entries that were only discovered opportunistically and never became
    /// replayable local threads. Keeping those around after the backend confirms they are gone
    /// would leave ghost rows in `/subagents`.
    pub(crate) fn remove(&mut self, thread_id: ThreadId) {
        self.threads.remove(&thread_id);
        self.order.retain(|candidate| *candidate != thread_id);
        self.stopped_threads.remove(&thread_id);
        self.status_changed_at.remove(&thread_id);
        self.parent_owned_threads.remove(&thread_id);
    }

    /// Returns whether there is at least one tracked thread other than the primary one.
    ///
    /// `App` uses this to decide whether the picker should be available even when the collaboration
    /// feature flag is currently disabled, because already-existing sub-agent threads should remain
    /// inspectable.
    pub(crate) fn has_non_primary_thread(&self, primary_thread_id: Option<ThreadId>) -> bool {
        self.threads
            .keys()
            .any(|thread_id| Some(*thread_id) != primary_thread_id)
    }

    /// Returns live picker rows in the same order users cycle through them.
    ///
    /// The `order` vector is intentionally historical and may briefly contain thread ids that no
    /// longer have cached metadata, so this filters through the map instead of assuming both
    /// collections are perfectly synchronized.
    pub(crate) fn ordered_threads(&self) -> Vec<(ThreadId, &AgentPickerThreadEntry)> {
        self.order
            .iter()
            .filter_map(|thread_id| self.threads.get(thread_id).map(|entry| (*thread_id, entry)))
            .collect()
    }

    pub(crate) fn ordered_path_backed_subagent_threads(
        &self,
        primary_thread_id: Option<ThreadId>,
    ) -> Vec<(ThreadId, &AgentPickerThreadEntry)> {
        self.ordered_threads()
            .into_iter()
            .filter(|(thread_id, entry)| {
                Some(*thread_id) != primary_thread_id
                    && entry
                        .agent_path
                        .as_deref()
                        .is_some_and(|agent_path| !agent_path.trim().is_empty())
            })
            .collect()
    }

    pub(crate) fn selector_entries(
        &self,
        primary_thread_id: Option<ThreadId>,
    ) -> Vec<AgentSelectorEntry> {
        fn append_children<'a>(
            parent_path: &str,
            ordered: &[(ThreadId, &'a AgentPickerThreadEntry)],
            visited: &mut HashSet<ThreadId>,
            output: &mut Vec<(ThreadId, &'a AgentPickerThreadEntry)>,
        ) {
            for (thread_id, entry) in ordered.iter().copied() {
                let Some(agent_path) = entry.agent_path.as_deref() else {
                    continue;
                };
                if parent_agent_path(agent_path) != Some(parent_path) || !visited.insert(thread_id)
                {
                    continue;
                }
                output.push((thread_id, entry));
                append_children(agent_path, ordered, visited, output);
            }
        }

        let ordered = self.ordered_threads();
        let mut visited = HashSet::new();
        let mut hierarchical = Vec::with_capacity(ordered.len());
        if let Some(primary_thread_id) = primary_thread_id
            && let Some((thread_id, entry)) = ordered
                .iter()
                .copied()
                .find(|(thread_id, _)| *thread_id == primary_thread_id)
        {
            visited.insert(thread_id);
            hierarchical.push((thread_id, entry));
        }
        append_children("/root", &ordered, &mut visited, &mut hierarchical);
        hierarchical.extend(
            ordered
                .into_iter()
                .filter(|(thread_id, _)| visited.insert(*thread_id)),
        );

        hierarchical
            .into_iter()
            .map(|(thread_id, entry)| {
                let is_primary = primary_thread_id == Some(thread_id);
                AgentSelectorEntry {
                    thread_id,
                    label: selector_label(entry, is_primary),
                    depth: if is_primary {
                        0
                    } else {
                        entry
                            .agent_path
                            .as_deref()
                            .map(agent_path_depth)
                            .unwrap_or(0)
                    },
                    is_running: entry.is_running,
                    is_closed: entry.is_closed,
                    status_changed_at: self
                        .status_changed_at
                        .get(&thread_id)
                        .copied()
                        .unwrap_or_else(Instant::now),
                }
            })
            .collect()
    }

    /// Returns tracked thread ids in the same stable order used by the picker.
    pub(crate) fn tracked_thread_ids(&self) -> Vec<ThreadId> {
        self.ordered_threads()
            .into_iter()
            .map(|(thread_id, _)| thread_id)
            .collect()
    }

    /// Returns the adjacent thread id for keyboard navigation in stable spawn order.
    ///
    /// The caller must pass the thread whose transcript is actually being shown to the user, not
    /// just whichever thread bookkeeping most recently marked active. If the wrong current thread
    /// is supplied, next/previous navigation will jump in a way that feels nondeterministic even
    /// though the cache itself is correct.
    pub(crate) fn adjacent_thread_id(
        &self,
        current_displayed_thread_id: Option<ThreadId>,
        direction: AgentNavigationDirection,
    ) -> Option<ThreadId> {
        let ordered_threads = self.ordered_threads();
        if ordered_threads.len() < 2 {
            return None;
        }

        let current_thread_id = current_displayed_thread_id?;
        let current_idx = ordered_threads
            .iter()
            .position(|(thread_id, _)| *thread_id == current_thread_id)?;
        let next_idx = match direction {
            AgentNavigationDirection::Next => (current_idx + 1) % ordered_threads.len(),
            AgentNavigationDirection::Previous => {
                if current_idx == 0 {
                    ordered_threads.len() - 1
                } else {
                    current_idx - 1
                }
            }
        };
        Some(ordered_threads[next_idx].0)
    }

    /// Derives the contextual footer label for the currently displayed thread.
    ///
    /// This intentionally returns `None` until there is more than one tracked thread so
    /// single-thread sessions do not waste footer space restating the obvious. When metadata for
    /// the displayed thread is missing, the label falls back to the same generic naming rules used
    /// by the picker.
    pub(crate) fn active_agent_label(
        &self,
        current_displayed_thread_id: Option<ThreadId>,
        primary_thread_id: Option<ThreadId>,
    ) -> Option<String> {
        if self.threads.len() <= 1 {
            return None;
        }

        let thread_id = current_displayed_thread_id?;
        let is_primary = primary_thread_id == Some(thread_id);
        Some(
            self.threads
                .get(&thread_id)
                .map(|entry| selector_label(entry, is_primary))
                .unwrap_or_else(|| {
                    format_agent_picker_item_name(
                        /*agent_nickname*/ None, /*agent_role*/ None, is_primary,
                    )
                }),
        )
    }

    #[cfg(test)]
    /// Returns only the ordered thread ids for focused tests of traversal invariants.
    ///
    /// This helper exists so tests can assert on ordering without embedding the full picker entry
    /// payload in every expectation.
    pub(crate) fn ordered_thread_ids(&self) -> Vec<ThreadId> {
        self.ordered_threads()
            .into_iter()
            .map(|(thread_id, _)| thread_id)
            .collect()
    }
}

fn parent_agent_path(agent_path: &str) -> Option<&str> {
    let trimmed = agent_path.trim_end_matches('/');
    let separator = trimmed.rfind('/')?;
    (separator > 0).then(|| &trimmed[..separator])
}

fn agent_path_depth(agent_path: &str) -> usize {
    agent_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count()
        .saturating_sub(2)
}

fn selector_label(entry: &AgentPickerThreadEntry, is_primary: bool) -> String {
    if is_primary {
        return format_agent_picker_item_name(
            entry.agent_nickname.as_deref(),
            entry.agent_role.as_deref(),
            /*is_primary*/ true,
        );
    }

    let nickname = entry
        .agent_nickname
        .as_deref()
        .map(str::trim)
        .filter(|nickname| !nickname.is_empty());
    let semantic_name = entry
        .agent_semantic_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let path_name = entry
        .agent_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .and_then(|path| path.rsplit('/').find(|segment| !segment.is_empty()));
    let identity = semantic_name.or(path_name).unwrap_or("Agent");
    let name = nickname
        .map(|nickname| format!("{nickname} · {identity}"))
        .unwrap_or_else(|| identity.to_string());
    let role = entry
        .agent_role
        .as_deref()
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .unwrap_or("default");
    format!("{name} [{role}]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn populated_state() -> (AgentNavigationState, ThreadId, ThreadId, ThreadId) {
        let mut state = AgentNavigationState::default();
        let main_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000101").expect("valid thread");
        let first_agent_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000102").expect("valid thread");
        let second_agent_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000103").expect("valid thread");

        state.upsert(
            main_thread_id,
            /*agent_nickname*/ None,
            /*agent_role*/ None,
            /*is_closed*/ false,
        );
        state.upsert(
            first_agent_id,
            Some("Robie".to_string()),
            Some("explorer".to_string()),
            /*is_closed*/ false,
        );
        state.upsert(
            second_agent_id,
            Some("Bob".to_string()),
            Some("worker".to_string()),
            /*is_closed*/ false,
        );

        (state, main_thread_id, first_agent_id, second_agent_id)
    }

    #[test]
    fn upsert_preserves_first_seen_order() {
        let (mut state, main_thread_id, first_agent_id, second_agent_id) = populated_state();

        state.upsert(
            first_agent_id,
            Some("Robie".to_string()),
            Some("worker".to_string()),
            /*is_closed*/ true,
        );

        assert_eq!(
            state.ordered_thread_ids(),
            vec![main_thread_id, first_agent_id, second_agent_id]
        );
    }

    #[test]
    fn parent_owned_state_is_removed_with_thread_metadata() {
        let (mut state, _main_thread_id, first_agent_id, second_agent_id) = populated_state();

        state.mark_parent_owned(first_agent_id);
        assert!(state.is_parent_owned(first_agent_id));
        state.remove(first_agent_id);
        assert!(!state.is_parent_owned(first_agent_id));

        state.mark_parent_owned(second_agent_id);
        state.clear();
        assert!(!state.is_parent_owned(second_agent_id));
    }

    #[test]
    fn picker_refresh_rejects_responses_from_before_clear() {
        let mut state = AgentNavigationState::default();
        let thread_id = ThreadId::new();
        let stale_request = state
            .begin_picker_refresh(thread_id)
            .expect("first picker refresh");

        assert_eq!(state.begin_picker_refresh(thread_id), None);
        state.clear();
        let current_request = state
            .begin_picker_refresh(thread_id)
            .expect("refresh after session reset");

        assert!(!state.finish_picker_refresh(thread_id, stale_request));
        assert!(state.finish_picker_refresh(thread_id, current_request));
    }

    #[test]
    fn adjacent_thread_id_wraps_in_spawn_order() {
        let (state, main_thread_id, first_agent_id, second_agent_id) = populated_state();

        assert_eq!(
            state.adjacent_thread_id(Some(second_agent_id), AgentNavigationDirection::Next),
            Some(main_thread_id)
        );
        assert_eq!(
            state.adjacent_thread_id(Some(second_agent_id), AgentNavigationDirection::Previous),
            Some(first_agent_id)
        );
        assert_eq!(
            state.adjacent_thread_id(Some(main_thread_id), AgentNavigationDirection::Previous),
            Some(second_agent_id)
        );
    }

    #[test]
    fn selector_entries_follow_agent_path_hierarchy() {
        let (mut state, main_thread_id, first_agent_id, second_agent_id) = populated_state();
        let grandchild_id = ThreadId::new();
        state.set_agent_path(first_agent_id, Some("/root/research".to_string()));
        state.set_agent_semantic_name(first_agent_id, Some("后端调研".to_string()));
        state.set_agent_path(second_agent_id, Some("/root/verify".to_string()));
        state.upsert(
            grandchild_id,
            Some("Turing".to_string()),
            /*agent_role*/ None,
            /*is_closed*/ false,
        );
        state.set_agent_path(grandchild_id, Some("/root/research/protocol".to_string()));

        let entries = state.selector_entries(Some(main_thread_id));
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.thread_id, entry.label.as_str(), entry.depth))
                .collect::<Vec<_>>(),
            vec![
                (main_thread_id, "Codex [default]", 0),
                (first_agent_id, "Robie · 后端调研 [explorer]", 0),
                (grandchild_id, "Turing · protocol [default]", 1),
                (second_agent_id, "Bob · verify [worker]", 0),
            ]
        );
    }

    #[test]
    fn active_agent_label_tracks_current_thread() {
        let (state, main_thread_id, first_agent_id, _) = populated_state();

        assert_eq!(
            state.active_agent_label(Some(first_agent_id), Some(main_thread_id)),
            Some("Robie [explorer]".to_string())
        );
        assert_eq!(
            state.active_agent_label(Some(main_thread_id), Some(main_thread_id)),
            Some("Codex [default]".to_string())
        );
    }
}

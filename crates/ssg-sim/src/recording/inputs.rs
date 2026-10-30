//! The session's sim-affecting inputs (P7, R1, R5): what a re-simulation needs
//! besides the scenario and the plan.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::project::recording::RecordingBodyRole;

/// One sim-affecting input; as a line of `inputs.jsonl` it is
/// `{"session_start": {…}}` or `{"start_capture": {…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedCommand {
    /// Always the first entry: the fixed steps that ran in the update that spawned
    /// the bodies, before `init_orbitals` built the propagators (k, C1). The CLI
    /// and the harness run 1; the app runs 0 … 3, or more with the warp raised
    /// before Start.
    SessionStart { steps_before_propagators: u32 },
    /// `SimCommand::StartCapture` applied to the body with this role.
    StartCapture {
        target: RecordingBodyRole,
        plan_id: String,
    },
}

/// An input applied after fixed step `after_step` (in `Last`, or by
/// `init_orbitals` in `First` for `SessionStart`); a line of `inputs.jsonl`
/// (`{"step": …, "command": …}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedInput {
    #[serde(rename = "step")]
    pub after_step: u64,
    pub command: RecordedCommand,
}

/// Every sim-affecting input of the session, recording or not, from step 0
/// (R5). Reset on entering the sim screen.
#[derive(Resource, Debug, Default, Clone)]
pub struct SimInputLog {
    pub entries: Vec<RecordedInput>,
    /// The capture target of the session's body table, set when the bodies spawn:
    /// inputs name bodies by role, never by `Entity`.
    pub(crate) target: Option<Entity>,
}

impl SimInputLog {
    /// Appends `SessionStart` with k = `step` unless the log already has one.
    pub fn push_session_start(&mut self, step: u32) {
        if self
            .entries
            .iter()
            .any(|e| matches!(e.command, RecordedCommand::SessionStart { .. }))
        {
            return;
        }
        self.entries.push(RecordedInput {
            after_step: u64::from(step),
            command: RecordedCommand::SessionStart {
                steps_before_propagators: step,
            },
        });
    }

    /// Appends a capture start after `after_step` on `entity`. Only the session's
    /// capture target has a role; a start on another entity is not recordable and
    /// is left out (neither the app nor the CLI sends one).
    pub fn push_start_capture(&mut self, after_step: u64, entity: Entity, plan_id: &str) {
        if self.target != Some(entity) {
            return;
        }
        self.entries.push(RecordedInput {
            after_step,
            command: RecordedCommand::StartCapture {
                target: RecordingBodyRole::Target,
                plan_id: plan_id.to_string(),
            },
        });
    }
}

/// Clears the log for a new session (`OnEnter(UiScreen::Sim)`).
pub(crate) fn reset_sim_input_log(mut log: ResMut<SimInputLog>) {
    *log = SimInputLog::default();
}

//! The app's screen state (Q35, P8 S2) and the sim session it drives.
//!
//! `UiScreen::Sim` ⇔ `SimSession::Active`. Every screen change sets
//! `NextState<UiScreen>` only; [`drive_sim_session`] mirrors it into
//! `NextState<SimSession>` just before the states apply, so both change in the
//! same `StateTransition` run. The transition order is fixed so the app's
//! screen systems see the session resources (F-40 class):
//! - entering the sim: `OnEnter(SimSession::Active)` (`SimSet::SessionInit`)
//!   runs before `OnEnter(UiScreen::Sim)`;
//! - leaving it: `OnExit(UiScreen::Sim)` runs before `OnExit(SimSession::Active)`.

use bevy::prelude::*;
use bevy::state::state::{EnterSchedules, ExitSchedules, StateTransitionSystems};
use ssg_sim::session::SimSession;

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum UiScreen {
    #[default]
    WorkingDirectorySetup,
    Home,
    Sim,
}

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<UiScreen>()
            .configure_sets(
                StateTransition,
                (
                    EnterSchedules::<SimSession>::default()
                        .before(EnterSchedules::<UiScreen>::default()),
                    ExitSchedules::<UiScreen>::default()
                        .before(ExitSchedules::<SimSession>::default()),
                ),
            )
            .add_systems(
                StateTransition,
                drive_sim_session.before(StateTransitionSystems::DependentTransitions),
            );
    }
}

/// Sets the sim session for a pending screen change: `Active` for the sim
/// (also on a re-entry, so the session restarts with the screen), `Idle` when
/// leaving it.
pub fn drive_sim_session(
    screen: Res<State<UiScreen>>,
    next_screen: Res<NextState<UiScreen>>,
    session: Res<State<SimSession>>,
    mut next_session: ResMut<NextState<SimSession>>,
) {
    let target = match next_screen.as_ref() {
        NextState::Unchanged => return,
        NextState::Pending(target) => target,
        NextState::PendingIfNeq(target) if target == screen.get() => return,
        NextState::PendingIfNeq(target) => target,
    };
    if *target == UiScreen::Sim {
        next_session.set(SimSession::Active);
    } else if *session.get() == SimSession::Active {
        next_session.set(SimSession::Idle);
    }
}

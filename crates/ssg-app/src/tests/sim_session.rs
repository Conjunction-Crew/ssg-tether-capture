//! The app's screen drives the sim session (Q35, P8 S2): `UiScreen::Sim` ⇔
//! `SimSession::Active`, changed in the same update, with the session resources
//! present for the app's own `OnEnter`/`OnExit(UiScreen::Sim)` systems (F-40 class).

use bevy::prelude::*;

use crate::tests::app_support::{enter_sim, test_app};
use crate::ui::screen::UiScreen;
use ssg_sim::capture::CaptureSphereRadius;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::session::SimSession;
use ssg_sim::time::WorldTime;

/// What an app screen system saw: the session state and whether each session
/// resource existed.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
struct Seen(Vec<(&'static str, SimSession, bool, bool, bool)>);

/// Records what an app screen system sees (`EXIT`: the OnExit probe).
fn probe<const EXIT: bool>(
    mut seen: ResMut<Seen>,
    session: Res<State<SimSession>>,
    cache: Option<Res<OrbitalCache>>,
    time: Option<Res<WorldTime>>,
    radius: Option<Res<CaptureSphereRadius>>,
) {
    seen.0.push((
        if EXIT { "exit" } else { "enter" },
        session.get().clone(),
        cache.is_some(),
        time.is_some(),
        radius.is_some(),
    ));
}

fn probed_app() -> App {
    let mut app = test_app();
    app.init_resource::<Seen>()
        .add_systems(OnEnter(UiScreen::Sim), probe::<false>)
        .add_systems(OnExit(UiScreen::Sim), probe::<true>);
    app
}

fn go_to(app: &mut App, screen: UiScreen) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(screen);
    app.update();
}

fn session(app: &App) -> SimSession {
    app.world().resource::<State<SimSession>>().get().clone()
}

fn take_seen(app: &mut App) -> Vec<(&'static str, SimSession, bool, bool, bool)> {
    std::mem::take(&mut app.world_mut().resource_mut::<Seen>().0)
}

#[test]
fn sim_session_active_when_sim_screen_entered() {
    let mut app = probed_app();
    go_to(&mut app, UiScreen::Home);
    assert_eq!(session(&app), SimSession::Idle);
    assert!(app.world().get_resource::<WorldTime>().is_none());

    enter_sim(&mut app);

    // Same update: the session is active and its resources exist when the
    // app's OnEnter(UiScreen::Sim) systems run.
    assert_eq!(session(&app), SimSession::Active);
    assert_eq!(
        take_seen(&mut app),
        [("enter", SimSession::Active, true, true, true)]
    );
}

#[test]
fn leaving_the_sim_screen_ends_the_session_after_the_app_exit_systems() {
    let mut app = probed_app();
    enter_sim(&mut app);
    take_seen(&mut app);

    go_to(&mut app, UiScreen::Home);

    // The app's OnExit(UiScreen::Sim) systems still see the session resources
    // (the state value already reads Idle: Bevy applies every state before it
    // runs the exit schedules); the session ends in the same update.
    assert_eq!(
        take_seen(&mut app),
        [("exit", SimSession::Idle, true, true, true)]
    );
    assert_eq!(session(&app), SimSession::Idle);
    assert!(app.world().get_resource::<OrbitalCache>().is_none());
    assert!(app.world().get_resource::<WorldTime>().is_none());
    assert!(app.world().get_resource::<CaptureSphereRadius>().is_none());
}

#[test]
fn screens_outside_the_sim_never_start_a_session() {
    let mut app = probed_app();
    go_to(&mut app, UiScreen::Home);
    go_to(&mut app, UiScreen::WorkingDirectorySetup);
    go_to(&mut app, UiScreen::Home);

    assert_eq!(session(&app), SimSession::Idle);
    assert!(take_seen(&mut app).is_empty());
}

#[test]
fn reentering_the_sim_through_home_restarts_the_session() {
    // The restart and replay paths leave through Home and come back (P7).
    let mut app = probed_app();
    enter_sim(&mut app);
    app.world_mut().resource_mut::<WorldTime>().epoch += 600.0;
    let start = app.world().resource::<WorldTime>().start_epoch;

    go_to(&mut app, UiScreen::Home);
    enter_sim(&mut app);

    assert_eq!(session(&app), SimSession::Active);
    assert_eq!(
        take_seen(&mut app),
        [
            ("enter", SimSession::Active, true, true, true),
            ("exit", SimSession::Idle, true, true, true),
            ("enter", SimSession::Active, true, true, true),
        ]
    );
    let time = app.world().resource::<WorldTime>();
    assert_eq!(time.epoch, start, "a fresh clock");
}

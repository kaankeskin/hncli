use ratatui::layout::Rect;

use crate::{
    app::state::AppState,
    ui::{
        components::{navigation::NAVIGATION_ID, resume_list::RESUME_LIST_ID},
        handlers::InputsController,
        router::{AppRoute, AppRouter},
        screens::{Screen, ScreenComponentsRegistry, ScreenEventResponse},
        utils::breakpoints::{Breakpoints, BreakpointsDirection},
    },
};

/// The "resume Item reading" screen of hncli.
///
/// The current layout is as following:
///
/// ```md
/// ------------------------------------------
/// |              navigation                |
/// ------------------------------------------
/// |                                        |
/// |                                        |
/// |               stories                  |
/// |                                        |
/// |                                        |
/// ------------------------------------------
/// ```
#[derive(Debug)]
pub struct ResumeScreen {
    breakpoints: Breakpoints,
}

impl ResumeScreen {
    pub fn new() -> Self {
        Self {
            breakpoints: Breakpoints::new("resume_screen", &[20, 80])
                .breakpoint(25, &[10, 90])
                .breakpoint(45, &[5, 95]),
        }
    }
}

impl Screen for ResumeScreen {
    fn handle_inputs(
        &mut self,
        _inputs: &InputsController,
        _router: &mut AppRouter,
        _state: &mut AppState,
    ) -> (ScreenEventResponse, Option<AppRoute>) {
        (ScreenEventResponse::PassThrough, None)
    }

    fn compute_layout(
        &self,
        frame_size: Rect,
        components_registry: &mut ScreenComponentsRegistry,
        _state: &AppState,
    ) {
        self.breakpoints.apply(
            components_registry,
            &[NAVIGATION_ID, RESUME_LIST_ID],
            frame_size,
            BreakpointsDirection::Vertical,
        );
    }
}

use async_trait::async_trait;

use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Tabs},
};

use crate::{
    api::{HnClient, client::HnStoriesSections},
    app::AppContext,
    errors::Result,
    ui::{
        common::{RenderFrame, UiComponent, UiComponentId, UiTickScalar},
        handlers::ApplicationAction,
        router::AppRoute,
    },
};

const LEFT_TABS_TITLES: [&str; 5] = ["Home", "Ask HN", "Show HN", "Jobs", "Settings"];
const RIGHT_TABS_TITLES: [&str; 2] = ["Help", "Resume"];

/// All the tabs, in selection order (left to right).
const TABS_TITLES: [&str; LEFT_TABS_TITLES.len() + RIGHT_TABS_TITLES.len()] = [
    "Home", "Ask HN", "Show HN", "Jobs", "Settings", "Help", "Resume",
];

/// Rendered width of a `Tabs` widget, which pads every title with one space
/// on each side and inserts a one-character divider in-between.
fn tabs_width(titles: &[&str]) -> u16 {
    let titles_width: usize = titles.iter().map(|title| title.chars().count() + 2).sum();
    let dividers_width = titles.len().saturating_sub(1);
    (titles_width + dividers_width) as u16
}

/// The Navigation bar provides a convenient way to switch between screens
/// by either pressing the hotkey associated with the title, or by
/// directly switching tabs with the help of the arrow keys.
#[derive(Debug)]
pub struct Navigation {
    titles: Vec<&'static str>,
    selected_index: usize,
}

impl Default for Navigation {
    fn default() -> Self {
        Self {
            titles: TABS_TITLES.to_vec(),
            selected_index: 0,
        }
    }
}

impl Navigation {
    fn next(&mut self) {
        self.selected_index = (self.selected_index + 1) % self.titles.len();
    }

    fn previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.titles.len() - 1;
        }
    }

    fn navigate_to_current_selection(&self, ctx: &mut AppContext) {
        let route = match self.selected_index {
            0 => AppRoute::Home(HnStoriesSections::Home),
            1 => AppRoute::Home(HnStoriesSections::Ask),
            2 => AppRoute::Home(HnStoriesSections::Show),
            3 => AppRoute::Home(HnStoriesSections::Jobs),
            4 => AppRoute::Settings,
            5 => AppRoute::Help,
            6 => AppRoute::Resume,
            _ => unreachable!(),
        };
        ctx.get_state_mut().set_main_stories_loading(true);
        ctx.router_replace_current_in_navigation_stack(route);
    }
}

pub const NAVIGATION_ID: UiComponentId = "navigation";

#[async_trait]
impl UiComponent for Navigation {
    fn id(&self) -> UiComponentId {
        NAVIGATION_ID
    }

    async fn should_update(
        &mut self,
        _elapsed_ticks: UiTickScalar,
        _ctx: &AppContext,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn update(&mut self, _client: &mut HnClient, _ctx: &mut AppContext) -> Result<()> {
        Ok(())
    }

    async fn handle_inputs(&mut self, ctx: &mut AppContext) -> Result<bool> {
        let inputs = ctx.get_inputs();
        Ok(if inputs.is_active(&ApplicationAction::NavigateLeft) {
            self.previous();
            true
        } else if inputs.is_active(&ApplicationAction::NavigateRight) {
            self.next();
            true
        } else if inputs.is_active(&ApplicationAction::SelectItem) {
            if ctx.get_state().get_latest_interacted_with_component() == Some(&NAVIGATION_ID) {
                self.navigate_to_current_selection(ctx);
                true
            } else {
                false
            }
        } else {
            false
        })
    }

    fn render(&mut self, f: &mut RenderFrame, inside: Rect, ctx: &AppContext) -> Result<()> {
        let theme = ctx.get_theme();

        let current_tab_index = match ctx.get_router().get_current_route() {
            AppRoute::Home(section) => match section {
                HnStoriesSections::Home => Some(0),
                HnStoriesSections::Ask => Some(1),
                HnStoriesSections::Show => Some(2),
                HnStoriesSections::Jobs => Some(3),
            },
            AppRoute::Settings => Some(4),
            AppRoute::Help => Some(5),
            AppRoute::Resume => Some(6),
            _ => None,
        };
        let selected_title = current_tab_index.map(|index| TABS_TITLES[index]);

        let build_tabs_titles = |titles: &[&'static str]| -> Vec<Line<'static>> {
            titles
                .iter()
                .map(|title| {
                    Line::from(vec![Span::styled(
                        *title,
                        Style::default().fg(Color::White).add_modifier(
                            if Some(*title) == selected_title {
                                Modifier::UNDERLINED | Modifier::BOLD
                            } else {
                                Modifier::BOLD
                            },
                        ),
                    )])
                })
                .collect()
        };
        let build_tabs = |titles: &[&'static str], selected: Option<usize>| {
            Tabs::new(build_tabs_titles(titles))
                .select(selected)
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().fg(theme.get_accent_color()))
                .divider(Span::raw("|"))
        };

        let block = Block::default()
            .style(Style::default().fg(theme.get_block_color()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Menu");
        let inner = block.inner(inside);
        f.render_widget(block, inside);

        let [left_area, right_area] = Layout::horizontal([
            Constraint::Length(tabs_width(&LEFT_TABS_TITLES)),
            Constraint::Length(tabs_width(&RIGHT_TABS_TITLES)),
        ])
        .flex(Flex::SpaceBetween)
        .areas(inner);

        // the selected index spans both groups, so it must be rebased for the right one
        let right_selected = self.selected_index.checked_sub(LEFT_TABS_TITLES.len());
        let left_selected = right_selected.is_none().then_some(self.selected_index);

        f.render_widget(build_tabs(&LEFT_TABS_TITLES, left_selected), left_area);
        f.render_widget(build_tabs(&RIGHT_TABS_TITLES, right_selected), right_area);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{LEFT_TABS_TITLES, Navigation, RIGHT_TABS_TITLES, TABS_TITLES, tabs_width};

    #[test]
    fn test_navigation_logic() {
        let mut navigation = Navigation::default();
        assert_eq!(navigation.selected_index, 0);

        navigation.next();
        assert_eq!(navigation.selected_index, 1);
        navigation.next();
        navigation.next();
        assert_eq!(navigation.selected_index, 3);
        navigation.next();
        navigation.next();
        navigation.next();
        navigation.next();
        assert_eq!(navigation.selected_index, 0);

        navigation.previous();
        assert_eq!(navigation.selected_index, 6);
        navigation.previous();
        assert_eq!(navigation.selected_index, 5);
    }

    #[test]
    fn test_navigation_tabs_groups_match_selection_order() {
        let groups: Vec<&str> = LEFT_TABS_TITLES
            .iter()
            .chain(RIGHT_TABS_TITLES.iter())
            .copied()
            .collect();
        assert_eq!(groups, TABS_TITLES.to_vec());
    }

    #[test]
    fn test_navigation_tabs_width() {
        // " Help | Resume "
        assert_eq!(tabs_width(&RIGHT_TABS_TITLES), 15);
    }
}

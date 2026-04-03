use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label,
    ListBox, ListBoxRow, Orientation, PolicyType, STYLE_PROVIDER_PRIORITY_APPLICATION,
    ScrolledWindow, SelectionMode, Stack, Widget, gdk, glib, prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{
    data::{
        SessionEntry, WorkspaceEntry, WorkspaceGroup, close_session, create_session,
        create_workspace, load_workspace_groups, rename_workspace,
    },
    ghostty,
};

const APP_ID: &str = "com.penberg.swarm.ui";
const STYLE: &str = r#"
window {
  background: #0f1115;
}

.app-shell {
  background: linear-gradient(180deg, #11151c 0%, #0d1016 100%);
}

.sidebar {
  background: #0a0d12;
  border-right: 1px solid rgba(255, 255, 255, 0.08);
  padding: 14px 10px;
}

.content {
  padding: 16px 20px;
}

.repo-row {
  margin-top: 14px;
  margin-bottom: 8px;
}

.repo-header {
  color: #566172;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.09em;
}

.repo-add {
  min-width: 24px;
  min-height: 24px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 8px;
  color: #7a8698;
  font-size: 16px;
  font-weight: 500;
}

.repo-add:hover {
  background: rgba(126, 203, 255, 0.08);
  color: #e7f3ff;
}

.workspace-list {
  background: transparent;
}

.workspace-row {
  background: transparent;
  border-radius: 10px;
}

.workspace-row:selected,
.workspace-row:selected:focus {
  background: rgba(126, 203, 255, 0.10);
}

.workspace-card {
  padding: 9px 10px 9px 10px;
}

.workspace-name {
  color: #f3f5f7;
  font-size: 13px;
  font-weight: 700;
}

.workspace-meta {
  color: #6f7887;
  font-size: 11px;
}

.session-toolbar {
  margin-bottom: 12px;
}

.session-tabs {
  spacing: 0;
}

.session-tab {
  background: transparent;
  border-radius: 10px;
  padding: 0 4px 0 0;
}

.session-tab:hover {
  background: rgba(126, 203, 255, 0.08);
}

.session-tab-active {
  background: rgba(126, 203, 255, 0.12);
}

.session-tab-select {
  background: transparent;
  border: none;
  border-radius: 10px;
  color: #8d98a8;
  font-size: 11px;
  font-weight: 700;
  padding: 7px 10px;
}

.session-tab-active .session-tab-select {
  color: #ecf6ff;
}

.session-add {
  min-width: 26px;
  min-height: 26px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 8px;
  color: #7a8698;
  font-size: 16px;
  font-weight: 500;
}

.session-add:hover {
  background: rgba(126, 203, 255, 0.08);
  color: #e7f3ff;
}

.session-close {
  min-width: 18px;
  min-height: 18px;
  margin: 0 6px 0 0;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 999px;
  color: #7a8698;
  font-size: 11px;
  font-weight: 700;
  opacity: 0;
}

.session-close:hover {
  background: rgba(255, 133, 133, 0.16);
  color: #ffd9d9;
}

.session-tab:hover .session-close,
.session-tab-active .session-close {
  opacity: 1;
}

.terminal-host {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 16px;
  min-height: 560px;
}

.terminal-view {
  background: transparent;
  color: #edf2f7;
  font-family: monospace;
  font-size: 12px;
  padding: 16px;
}

.terminal-empty {
  color: #5e6877;
  font-size: 13px;
}

"#;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
    Ok(())
}

fn build_ui(app: &Application) {
    install_css();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("swarm")
        .default_width(1480)
        .default_height(920)
        .build();

    let state = Rc::new(AppState {
        window: window.clone(),
        selected_workspace: RefCell::new(None),
        editing_workspace: RefCell::new(None),
        selected_session: RefCell::new(None),
    });
    refresh_ui(&state, None);

    window.present();
}

struct AppState {
    window: ApplicationWindow,
    selected_workspace: RefCell<Option<String>>,
    editing_workspace: RefCell<Option<String>>,
    selected_session: RefCell<Option<String>>,
}

fn refresh_ui(state: &Rc<AppState>, preferred_workspace: Option<String>) {
    let groups = match load_workspace_groups() {
        Ok(groups) => groups,
        Err(err) => {
            eprintln!("failed to load workspaces: {err}");
            return;
        }
    };

    let selected_workspace = preferred_workspace
        .or_else(|| state.selected_workspace.borrow().clone())
        .and_then(|workspace_ref| find_workspace_by_ref(&groups, &workspace_ref))
        .or_else(|| first_workspace(&groups));

    let shell = GtkBox::new(Orientation::Horizontal, 0);
    shell.add_css_class("app-shell");

    let detail_widgets = DetailWidgets::new(state);
    if let Some(workspace) = selected_workspace.as_ref() {
        detail_widgets.render_workspace(workspace, state);
        *state.selected_workspace.borrow_mut() = Some(workspace_ref(workspace));
    } else {
        detail_widgets.render_empty();
        *state.selected_workspace.borrow_mut() = None;
        *state.editing_workspace.borrow_mut() = None;
        *state.selected_session.borrow_mut() = None;
    }

    let sidebar = build_sidebar(
        &groups,
        state.clone(),
        detail_widgets.clone(),
        selected_workspace,
    );
    let content = build_content(detail_widgets.container.clone());

    shell.append(&sidebar);
    shell.append(&content);
    state.window.set_child(Some(&shell));
}

fn schedule_refresh(state: &Rc<AppState>, preferred_workspace: Option<String>) {
    let state = state.clone();
    glib::idle_add_local_once(move || {
        refresh_ui(&state, preferred_workspace);
    });
}

fn build_sidebar(
    groups: &[WorkspaceGroup],
    state: Rc<AppState>,
    detail_widgets: DetailWidgets,
    selected_workspace: Option<WorkspaceEntry>,
) -> GtkBox {
    let sidebar = GtkBox::new(Orientation::Vertical, 0);
    sidebar.set_size_request(250, -1);
    sidebar.set_hexpand(false);
    sidebar.set_vexpand(true);
    sidebar.add_css_class("sidebar");

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("workspace-list");
    scroller.set_child(Some(&list));

    let rows = Rc::new(RefCell::new(Vec::<(ListBoxRow, WorkspaceEntry)>::new()));

    for group in groups {
        let repo_row = build_repo_row(&state, &group.repo_label, &group.repo_canonical);
        sidebar_append_static_row(&list, &repo_row);

        for workspace in &group.workspaces {
            let is_editing = state
                .editing_workspace
                .borrow()
                .as_ref()
                .is_some_and(|editing| editing == &workspace_ref(workspace));
            let row = build_workspace_row(workspace, &state, is_editing);
            if selected_workspace
                .as_ref()
                .is_some_and(|selected| selected.path == workspace.path)
            {
                list.select_row(Some(&row));
            }
            rows.borrow_mut().push((row.clone(), workspace.clone()));
            list.append(&row);
        }
    }

    if rows.borrow().is_empty() {
        let empty = Label::new(Some("No workspaces yet."));
        empty.set_xalign(0.0);
        empty.add_css_class("terminal-empty");
        sidebar_append_static_row(&list, &empty);
    }

    {
        let rows_for_signal = rows.clone();
        let state = state.clone();
        list.connect_row_selected(move |_list, row| {
            let Some(row) = row else {
                return;
            };

            if !row.is_selectable() {
                return;
            }

            if let Some((_, workspace)) = rows_for_signal
                .borrow()
                .iter()
                .find(|(candidate, _)| candidate == row)
            {
                *state.selected_workspace.borrow_mut() = Some(workspace_ref(workspace));
                *state.selected_session.borrow_mut() = None;
                detail_widgets.render_workspace(workspace, &state);
            }
        });
    }

    sidebar.append(&scroller);
    sidebar
}

fn build_repo_row(state: &Rc<AppState>, repo_label: &str, repo_canonical: &str) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.set_halign(Align::Fill);
    row.set_hexpand(true);
    row.add_css_class("repo-row");

    let label = Label::new(Some(&repo_label.to_uppercase()));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.add_css_class("repo-header");

    let button = Button::with_label("+");
    button.set_valign(Align::Center);
    button.add_css_class("repo-add");

    {
        let state = state.clone();
        let repo_canonical = repo_canonical.to_string();
        button.connect_clicked(move |_| {
            create_and_edit_workspace(&state, &repo_canonical);
        });
    }

    row.append(&label);
    row.append(&button);
    row
}

fn build_workspace_row(
    workspace: &WorkspaceEntry,
    state: &Rc<AppState>,
    is_editing: bool,
) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_selectable(true);
    row.set_activatable(true);
    row.add_css_class("workspace-row");

    let card = GtkBox::new(Orientation::Vertical, 4);
    card.add_css_class("workspace-card");

    if is_editing {
        let entry = Entry::new();
        entry.set_hexpand(true);
        entry.set_text(&workspace.name);
        entry.select_region(0, -1);
        entry.add_css_class("workspace-name");
        install_workspace_rename_handlers(&entry, state, workspace);
        card.append(&entry);
        glib::idle_add_local_once(move || {
            entry.grab_focus();
        });
    } else {
        let name = Label::new(Some(&workspace.name));
        name.set_xalign(0.0);
        name.add_css_class("workspace-name");
        card.append(&name);
    }

    let meta = Label::new(Some(&format!(
        "{}  •  {} sessions",
        workspace.branch,
        workspace.sessions.len()
    )));
    meta.set_xalign(0.0);
    meta.add_css_class("workspace-meta");

    card.append(&meta);
    row.set_child(Some(&card));
    row
}

fn build_content(detail_container: GtkBox) -> GtkBox {
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.add_css_class("content");
    content.append(&detail_container);
    content
}

fn create_and_edit_workspace(state: &Rc<AppState>, repo_canonical: &str) {
    let placeholder = next_workspace_placeholder();
    match create_workspace(repo_canonical, Some(&placeholder)) {
        Ok(workspace) => {
            let workspace_ref = workspace_ref(&workspace);
            *state.selected_workspace.borrow_mut() = Some(workspace_ref.clone());
            *state.editing_workspace.borrow_mut() = Some(workspace_ref.clone());
            *state.selected_session.borrow_mut() = None;
            schedule_refresh(state, Some(workspace_ref));
        }
        Err(err) => {
            eprintln!("failed to create workspace: {err}");
        }
    }
}

fn sidebar_append_static_row<W: IsA<Widget>>(list: &ListBox, widget: &W) {
    let row = ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_child(Some(widget));
    list.append(&row);
}

fn first_workspace(groups: &[WorkspaceGroup]) -> Option<WorkspaceEntry> {
    groups
        .iter()
        .find_map(|group| group.workspaces.first().cloned())
}

fn find_workspace_by_ref(groups: &[WorkspaceGroup], selected_ref: &str) -> Option<WorkspaceEntry> {
    groups.iter().find_map(|group| {
        group.workspaces.iter().find_map(|workspace| {
            if workspace_ref(workspace) == selected_ref {
                Some(workspace.clone())
            } else {
                None
            }
        })
    })
}

fn workspace_ref(workspace: &WorkspaceEntry) -> String {
    format!("{}:{}", workspace.repo_canonical, workspace.name)
}

fn install_workspace_rename_handlers(
    entry: &Entry,
    state: &Rc<AppState>,
    workspace: &WorkspaceEntry,
) {
    let workspace_ref = workspace_ref(workspace);
    let committed = Rc::new(Cell::new(false));

    {
        let state = state.clone();
        let workspace_ref = workspace_ref.clone();
        let committed = committed.clone();
        entry.connect_activate(move |entry| {
            committed.set(true);
            commit_workspace_rename(&state, &workspace_ref, &entry.text());
        });
    }

    {
        let state = state.clone();
        let workspace_ref = workspace_ref.clone();
        let committed = committed.clone();
        entry.connect_has_focus_notify(move |entry| {
            if entry.has_focus() || committed.get() {
                return;
            }

            committed.set(true);
            commit_workspace_rename(&state, &workspace_ref, &entry.text());
        });
    }
}

fn commit_workspace_rename(state: &Rc<AppState>, current_workspace_ref: &str, next_name: &str) {
    let next_name = next_name.trim();
    *state.editing_workspace.borrow_mut() = None;

    if next_name.is_empty() {
        schedule_refresh(state, Some(current_workspace_ref.to_string()));
        return;
    }

    match rename_workspace(current_workspace_ref, next_name) {
        Ok(workspace) => {
            let next_workspace_ref = workspace_ref(&workspace);
            *state.selected_workspace.borrow_mut() = Some(next_workspace_ref.clone());
            *state.selected_session.borrow_mut() = None;
            schedule_refresh(state, Some(next_workspace_ref));
        }
        Err(err) => {
            eprintln!("failed to rename workspace: {err}");
            *state.editing_workspace.borrow_mut() = Some(current_workspace_ref.to_string());
            schedule_refresh(state, Some(current_workspace_ref.to_string()));
        }
    }
}

fn next_workspace_placeholder() -> String {
    let groups = load_workspace_groups().unwrap_or_default();
    for index in 1.. {
        let candidate = format!("new-{index}");
        let exists = groups.iter().any(|group| {
            group
                .workspaces
                .iter()
                .any(|workspace| workspace.name == candidate)
        });
        if !exists {
            return candidate;
        }
    }

    "new".to_string()
}

#[derive(Clone)]
struct DetailWidgets {
    container: GtkBox,
    session_toolbar: GtkBox,
    session_tabs: GtkBox,
    session_stack: Stack,
}

impl DetailWidgets {
    fn new(state: &Rc<AppState>) -> Self {
        let container = GtkBox::new(Orientation::Vertical, 10);
        container.set_hexpand(true);
        container.set_vexpand(true);

        let session_toolbar = GtkBox::new(Orientation::Horizontal, 8);
        session_toolbar.set_halign(Align::Start);
        session_toolbar.add_css_class("session-toolbar");

        let session_stack = Stack::new();
        session_stack.set_hexpand(true);
        session_stack.set_vexpand(true);

        let session_tabs = GtkBox::new(Orientation::Horizontal, 6);
        session_tabs.set_halign(Align::Start);
        session_tabs.add_css_class("session-tabs");

        {
            let state = state.clone();
            session_stack.connect_visible_child_name_notify(move |stack| {
                *state.selected_session.borrow_mut() =
                    stack.visible_child_name().map(|name| name.to_string());
            });
        }

        session_toolbar.append(&session_tabs);
        container.append(&session_toolbar);
        container.append(&session_stack);

        Self {
            container,
            session_toolbar,
            session_tabs,
            session_stack,
        }
    }

    fn render_empty(&self) {
        clear_box(&self.session_toolbar);
        clear_stack(&self.session_stack);
    }

    fn render_workspace(&self, workspace: &WorkspaceEntry, state: &Rc<AppState>) {
        clear_box(&self.session_toolbar);
        clear_stack(&self.session_stack);

        clear_box(&self.session_tabs);
        self.session_toolbar.append(&self.session_tabs);

        let add_button = Button::with_label("+");
        add_button.set_valign(Align::Center);
        add_button.add_css_class("session-add");
        {
            let state = state.clone();
            let workspace_ref = workspace_ref(workspace);
            add_button.connect_clicked(move |_| {
                create_and_select_session(&state, &workspace_ref);
            });
        }
        self.session_toolbar.append(&add_button);

        if workspace.sessions.is_empty() {
            let empty = ghostty::terminal_host(&SessionEntry {
                id: "No sessions".to_string(),
                status: "idle".to_string(),
                command: "Create or attach a session to mount Ghostty here.".to_string(),
                log_path: String::new(),
                socket_path: String::new(),
            });
            self.session_stack
                .add_titled(&empty, Some("empty"), "empty");
            return;
        }

        for session in &workspace.sessions {
            let host = ghostty::terminal_host(session);
            self.session_stack
                .add_titled(&host, Some(&session.id), &session.id);
        }

        let selected_session = state
            .selected_session
            .borrow()
            .clone()
            .filter(|selected| {
                workspace
                    .sessions
                    .iter()
                    .any(|session| session.id == *selected)
            })
            .unwrap_or_else(|| workspace.sessions[0].id.clone());
        self.session_stack.set_visible_child_name(&selected_session);

        let workspace_ref = workspace_ref(workspace);
        let session_ids = workspace
            .sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        for session in &workspace.sessions {
            let tab = build_session_tab(
                state,
                &workspace_ref,
                &session_ids,
                &session.id,
                session.id == selected_session,
                &self.session_stack,
            );
            self.session_tabs.append(&tab);
        }
    }
}

fn clear_box(container: &GtkBox) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn clear_stack(stack: &Stack) {
    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }
}

fn build_session_tab(
    state: &Rc<AppState>,
    workspace_ref: &str,
    session_ids: &[String],
    session_id: &str,
    active: bool,
    session_stack: &Stack,
) -> GtkBox {
    let tab = GtkBox::new(Orientation::Horizontal, 0);
    tab.add_css_class("session-tab");
    if active {
        tab.add_css_class("session-tab-active");
    }

    let select_button = Button::with_label(session_id);
    select_button.set_valign(Align::Center);
    select_button.add_css_class("session-tab-select");
    {
        let state = state.clone();
        let session_id = session_id.to_string();
        let session_stack = session_stack.clone();
        select_button.connect_clicked(move |_| {
            *state.selected_session.borrow_mut() = Some(session_id.clone());
            session_stack.set_visible_child_name(&session_id);
        });
    }

    let close_button = Button::with_label("X");
    close_button.set_focus_on_click(false);
    close_button.set_valign(Align::Center);
    close_button.add_css_class("session-close");
    {
        let state = state.clone();
        let workspace_ref = workspace_ref.to_string();
        let session_ids = session_ids.to_vec();
        let session_id = session_id.to_string();
        close_button.connect_clicked(move |_| {
            close_specific_session(&state, &workspace_ref, &session_ids, &session_id);
        });
    }

    tab.append(&select_button);
    tab.append(&close_button);
    tab
}

fn create_and_select_session(state: &Rc<AppState>, workspace_ref: &str) {
    match create_session(workspace_ref) {
        Ok(session) => {
            *state.selected_workspace.borrow_mut() = Some(workspace_ref.to_string());
            *state.selected_session.borrow_mut() = Some(session.id.clone());
            schedule_refresh(state, Some(workspace_ref.to_string()));
        }
        Err(err) => {
            eprintln!("failed to create session: {err}");
        }
    }
}

fn close_specific_session(
    state: &Rc<AppState>,
    workspace_ref: &str,
    session_ids: &[String],
    session_id: &str,
) {
    let next_selected = session_ids
        .iter()
        .find(|id| id.as_str() != session_id)
        .cloned();

    match close_session(session_id) {
        Ok(_) => {
            *state.selected_workspace.borrow_mut() = Some(workspace_ref.to_string());
            *state.selected_session.borrow_mut() = next_selected;
            schedule_refresh(state, Some(workspace_ref.to_string()));
        }
        Err(err) => {
            eprintln!("failed to close session: {err}");
        }
    }
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_string(STYLE);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("missing display"),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

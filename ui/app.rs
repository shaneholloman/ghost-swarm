use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, CssProvider, Label, ListBox, ListBoxRow,
    Orientation, PolicyType, STYLE_PROVIDER_PRIORITY_APPLICATION, ScrolledWindow, SelectionMode,
    Stack, StackSwitcher, Widget, gdk, prelude::*,
};
use std::{cell::RefCell, rc::Rc};

use crate::{
    data::{SessionEntry, WorkspaceEntry, WorkspaceGroup, load_workspace_groups},
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

.repo-header {
  color: #566172;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.09em;
  margin-top: 14px;
  margin-bottom: 8px;
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

.panel-title {
  color: #f3f5f7;
  font-size: 22px;
  font-weight: 800;
}

.panel-copy {
  color: #7c8695;
  font-size: 12px;
}

.session-switcher {
  margin-top: 10px;
  margin-bottom: 12px;
}

.session-switcher button {
  background: transparent;
  border: none;
  border-radius: 10px;
  color: #8d98a8;
  font-size: 11px;
  font-weight: 700;
  padding: 7px 10px;
}

.session-switcher button:checked {
  background: rgba(126, 203, 255, 0.12);
  color: #ecf6ff;
}

.detail-label {
  color: #5f6978;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
}

.detail-value {
  color: #dbe1e8;
  font-size: 13px;
  font-weight: 500;
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
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 12px;
  padding: 16px;
}

.terminal-title {
  color: #f2f5f8;
  font-size: 13px;
  font-weight: 700;
}

.terminal-subtitle {
  color: #7c8695;
  font-size: 11px;
}

.terminal-empty {
  color: #5e6877;
  font-size: 13px;
}
"#;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let groups = load_workspace_groups()?;
    let app = Application::builder().application_id(APP_ID).build();
    let groups = Rc::new(groups);

    app.connect_activate(move |app| build_ui(app, groups.clone()));
    app.run();
    Ok(())
}

fn build_ui(app: &Application, groups: Rc<Vec<WorkspaceGroup>>) {
    install_css();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("swarm")
        .default_width(1480)
        .default_height(920)
        .build();

    let shell = GtkBox::new(Orientation::Horizontal, 0);
    shell.add_css_class("app-shell");

    let detail_widgets = DetailWidgets::new();
    let selected_workspace = first_workspace(groups.as_ref());
    if let Some(workspace) = selected_workspace.as_ref() {
        detail_widgets.render_workspace(workspace);
    } else {
        detail_widgets.render_empty();
    }

    let sidebar = build_sidebar(groups.as_ref(), detail_widgets.clone(), selected_workspace);
    let content = build_content(detail_widgets.container.clone());

    shell.append(&sidebar);
    shell.append(&content);

    window.set_child(Some(&shell));
    window.present();
}

fn build_sidebar(
    groups: &[WorkspaceGroup],
    detail_widgets: DetailWidgets,
    selected_workspace: Option<WorkspaceEntry>,
) -> GtkBox {
    let sidebar = GtkBox::new(Orientation::Vertical, 0);
    sidebar.set_size_request(250, -1);
    sidebar.set_hexpand(false);
    sidebar.set_vexpand(true);
    sidebar.add_css_class("sidebar");

    let header = GtkBox::new(Orientation::Vertical, 4);
    sidebar.append(&header);

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.set_margin_top(18);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("workspace-list");
    scroller.set_child(Some(&list));

    let rows = Rc::new(RefCell::new(Vec::<(ListBoxRow, WorkspaceEntry)>::new()));

    for group in groups {
        let repo_header = Label::new(Some(&group.repo_label.to_uppercase()));
        repo_header.set_xalign(0.0);
        repo_header.add_css_class("repo-header");
        sidebar_append_static_row(&list, &repo_header);

        for workspace in &group.workspaces {
            let row = build_workspace_row(workspace);
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
        empty.add_css_class("panel-copy");
        sidebar_append_static_row(&list, &empty);
    }

    let rows_for_signal = rows.clone();
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
            detail_widgets.render_workspace(workspace);
        }
    });

    sidebar.append(&scroller);
    sidebar
}

fn build_workspace_row(workspace: &WorkspaceEntry) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_selectable(true);
    row.set_activatable(true);
    row.add_css_class("workspace-row");

    let card = GtkBox::new(Orientation::Vertical, 4);
    card.add_css_class("workspace-card");

    let name = Label::new(Some(&workspace.name));
    name.set_xalign(0.0);
    name.add_css_class("workspace-name");

    let meta = Label::new(Some(&format!(
        "{}  •  {} sessions",
        workspace.branch,
        workspace.sessions.len()
    )));
    meta.set_xalign(0.0);
    meta.add_css_class("workspace-meta");

    card.append(&name);
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

#[derive(Clone)]
struct DetailWidgets {
    container: GtkBox,
    details: GtkBox,
    session_switcher: StackSwitcher,
    session_stack: Stack,
}

impl DetailWidgets {
    fn new() -> Self {
        let container = GtkBox::new(Orientation::Vertical, 10);
        container.set_hexpand(true);
        container.set_vexpand(true);

        let details = GtkBox::new(Orientation::Vertical, 10);
        details.set_halign(Align::Start);
        details.set_hexpand(false);

        let session_stack = Stack::new();
        session_stack.set_hexpand(true);
        session_stack.set_vexpand(true);

        let session_switcher = StackSwitcher::new();
        session_switcher.add_css_class("session-switcher");
        session_switcher.set_halign(Align::Start);
        session_switcher.set_stack(Some(&session_stack));

        container.append(&details);
        container.append(&session_switcher);
        container.append(&session_stack);

        Self {
            container,
            details,
            session_switcher,
            session_stack,
        }
    }

    fn render_empty(&self) {
        clear_box(&self.details);
        clear_stack(&self.session_stack);
        self.session_switcher.set_visible(false);
    }

    fn render_workspace(&self, workspace: &WorkspaceEntry) {
        clear_box(&self.details);
        clear_stack(&self.session_stack);

        if workspace.sessions.is_empty() {
            self.session_switcher.set_visible(false);
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

        self.session_switcher.set_visible(true);
        for session in &workspace.sessions {
            let host = ghostty::terminal_host(session);
            self.session_stack
                .add_titled(&host, Some(&session.id), &session.id);
        }
        self.session_stack
            .set_visible_child_name(&workspace.sessions[0].id);
    }
}

fn detail_block(label: &str, value: &str) -> GtkBox {
    let block = GtkBox::new(Orientation::Vertical, 2);

    let title = Label::new(Some(label));
    title.set_xalign(0.0);
    title.add_css_class("detail-label");

    let value_label = Label::new(Some(value));
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    value_label.set_max_width_chars(96);
    value_label.add_css_class("detail-value");

    block.append(&title);
    block.append(&value_label);
    block
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

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_string(STYLE);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("missing display"),
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

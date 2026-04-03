use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry,
    EventControllerMotion, Image, Label, ListBox, ListBoxRow, Orientation, PolicyType,
    STYLE_PROVIDER_PRIORITY_APPLICATION, ScrolledWindow, SelectionMode, Stack, Widget, gdk,
    gio::{self, FileMonitor, FileMonitorFlags},
    glib,
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{
    data::{
        SessionEntry, WorkspaceEntry, WorkspaceGroup, add_repository, close_session,
        create_session, create_workspace, current_workspace_branch, load_workspace_groups,
        remove_workspace, rename_workspace, sync_repository, workspace_head_path,
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

.sidebar-toolbar {
  margin-bottom: 10px;
}

.sidebar-primary-action {
  min-height: 28px;
  padding: 0 10px;
  background: transparent;
  color: #cfd8e4;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 9px;
  font-size: 12px;
  font-weight: 700;
}

.sidebar-primary-action:hover {
  background: rgba(255, 255, 255, 0.04);
  color: #f3f7fb;
}

.repo-compose {
  margin-bottom: 12px;
  padding: 2px 0 4px 0;
}

.repo-compose-entry {
  min-height: 32px;
  padding: 0 9px;
  background: rgba(255, 255, 255, 0.025);
  color: #eef4fb;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 9px;
}

.repo-compose-entry text {
  color: #eef4fb;
}

.repo-compose-error {
  color: #ff9b9b;
  font-size: 11px;
  font-weight: 600;
}

.repo-compose-submit {
  min-height: 26px;
  padding: 0 10px;
  background: transparent;
  color: #eef7ff;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 9px;
  font-size: 11px;
  font-weight: 700;
}

.repo-compose-submit:hover {
  background: rgba(255, 255, 255, 0.05);
}

.repo-compose-cancel {
  min-height: 26px;
  padding: 0 10px;
  background: transparent;
  color: #7f8b9d;
  border: none;
  border-radius: 9px;
  font-size: 11px;
  font-weight: 700;
}

.repo-compose-cancel:hover {
  background: rgba(255, 255, 255, 0.05);
  color: #f3f5f7;
}

.content {
  padding: 16px 20px;
}

.repo-row {
  margin-top: 14px;
  margin-bottom: 8px;
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

.repo-sync {
  min-width: 24px;
  min-height: 24px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 8px;
  color: #7a8698;
}

.repo-sync:hover {
  background: rgba(126, 203, 255, 0.08);
  color: #e7f3ff;
}

.repo-header {
  color: #566172;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.09em;
}

.repo-status {
  color: #6f7887;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.02em;
  opacity: 0;
}

.repo-row-hover .repo-status {
  opacity: 1;
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

.workspace-delete-slot {
  min-width: 28px;
}

.workspace-delete {
  min-width: 24px;
  min-height: 24px;
  padding: 0;
  background: transparent;
  border: none;
  border-radius: 8px;
  color: #7a8698;
}

.workspace-delete:hover {
  background: rgba(255, 133, 133, 0.16);
  color: #ffd9d9;
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

.workspace-name-entry {
  color: #6f7887;
  font-size: 11px;
  font-weight: 400;
  padding: 0;
  background: transparent;
  border: none;
  box-shadow: none;
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
  background: linear-gradient(180deg, rgba(19, 25, 36, 0.96) 0%, rgba(12, 17, 26, 0.98) 100%);
  border: 1px solid rgba(154, 182, 255, 0.14);
  border-radius: 18px;
  min-height: 560px;
  padding: 10px 10px 10px 0;
  box-shadow: inset 0 1px rgba(255, 255, 255, 0.04), 0 18px 48px rgba(0, 0, 0, 0.24);
}

.terminal-view {
  background: transparent;
  color: #e6edf3;
  font-family: "JetBrainsMono Nerd Font", "JetBrains Mono", "Iosevka Term", "SF Mono", "Menlo", monospace;
  font-size: 10px;
}

.terminal-empty {
  color: #74839a;
  font-size: 13px;
}

.terminal-host scrollbar {
  background: transparent;
  margin: 10px 8px 10px 0;
  min-width: 10px;
}

.terminal-host scrollbar slider {
  background: rgba(143, 175, 255, 0.24);
  border-radius: 999px;
  min-width: 8px;
}

.terminal-host scrollbar slider:hover {
  background: rgba(143, 175, 255, 0.34);
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
        repository_form: RefCell::new(RepositoryFormState::default()),
        branch_monitors: RefCell::new(Vec::new()),
    });
    refresh_ui(&state, None, None);

    window.present();
}

struct AppState {
    window: ApplicationWindow,
    selected_workspace: RefCell<Option<String>>,
    editing_workspace: RefCell<Option<String>>,
    selected_session: RefCell<Option<String>>,
    repository_form: RefCell<RepositoryFormState>,
    branch_monitors: RefCell<Vec<FileMonitor>>,
}

#[derive(Clone, Default)]
struct RepositoryFormState {
    expanded: bool,
    repository: String,
    alias: String,
    error: Option<String>,
}

fn refresh_ui(
    state: &Rc<AppState>,
    preferred_workspace: Option<String>,
    preferred_session: Option<String>,
) {
    state.branch_monitors.borrow_mut().clear();
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
        detail_widgets.render_workspace(workspace, state, preferred_session.as_deref());
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

fn schedule_refresh(
    state: &Rc<AppState>,
    preferred_workspace: Option<String>,
    preferred_session: Option<String>,
) {
    let state = state.clone();
    glib::idle_add_local_once(move || {
        refresh_ui(&state, preferred_workspace, preferred_session);
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

    let groups_empty = groups.is_empty();
    let toolbar = build_sidebar_toolbar(&state, groups_empty);
    sidebar.append(&toolbar);

    if state.repository_form.borrow().expanded || groups_empty {
        let form = build_repository_form(&state, groups_empty);
        sidebar.append(&form);
    }

    let scroller = ScrolledWindow::new();
    scroller.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("workspace-list");
    scroller.set_child(Some(&list));

    let rows = Rc::new(RefCell::new(Vec::<WorkspaceRow>::new()));

    for group in groups {
        let repo_row = build_repo_row(
            &state,
            &group.repo_label,
            &group.repo_canonical,
            group.repo_status.as_deref(),
        );
        sidebar_append_static_row(&list, &repo_row);

        for workspace in &group.workspaces {
            let is_editing = state
                .editing_workspace
                .borrow()
                .as_ref()
                .is_some_and(|editing| editing == &workspace_ref(workspace));
            let is_selected = selected_workspace
                .as_ref()
                .is_some_and(|selected| selected.path == workspace.path);
            let workspace_row = build_workspace_row(workspace, &state, is_editing, is_selected);
            let row = workspace_row.row.clone();
            if is_selected {
                list.select_row(Some(&row));
            }
            rows.borrow_mut().push(workspace_row);
            list.append(&row);
        }
    }

    if groups_empty {
        let empty = Label::new(Some("Add a repository to start creating workspaces."));
        empty.set_wrap(true);
        empty.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        empty.set_xalign(0.0);
        empty.add_css_class("terminal-empty");
        sidebar_append_static_row(&list, &empty);
    } else if rows.borrow().is_empty() {
        let empty = Label::new(Some("No workspaces yet."));
        empty.set_xalign(0.0);
        empty.add_css_class("terminal-empty");
        sidebar_append_static_row(&list, &empty);
    }

    {
        let rows_for_signal = rows.clone();
        let state = state.clone();
        let detail_widgets = detail_widgets.clone();
        list.connect_row_selected(move |_list, row| {
            let Some(row) = row else {
                return;
            };
            let selected_row = row.clone();

            if !row.is_selectable() {
                return;
            }

            let selected_workspace = {
                let rows = rows_for_signal.borrow();
                for candidate in rows.iter() {
                    let is_selected = candidate.row == selected_row;
                    candidate.delete_button.set_visible(is_selected);
                    candidate.delete_button.set_sensitive(is_selected);
                }

                rows.iter()
                    .find(|workspace_row| workspace_row.row == selected_row)
                    .map(|workspace_row| workspace_row.workspace.clone())
            };

            if let Some(workspace) = selected_workspace {
                let next_workspace_ref = workspace_ref(&workspace);
                let preserve_session = state
                    .selected_workspace
                    .borrow()
                    .as_ref()
                    .is_some_and(|selected| selected == &next_workspace_ref);
                let preferred_session = if preserve_session {
                    state.selected_session.borrow().clone()
                } else {
                    None
                };

                *state.selected_workspace.borrow_mut() = Some(next_workspace_ref);
                *state.selected_session.borrow_mut() = preferred_session.clone();
                detail_widgets.render_workspace(&workspace, &state, preferred_session.as_deref());
            }
        });
    }

    sidebar.append(&scroller);
    sidebar
}

fn build_sidebar_toolbar(state: &Rc<AppState>, groups_empty: bool) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_halign(Align::Fill);
    row.set_hexpand(true);
    row.add_css_class("sidebar-toolbar");

    if !groups_empty {
        let button = Button::with_label("Add repository");
        button.set_valign(Align::Center);
        button.add_css_class("sidebar-primary-action");
        {
            let state = state.clone();
            button.connect_clicked(move |_| {
                let preferred_workspace = state.selected_workspace.borrow().clone();
                let mut form = state.repository_form.borrow_mut();
                form.expanded = true;
                form.error = None;
                drop(form);
                schedule_refresh(&state, preferred_workspace, None);
            });
        }
        row.append(&button);
    }

    row
}

fn build_repository_form(state: &Rc<AppState>, groups_empty: bool) -> GtkBox {
    let form_state = state.repository_form.borrow().clone();

    let card = GtkBox::new(Orientation::Vertical, 6);
    card.add_css_class("repo-compose");

    let repository_entry = Entry::new();
    repository_entry.set_placeholder_text(Some("github.com/owner/name"));
    repository_entry.set_text(&form_state.repository);
    repository_entry.add_css_class("repo-compose-entry");
    card.append(&repository_entry);

    let alias_entry = Entry::new();
    alias_entry.set_placeholder_text(Some("Optional alias"));
    alias_entry.set_text(&form_state.alias);
    alias_entry.add_css_class("repo-compose-entry");
    card.append(&alias_entry);

    if let Some(error) = form_state.error {
        let error_label = Label::new(Some(&error));
        error_label.set_xalign(0.0);
        error_label.set_wrap(true);
        error_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        error_label.add_css_class("repo-compose-error");
        card.append(&error_label);
    }

    let actions = GtkBox::new(Orientation::Horizontal, 6);

    let submit = Button::with_label("Add");
    submit.add_css_class("repo-compose-submit");
    actions.append(&submit);

    if !groups_empty {
        let cancel = Button::with_label("Cancel");
        cancel.add_css_class("repo-compose-cancel");
        {
            let state = state.clone();
            cancel.connect_clicked(move |_| {
                let preferred_workspace = state.selected_workspace.borrow().clone();
                let mut form = state.repository_form.borrow_mut();
                form.expanded = false;
                form.error = None;
                drop(form);
                schedule_refresh(&state, preferred_workspace, None);
            });
        }
        actions.append(&cancel);
    }

    card.append(&actions);

    {
        let state = state.clone();
        repository_entry.connect_changed(move |entry| {
            let mut form = state.repository_form.borrow_mut();
            form.repository = entry.text().to_string();
            form.error = None;
        });
    }

    {
        let state = state.clone();
        alias_entry.connect_changed(move |entry| {
            let mut form = state.repository_form.borrow_mut();
            form.alias = entry.text().to_string();
            form.error = None;
        });
    }

    {
        let state = state.clone();
        let repository_entry = repository_entry.clone();
        let alias_entry = alias_entry.clone();
        submit.connect_clicked(move |_| {
            submit_repository_form(&state, &repository_entry, &alias_entry);
        });
    }

    {
        let state = state.clone();
        let submit_repository_entry = repository_entry.clone();
        let submit_alias_entry = alias_entry.clone();
        repository_entry.clone().connect_activate(move |_| {
            submit_repository_form(&state, &submit_repository_entry, &submit_alias_entry);
        });
    }

    {
        let state = state.clone();
        let submit_repository_entry = repository_entry.clone();
        let submit_alias_entry = alias_entry.clone();
        alias_entry.clone().connect_activate(move |_| {
            submit_repository_form(&state, &submit_repository_entry, &submit_alias_entry);
        });
    }

    glib::idle_add_local_once(move || {
        repository_entry.grab_focus();
    });

    card
}

fn build_repo_row(
    state: &Rc<AppState>,
    repo_label: &str,
    repo_canonical: &str,
    repo_status: Option<&str>,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.set_halign(Align::Fill);
    row.set_hexpand(true);
    row.set_valign(Align::Center);
    row.set_spacing(6);
    row.add_css_class("repo-row");

    let copy = GtkBox::new(Orientation::Vertical, 0);
    copy.set_hexpand(true);
    copy.set_valign(Align::Center);

    let label = Label::new(Some(&repo_label.to_uppercase()));
    label.set_xalign(0.0);
    label.set_valign(Align::Center);
    label.add_css_class("repo-header");
    copy.append(&label);
    row.append(&copy);

    if let Some(repo_status) = repo_status {
        let meta = Label::new(Some(repo_status));
        meta.set_xalign(1.0);
        meta.set_valign(Align::Center);
        meta.add_css_class("repo-status");
        meta.set_tooltip_text(Some(&format!("Last synced {repo_status}")));
        row.append(&meta);
    }

    let sync_button = Button::new();
    sync_button.set_valign(Align::Center);
    sync_button.add_css_class("repo-sync");
    sync_button.set_tooltip_text(Some("Sync repository"));
    let sync_icon = Image::from_icon_name("view-refresh-symbolic");
    sync_icon.set_pixel_size(14);
    sync_button.set_child(Some(&sync_icon));

    {
        let state = state.clone();
        let repo_canonical = repo_canonical.to_string();
        sync_button.connect_clicked(move |_| {
            sync_repo_and_refresh(&state, &repo_canonical);
        });
    }

    let button = Button::with_label("+");
    button.set_valign(Align::Center);
    button.add_css_class("repo-add");
    button.set_tooltip_text(Some("New workspace"));

    {
        let state = state.clone();
        let repo_canonical = repo_canonical.to_string();
        button.connect_clicked(move |_| {
            create_and_edit_workspace(&state, &repo_canonical);
        });
    }

    let hover = EventControllerMotion::new();
    {
        let row = row.clone();
        hover.connect_enter(move |_, _, _| {
            row.add_css_class("repo-row-hover");
        });
    }
    {
        let row = row.clone();
        hover.connect_leave(move |_| {
            row.remove_css_class("repo-row-hover");
        });
    }
    row.add_controller(hover);

    row.append(&sync_button);
    row.append(&button);
    row
}

fn build_workspace_row(
    workspace: &WorkspaceEntry,
    state: &Rc<AppState>,
    is_editing: bool,
    is_selected: bool,
) -> WorkspaceRow {
    let row = ListBoxRow::new();
    row.set_selectable(true);
    row.set_activatable(true);
    row.add_css_class("workspace-row");

    let container = GtkBox::new(Orientation::Horizontal, 0);
    container.set_hexpand(true);

    let delete_slot = GtkBox::new(Orientation::Horizontal, 0);
    delete_slot.set_valign(Align::Center);
    delete_slot.add_css_class("workspace-delete-slot");

    let delete_button = Button::new();
    delete_button.set_valign(Align::Center);
    delete_button.set_tooltip_text(Some("Remove workspace"));
    delete_button.add_css_class("workspace-delete");
    delete_button.set_visible(is_selected);
    delete_button.set_sensitive(is_selected);

    let icon = Image::from_icon_name("user-trash-symbolic");
    delete_button.set_child(Some(&icon));

    {
        let state = state.clone();
        let workspace = workspace.clone();
        delete_button.connect_clicked(move |_| {
            remove_selected_workspace(&state, &workspace);
        });
    }

    delete_slot.append(&delete_button);

    let card = GtkBox::new(Orientation::Vertical, 4);
    card.set_hexpand(true);
    card.add_css_class("workspace-card");

    let branch = Label::new(Some(&workspace.branch));
    branch.set_xalign(0.0);
    branch.add_css_class("workspace-name");
    card.append(&branch);

    if is_editing {
        let entry = Entry::new();
        entry.set_hexpand(true);
        entry.set_text(&workspace.name);
        entry.select_region(0, -1);
        entry.add_css_class("workspace-name-entry");
        install_workspace_rename_handlers(&entry, state, workspace);
        card.append(&entry);
        glib::idle_add_local_once(move || {
            entry.grab_focus();
        });
    } else {
        let meta = Label::new(Some(&workspace_meta_text(
            &workspace.name,
            workspace.sessions.len(),
        )));
        meta.set_xalign(0.0);
        meta.add_css_class("workspace-meta");
        card.append(&meta);
    }

    install_branch_monitor(state, workspace, &branch);

    container.append(&delete_slot);
    container.append(&card);
    row.set_child(Some(&container));

    WorkspaceRow {
        row,
        workspace: workspace.clone(),
        delete_button,
    }
}

fn install_branch_monitor(state: &Rc<AppState>, workspace: &WorkspaceEntry, branch: &Label) {
    let head_path = match workspace_head_path(&workspace.path) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("failed to resolve HEAD for {}: {err}", workspace.path);
            return;
        }
    };

    let monitor = match gio::File::for_path(head_path)
        .monitor_file(FileMonitorFlags::NONE, None::<&gio::Cancellable>)
    {
        Ok(monitor) => monitor,
        Err(err) => {
            eprintln!(
                "failed to watch workspace branch for {}: {err}",
                workspace.path
            );
            return;
        }
    };

    let label = branch.clone();
    let workspace_path = workspace.path.clone();
    let session_count = workspace.sessions.len();
    monitor.connect_changed(
        move |_, _, _, _| match current_workspace_branch(&workspace_path) {
            Ok(branch) => label.set_text(&workspace_meta_text(&branch, session_count)),
            Err(err) => eprintln!("failed to refresh branch for {workspace_path}: {err}"),
        },
    );

    state.branch_monitors.borrow_mut().push(monitor);
}

fn workspace_meta_text(name: &str, session_count: usize) -> String {
    format!("{name}  •  {session_count} sessions")
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
            schedule_refresh(state, Some(workspace_ref), None);
        }
        Err(err) => {
            eprintln!("failed to create workspace: {err}");
        }
    }
}

fn sync_repo_and_refresh(state: &Rc<AppState>, repo_canonical: &str) {
    match sync_repository(repo_canonical) {
        Ok(()) => {
            let preferred_workspace = state.selected_workspace.borrow().clone();
            let preferred_session = state.selected_session.borrow().clone();
            schedule_refresh(state, preferred_workspace, preferred_session);
        }
        Err(err) => {
            eprintln!("failed to sync repository: {err}");
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
        schedule_refresh(state, Some(current_workspace_ref.to_string()), None);
        return;
    }

    match rename_workspace(current_workspace_ref, next_name) {
        Ok(workspace) => {
            let next_workspace_ref = workspace_ref(&workspace);
            *state.selected_workspace.borrow_mut() = Some(next_workspace_ref.clone());
            *state.selected_session.borrow_mut() = None;
            schedule_refresh(state, Some(next_workspace_ref), None);
        }
        Err(err) => {
            eprintln!("failed to rename workspace: {err}");
            *state.editing_workspace.borrow_mut() = Some(current_workspace_ref.to_string());
            schedule_refresh(state, Some(current_workspace_ref.to_string()), None);
        }
    }
}

fn remove_selected_workspace(state: &Rc<AppState>, workspace: &WorkspaceEntry) {
    let workspace_ref = workspace_ref(workspace);
    match remove_workspace(&workspace_ref) {
        Ok(_) => {
            *state.selected_workspace.borrow_mut() = None;
            *state.editing_workspace.borrow_mut() = None;
            *state.selected_session.borrow_mut() = None;
            schedule_refresh(state, None, None);
        }
        Err(err) => {
            eprintln!("failed to remove workspace: {err}");
            schedule_refresh(state, Some(workspace_ref), None);
        }
    }
}

fn submit_repository_form(state: &Rc<AppState>, repository_entry: &Entry, alias_entry: &Entry) {
    let repository = repository_entry.text().trim().to_string();
    let alias_text = alias_entry.text().trim().to_string();

    {
        let mut form = state.repository_form.borrow_mut();
        form.repository = repository.clone();
        form.alias = alias_text.clone();
        form.error = None;
    }

    let alias = (!alias_text.is_empty()).then_some(alias_text.as_str());
    let preferred_workspace = state.selected_workspace.borrow().clone();

    match add_repository(&repository, alias) {
        Ok(_) => {
            *state.repository_form.borrow_mut() = RepositoryFormState::default();
            schedule_refresh(state, preferred_workspace, None);
        }
        Err(err) => {
            state.repository_form.borrow_mut().error = Some(err.to_string());
            schedule_refresh(state, preferred_workspace, None);
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

struct WorkspaceRow {
    row: ListBoxRow,
    workspace: WorkspaceEntry,
    delete_button: Button,
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
            let session_tabs = session_tabs.clone();
            session_stack.connect_visible_child_name_notify(move |stack| {
                let selected_session = stack.visible_child_name().map(|name| name.to_string());
                *state.selected_session.borrow_mut() = selected_session.clone();
                sync_session_tab_active_state(&session_tabs, selected_session.as_deref());
                focus_visible_terminal(stack);
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

    fn render_workspace(
        &self,
        workspace: &WorkspaceEntry,
        state: &Rc<AppState>,
        preferred_session: Option<&str>,
    ) {
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

        let selected_session = preferred_session
            .map(str::to_string)
            .or_else(|| state.selected_session.borrow().clone())
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
                &self.session_tabs,
                &workspace_ref,
                &session_ids,
                &session.id,
                session.id == selected_session,
                &self.session_stack,
            );
            self.session_tabs.append(&tab);
        }
        sync_session_tab_active_state(&self.session_tabs, Some(&selected_session));
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

fn sync_session_tab_active_state(session_tabs: &GtkBox, selected_session: Option<&str>) {
    let mut child = session_tabs.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if widget.has_css_class("session-tab") {
            let is_active =
                selected_session.is_some_and(|selected| widget.widget_name() == selected);
            if is_active {
                widget.add_css_class("session-tab-active");
            } else {
                widget.remove_css_class("session-tab-active");
            }
        }
        child = next;
    }
}

fn focus_visible_terminal(session_stack: &Stack) {
    let Some(host) = session_stack.visible_child() else {
        return;
    };

    glib::idle_add_local_once(move || {
        if let Some(area) = host.first_child() {
            area.grab_focus();
        } else {
            host.grab_focus();
        }
    });
}

fn build_session_tab(
    state: &Rc<AppState>,
    session_tabs: &GtkBox,
    workspace_ref: &str,
    session_ids: &[String],
    session_id: &str,
    active: bool,
    session_stack: &Stack,
) -> GtkBox {
    let tab = GtkBox::new(Orientation::Horizontal, 0);
    tab.add_css_class("session-tab");
    tab.set_widget_name(session_id);
    if active {
        tab.add_css_class("session-tab-active");
    }

    let select_button = Button::with_label(session_id);
    select_button.set_valign(Align::Center);
    select_button.add_css_class("session-tab-select");
    {
        let state = state.clone();
        let session_id = session_id.to_string();
        let session_tabs = session_tabs.clone();
        let session_stack = session_stack.clone();
        select_button.connect_clicked(move |_| {
            *state.selected_session.borrow_mut() = Some(session_id.clone());
            session_stack.set_visible_child_name(&session_id);
            sync_session_tab_active_state(&session_tabs, Some(&session_id));
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
            schedule_refresh(
                state,
                Some(workspace_ref.to_string()),
                Some(session.id.clone()),
            );
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
            schedule_refresh(
                state,
                Some(workspace_ref.to_string()),
                state.selected_session.borrow().clone(),
            );
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

use gtk::{
    Adjustment, Box as GtkBox, DrawingArea, EventControllerKey, EventControllerScroll,
    EventControllerScrollFlags, GestureClick, Orientation, Scrollbar, gdk, glib, prelude::*,
};
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions, ffi,
    render::{CellIterator, CursorVisualStyle, RowIterator},
    style::{RgbColor, Style},
    terminal::ScrollViewport,
};
use std::{
    cell::RefCell,
    f64, fs,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    rc::Rc,
    time::Duration,
};

use crate::data::SessionEntry;

const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 40;
const MAX_SCROLLBACK: usize = 20_000;
const CELL_WIDTH: f64 = 8.6;
const CELL_HEIGHT: f64 = 18.0;
const FONT_SIZE: f64 = 13.0;
const FONT_FAMILY: &str = "monospace";
const SCROLL_LINES_PER_TICK: isize = 3;

pub fn terminal_host(session: &SessionEntry) -> GtkBox {
    let area = DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_focusable(true);
    area.add_css_class("terminal-view");

    let adjustment = Adjustment::new(0.0, 0.0, 0.0, 1.0, 8.0, 0.0);
    let scrollbar = Scrollbar::new(Orientation::Vertical, Some(&adjustment));
    scrollbar.set_vexpand(true);

    let container = GtkBox::new(Orientation::Horizontal, 0);
    container.set_hexpand(true);
    container.set_vexpand(true);
    container.set_focusable(true);
    container.add_css_class("terminal-host");
    container.append(&area);
    container.append(&scrollbar);

    let state = SessionTerminalState::new(
        session.clone(),
        area.clone(),
        adjustment.clone(),
        scrollbar.clone(),
    );
    state.borrow_mut().refresh_view();

    install_draw_func(&area, state.clone());
    install_key_controller(&area, state.clone());
    install_scroll_controller(&area, state.clone());
    install_focus_controller(&area, state.clone());
    install_scrollbar_sync(&adjustment, state.clone());
    install_socket_pump(state);

    {
        let area = area.clone();
        glib::idle_add_local_once(move || {
            area.grab_focus();
        });
    }
    container
}

fn install_draw_func(area: &DrawingArea, state: Rc<RefCell<SessionTerminalState>>) {
    area.set_draw_func(move |_area, cr, width, height| {
        if let Ok(mut state) = state.try_borrow_mut() {
            state.draw(cr, width, height);
        }
    });
}

fn install_socket_pump(state: Rc<RefCell<SessionTerminalState>>) {
    glib::timeout_add_local(Duration::from_millis(33), move || {
        if let Ok(mut state) = state.try_borrow_mut() {
            state.poll();
        }
        glib::ControlFlow::Continue
    });
}

fn install_key_controller(area: &DrawingArea, state: Rc<RefCell<SessionTerminalState>>) {
    let controller = EventControllerKey::new();
    let area_for_clipboard = area.clone();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if is_paste_shortcut(key, modifiers) {
            paste_from_clipboard(&area_for_clipboard, state.clone(), false);
            return glib::Propagation::Stop;
        }

        if let Ok(mut state) = state.try_borrow_mut() {
            state.handle_key(key, modifiers);
        }
        glib::Propagation::Stop
    });
    area.add_controller(controller);
}

fn install_scroll_controller(area: &DrawingArea, state: Rc<RefCell<SessionTerminalState>>) {
    let controller = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    controller.connect_scroll(move |_controller, _dx, dy| {
        if let Ok(mut state) = state.try_borrow_mut() {
            state.scroll_by_delta(dy);
        }
        glib::Propagation::Stop
    });
    area.add_controller(controller);
}

fn install_scrollbar_sync(adjustment: &Adjustment, state: Rc<RefCell<SessionTerminalState>>) {
    adjustment.connect_value_changed(move |adjustment| {
        if let Ok(mut state) = state.try_borrow_mut() {
            state.handle_adjustment_change(adjustment.value().round() as u64);
        }
    });
}

fn install_focus_controller(area: &DrawingArea, state: Rc<RefCell<SessionTerminalState>>) {
    let controller = GestureClick::new();
    {
        let area = area.clone();
        let state = state.clone();
        controller.connect_pressed(move |gesture, _n_press, _x, _y| {
            area.grab_focus();
            if gesture.current_button() == 2 {
                paste_from_clipboard(&area, state.clone(), true);
            }
        });
    }
    area.add_controller(controller);
}

struct SessionTerminalState {
    area: DrawingArea,
    adjustment: Adjustment,
    scrollbar: Scrollbar,
    terminal: Terminal<'static, 'static>,
    render_state: RenderState<'static>,
    row_iterator: RowIterator<'static>,
    cell_iterator: CellIterator<'static>,
    socket: Option<UnixStream>,
    sync_adjustment: bool,
    last_cols: u16,
    last_rows: u16,
}

impl SessionTerminalState {
    fn new(
        session: SessionEntry,
        area: DrawingArea,
        adjustment: Adjustment,
        scrollbar: Scrollbar,
    ) -> Rc<RefCell<Self>> {
        let mut terminal = Terminal::new(TerminalOptions {
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            max_scrollback: MAX_SCROLLBACK,
        })
        .expect("failed to create libghostty-vt terminal");

        if let Ok(log) = fs::read(Path::new(&session.log_path)) {
            terminal.vt_write(&log);
        }

        let socket = if matches!(session.status.as_str(), "running" | "starting")
            && Path::new(&session.socket_path).exists()
        {
            UnixStream::connect(&session.socket_path)
                .ok()
                .and_then(|stream| {
                    stream.set_nonblocking(true).ok()?;
                    Some(stream)
                })
        } else {
            None
        };

        Rc::new(RefCell::new(Self {
            area,
            adjustment,
            scrollbar,
            terminal,
            render_state: RenderState::new().expect("failed to create libghostty-vt render state"),
            row_iterator: RowIterator::new().expect("failed to create libghostty-vt row iterator"),
            cell_iterator: CellIterator::new()
                .expect("failed to create libghostty-vt cell iterator"),
            socket,
            sync_adjustment: false,
            last_cols: DEFAULT_COLS,
            last_rows: DEFAULT_ROWS,
        }))
    }

    fn poll(&mut self) {
        let should_follow_output = self.should_follow_output();
        let Some(socket) = &mut self.socket else {
            return;
        };

        let mut changed = false;

        loop {
            let mut chunk = [0_u8; 4096];
            match socket.read(&mut chunk) {
                Ok(0) => {
                    self.socket = None;
                    break;
                }
                Ok(len) => {
                    self.terminal.vt_write(&chunk[..len]);
                    changed = true;
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.socket = None;
                    break;
                }
            }
        }

        if changed {
            if should_follow_output {
                self.terminal.scroll_viewport(ScrollViewport::Bottom);
            }
            self.refresh_view();
        }
    }

    fn refresh_view(&mut self) {
        self.sync_scrollbar();
        self.area.queue_draw();
    }

    fn draw(&mut self, cr: &gtk::cairo::Context, width: i32, height: i32) {
        self.resize_terminal(width, height);

        let Ok(snapshot) = self.render_state.update(&self.terminal) else {
            return;
        };
        let Ok(colors) = snapshot.colors() else {
            return;
        };

        paint_rect(cr, 0.0, 0.0, width as f64, height as f64, colors.background);
        cr.select_font_face(
            FONT_FAMILY,
            gtk::cairo::FontSlant::Normal,
            gtk::cairo::FontWeight::Normal,
        );
        cr.set_font_size(FONT_SIZE);
        let Ok(font_extents) = cr.font_extents() else {
            return;
        };
        let baseline = ((CELL_HEIGHT - font_extents.height()) / 2.0) + font_extents.ascent();

        let Ok(mut rows) = self.row_iterator.update(&snapshot) else {
            return;
        };

        let mut row_index = 0_u16;
        while let Some(row) = rows.next() {
            let Ok(mut cells) = self.cell_iterator.update(row) else {
                break;
            };

            let mut col_index = 0_u16;
            while let Some(cell) = cells.next() {
                let style = cell.style().unwrap_or_default();
                let (fg, bg) = resolved_colors(cell, &style, colors.foreground, colors.background);

                let x = f64::from(col_index) * CELL_WIDTH;
                let y = f64::from(row_index) * CELL_HEIGHT;
                paint_rect(cr, x, y, CELL_WIDTH, CELL_HEIGHT, bg);

                if !style.invisible {
                    let text: String = cell.graphemes().unwrap_or_default().into_iter().collect();
                    if !text.is_empty() && text != " " {
                        cr.select_font_face(FONT_FAMILY, font_slant(style), font_weight(style));
                        cr.set_font_size(FONT_SIZE);
                        set_source_color(cr, fg);
                        cr.move_to(x, y + baseline);
                        let _ = cr.show_text(&text);
                    }
                }

                col_index = col_index.saturating_add(1);
            }

            row_index = row_index.saturating_add(1);
        }

        if let Ok(true) = snapshot.cursor_visible() {
            if let Ok(Some(cursor)) = snapshot.cursor_viewport() {
                let cursor_color = snapshot
                    .cursor_color()
                    .ok()
                    .flatten()
                    .or(colors.cursor)
                    .unwrap_or(colors.foreground);
                draw_cursor(cr, cursor.x, cursor.y, cursor_color, &snapshot);
            }
        }
    }

    fn resize_terminal(&mut self, width: i32, height: i32) {
        let cols = ((f64::from(width) / CELL_WIDTH).floor() as u16).max(1);
        let rows = ((f64::from(height) / CELL_HEIGHT).floor() as u16).max(1);
        if cols == self.last_cols && rows == self.last_rows {
            return;
        }

        self.last_cols = cols;
        self.last_rows = rows;
        let _ = self.terminal.resize(
            cols,
            rows,
            CELL_WIDTH.round() as u32,
            CELL_HEIGHT.round() as u32,
        );
        if self.should_follow_output() {
            self.terminal.scroll_viewport(ScrollViewport::Bottom);
        }
        self.sync_scrollbar();
    }

    fn should_follow_output(&self) -> bool {
        let Ok(scrollbar) = self.terminal.scrollbar() else {
            return false;
        };

        !self.is_alternate_screen()
            && scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total
    }

    fn handle_adjustment_change(&mut self, value: u64) {
        if self.sync_adjustment {
            return;
        }

        let Ok(scrollbar) = self.terminal.scrollbar() else {
            return;
        };

        let target = value.min(scrollbar.total.saturating_sub(scrollbar.len));
        if target == scrollbar.offset {
            return;
        }

        if target == 0 {
            self.terminal.scroll_viewport(ScrollViewport::Top);
        } else if target.saturating_add(scrollbar.len) >= scrollbar.total {
            self.terminal.scroll_viewport(ScrollViewport::Bottom);
        } else {
            let delta = target as i64 - scrollbar.offset as i64;
            self.terminal
                .scroll_viewport(ScrollViewport::Delta(delta as isize));
        }

        self.refresh_view();
    }

    fn scroll_by_delta(&mut self, dy: f64) {
        if dy == 0.0 {
            return;
        }

        if self.is_alternate_screen() {
            return;
        }

        let delta = (dy.signum() as isize) * SCROLL_LINES_PER_TICK;
        self.terminal.scroll_viewport(ScrollViewport::Delta(delta));
        self.refresh_view();
    }

    fn sync_scrollbar(&mut self) {
        let Ok(scrollbar) = self.terminal.scrollbar() else {
            return;
        };

        self.sync_adjustment = true;
        self.adjustment.set_lower(0.0);
        self.adjustment
            .set_upper(scrollbar.total.max(scrollbar.len) as f64);
        self.adjustment.set_page_size(scrollbar.len as f64);
        self.adjustment
            .set_page_increment(scrollbar.len.max(1) as f64);
        self.adjustment.set_step_increment(1.0);
        self.adjustment.set_value(scrollbar.offset as f64);
        self.sync_adjustment = false;

        self.scrollbar
            .set_visible(scrollbar.total > scrollbar.len && !self.is_alternate_screen());
    }

    fn is_alternate_screen(&self) -> bool {
        matches!(
            self.terminal.active_screen(),
            Ok(ffi::GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE)
        )
    }

    fn handle_key(&mut self, key: gdk::Key, modifiers: gdk::ModifierType) {
        let Some(socket) = &mut self.socket else {
            return;
        };

        let bytes = encode_key(key, modifiers);
        if bytes.is_empty() {
            return;
        }

        if socket.write_all(&bytes).is_ok() {
            let _ = socket.flush();
        }
    }

    fn paste_text(&mut self, text: &str) {
        let Some(socket) = &mut self.socket else {
            return;
        };

        if socket.write_all(text.as_bytes()).is_ok() {
            let _ = socket.flush();
        }
    }
}

fn resolved_colors(
    cell: &libghostty_vt::render::CellIteration<'_, '_>,
    style: &Style,
    default_fg: RgbColor,
    default_bg: RgbColor,
) -> (RgbColor, RgbColor) {
    let mut fg = cell.fg_color().ok().flatten().unwrap_or(default_fg);
    let mut bg = cell.bg_color().ok().flatten().unwrap_or(default_bg);

    if style.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    if style.faint {
        fg = dim_color(fg);
    }

    (fg, bg)
}

fn dim_color(color: RgbColor) -> RgbColor {
    RgbColor {
        r: ((f64::from(color.r) * 0.7).round() as u8),
        g: ((f64::from(color.g) * 0.7).round() as u8),
        b: ((f64::from(color.b) * 0.7).round() as u8),
    }
}

fn font_slant(style: Style) -> gtk::cairo::FontSlant {
    if style.italic {
        gtk::cairo::FontSlant::Italic
    } else {
        gtk::cairo::FontSlant::Normal
    }
}

fn font_weight(style: Style) -> gtk::cairo::FontWeight {
    if style.bold {
        gtk::cairo::FontWeight::Bold
    } else {
        gtk::cairo::FontWeight::Normal
    }
}

fn draw_cursor(
    cr: &gtk::cairo::Context,
    x: u16,
    y: u16,
    color: RgbColor,
    snapshot: &libghostty_vt::render::Snapshot<'_, '_>,
) {
    let x = f64::from(x) * CELL_WIDTH;
    let y = f64::from(y) * CELL_HEIGHT;
    let style = snapshot
        .cursor_visual_style()
        .unwrap_or(CursorVisualStyle::Block);

    match style {
        CursorVisualStyle::Bar => {
            paint_rect(cr, x, y, 2.0, CELL_HEIGHT, color);
        }
        CursorVisualStyle::Underline => {
            paint_rect(cr, x, y + CELL_HEIGHT - 2.0, CELL_WIDTH, 2.0, color);
        }
        CursorVisualStyle::BlockHollow => {
            set_source_color(cr, color);
            cr.set_line_width(1.0);
            cr.rectangle(x + 0.5, y + 0.5, CELL_WIDTH - 1.0, CELL_HEIGHT - 1.0);
            let _ = cr.stroke();
        }
        CursorVisualStyle::Block => {
            let mut fill = color;
            fill.r = fill.r.saturating_add(24);
            fill.g = fill.g.saturating_add(24);
            fill.b = fill.b.saturating_add(24);
            paint_rect(cr, x, y, CELL_WIDTH, CELL_HEIGHT, fill);
        }
        _ => {
            paint_rect(cr, x, y, CELL_WIDTH, CELL_HEIGHT, color);
        }
    }
}

fn paint_rect(cr: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, color: RgbColor) {
    set_source_color(cr, color);
    cr.rectangle(x, y, width, height);
    let _ = cr.fill();
}

fn set_source_color(cr: &gtk::cairo::Context, color: RgbColor) {
    cr.set_source_rgb(
        f64::from(color.r) / 255.0,
        f64::from(color.g) / 255.0,
        f64::from(color.b) / 255.0,
    );
}

fn is_paste_shortcut(key: gdk::Key, modifiers: gdk::ModifierType) -> bool {
    let ctrl_shift = modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && modifiers.contains(gdk::ModifierType::SHIFT_MASK);
    let shift_only = modifiers.contains(gdk::ModifierType::SHIFT_MASK)
        && !modifiers.contains(gdk::ModifierType::CONTROL_MASK)
        && !modifiers.contains(gdk::ModifierType::ALT_MASK);

    (ctrl_shift && matches!(key, gdk::Key::V | gdk::Key::v))
        || (shift_only && matches!(key, gdk::Key::Insert))
}

fn paste_from_clipboard(
    area: &DrawingArea,
    state: Rc<RefCell<SessionTerminalState>>,
    primary: bool,
) {
    let clipboard = if primary {
        area.primary_clipboard()
    } else {
        area.clipboard()
    };

    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
        let Ok(Some(text)) = result else {
            return;
        };

        if let Ok(mut state) = state.try_borrow_mut() {
            state.paste_text(text.as_str());
        }
    });
}

fn encode_key(key: gdk::Key, modifiers: gdk::ModifierType) -> Vec<u8> {
    match key {
        gdk::Key::Return => vec![b'\r'],
        gdk::Key::Tab => vec![b'\t'],
        gdk::Key::BackSpace => vec![0x7f],
        gdk::Key::Escape => vec![0x1b],
        gdk::Key::Up => b"\x1b[A".to_vec(),
        gdk::Key::Down => b"\x1b[B".to_vec(),
        gdk::Key::Right => b"\x1b[C".to_vec(),
        gdk::Key::Left => b"\x1b[D".to_vec(),
        gdk::Key::Home => b"\x1b[H".to_vec(),
        gdk::Key::End => b"\x1b[F".to_vec(),
        gdk::Key::Page_Up => b"\x1b[5~".to_vec(),
        gdk::Key::Page_Down => b"\x1b[6~".to_vec(),
        gdk::Key::Delete => b"\x1b[3~".to_vec(),
        key => {
            if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                if let Some(ch) = key.to_unicode() {
                    if let Some(byte) = encode_ctrl_char(ch) {
                        return vec![byte];
                    }
                }
                return Vec::new();
            }

            if let Some(ch) = key.to_unicode() {
                let mut out = Vec::new();
                if modifiers.contains(gdk::ModifierType::ALT_MASK) {
                    out.push(0x1b);
                }
                let mut buf = [0_u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                return out;
            }

            Vec::new()
        }
    }
}

fn encode_ctrl_char(ch: char) -> Option<u8> {
    match ch {
        '@' | '`' | ' ' => Some(0x00),
        'a'..='z' => Some((ch as u8) - b'a' + 0x01),
        'A'..='Z' => Some((ch as u8) - b'A' + 0x01),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        _ => None,
    }
}

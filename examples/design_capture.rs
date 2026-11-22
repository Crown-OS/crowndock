//! Captures every dock state for the CrownOS-Design Figma file. Run both appearances with
//! `../crownui/capture/scripts/capture-app.sh .`; output lands in `$CROWN_DESIGN_DIR`.

use std::{
    any::Any,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender, channel},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use capture::{CaptureApp, Carrier};
use crowndock::settings::{DockSettings, Metrics};
use crownui::prelude::{AppWindows, Capabilities, DndActions, Point, Size};
use tokio::runtime::Handle;
use view::{TaskExecutor, TaskFuture, TaskId};

const OUTPUT_WIDTH: f32 = 1920.0;
const ICONS: &str = "/usr/share/icons/Colloid/apps/scalable";
const URI_LIST: &str = "text/uri-list";
/// Tasks that never finish while the dock lives: the `dock.ron` watcher.
const LIVE_TASKS: usize = 1;
const PUMP_LIMIT: Duration = Duration::from_secs(5);
const PUMP_STEP: Duration = Duration::from_millis(40);

const DOCK: &str = "Current build · Dock";
const RUNNING: &str = "Current build · Running apps";
const POINTER: &str = "Current build · Pointer";
const DRAG: &str = "Current build · Drag & drop";
const AUTOHIDE: &str = "Current build · Autohide";
const FALLBACK: &str = "Current build · Loading, empty & fallback";

/// `(file stem, name, StartupWMClass, icon file)` of each sample app.
const PINNED: [(&str, &str, &str, &str); 9] = [
    (
        "crownlauncher",
        "Launcher",
        "crownlauncher",
        "system-search",
    ),
    (
        "crownos-files",
        "Files",
        "crownos-files",
        "org.gnome.Nautilus",
    ),
    ("firefox", "Firefox", "firefox", "firefox"),
    ("terminal", "Terminal", "terminal", "org.gnome.Terminal"),
    (
        "crownappstore",
        "App Store",
        "crownappstore",
        "system-software-install",
    ),
    (
        "crownos-settings",
        "Settings",
        "crownsettings",
        "preferences-system",
    ),
    (
        "crownos-process-monitor",
        "Process Monitor",
        "crownos-process-monitor",
        "utilities-system-monitor",
    ),
    ("crownos-camera", "Camera", "crownos-camera", "cheese"),
    (
        "crownos-preview",
        "Preview",
        "crownpreview",
        "org.gnome.Loupe",
    ),
];
const UNPINNED: [(&str, &str, &str, &str); 2] = [
    ("code", "Visual Studio Code", "code", "visual-studio-code"),
    ("telegram", "Telegram", "telegram", "telegram"),
];

type TaskOutput = (TaskId, Box<dyn Any + Send>);

/// Runs the dock's `cx.spawn` futures on a tokio runtime, handing their outputs back.
struct TokioTasks {
    handle: Handle,
    done: Sender<TaskOutput>,
}

impl TaskExecutor for TokioTasks {
    fn spawn(&mut self, id: TaskId, future: TaskFuture) {
        let done = self.done.clone();
        self.handle.spawn(async move {
            let _ = done.send((id, future.await));
        });
    }

    fn cancel(&mut self, _id: TaskId) {}
}

struct Fixtures {
    applications: PathBuf,
    tasks: Handle,
}

impl Fixtures {
    fn new(root: &Path) -> Result<Self> {
        let applications = root.join("applications");
        std::fs::create_dir_all(&applications)?;
        for (stem, name, class, icon) in PINNED.iter().chain(&UNPINNED) {
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName={name}\nIcon={ICONS}/{icon}.svg\nStartupWMClass={class}\n"
            );
            std::fs::write(applications.join(format!("{stem}.desktop")), entry)?;
        }
        // Safety: set before any thread starts, so nothing reads the environment concurrently.
        unsafe {
            std::env::set_var("XDG_DATA_HOME", root);
            std::env::set_var("XDG_DATA_DIRS", root);
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let tasks = runtime.handle().clone();
        std::thread::Builder::new()
            .name("design-capture-tasks".into())
            .spawn(move || runtime.block_on(std::future::pending::<()>()))?;
        Ok(Self {
            applications,
            tasks,
        })
    }

    fn settings(&self, pinned: usize, autohide: bool) -> DockSettings {
        DockSettings {
            items: PINNED[..pinned]
                .iter()
                .map(|(stem, ..)| self.applications.join(format!("{stem}.desktop")))
                .collect(),
            autohide,
            ..DockSettings::default()
        }
    }

    fn open(&self, settings: &DockSettings, glass: Glass) -> Result<Dock> {
        crownconfig::save_section(DockSettings::SECTION, settings)?;
        let metrics = Metrics::new(settings);
        let surface = Size::new(OUTPUT_WIDTH, metrics.surface_height() as f32);
        let mut app = capture::mount_app(
            crowndock::root(settings.clone()),
            &crowndock::app_config(settings),
            Some((surface.width, surface.height)),
        )?;
        let composited = glass == Glass::Composited;
        app.ui().set_platform_capabilities(Capabilities {
            materials: composited,
            layer_shell: true,
            app_icons: true,
            ..Capabilities::default()
        });
        app.ui().set_emulate_materials(composited);
        let (done, outputs) = channel();
        app.ui().set_task_executor(Box::new(TokioTasks {
            handle: self.tasks.clone(),
            done,
        }));
        Ok(Dock {
            app,
            outputs,
            metrics,
            surface,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glass {
    /// The compositor blurs behind the dock, as on CrownOS.
    Composited,
    /// No compositor blur: the nearly solid tint.
    Solid,
}

struct Dock {
    app: CaptureApp,
    outputs: Receiver<TaskOutput>,
    metrics: Metrics,
    surface: Size,
}

impl Dock {
    /// Finishes the dock's background work (icon decoding, reveal timers) and settles.
    fn pump(&mut self) -> &mut Self {
        let deadline = Instant::now() + PUMP_LIMIT;
        self.app.settle();
        while self.app.ui().pending_tasks() > LIVE_TASKS && Instant::now() < deadline {
            if let Ok((id, output)) = self.outputs.recv_timeout(PUMP_STEP) {
                self.app.ui().complete_task(id, output);
                self.app.settle();
            }
        }
        self.app.settle();
        self
    }

    fn icon(&self, index: usize, tiles: usize) -> Point {
        self.metrics
            .resting_icon(index, tiles, self.surface)
            .center()
    }

    fn windows(&mut self, app_id: &str, count: u32, focused: bool) -> &mut Self {
        self.app.app_windows(AppWindows {
            app_id: app_id.to_owned(),
            count,
            focused,
        });
        self.pump()
    }

    fn capture(
        &mut self,
        carrier: &mut Carrier,
        group: &str,
        name: &str,
        description: &str,
    ) -> Result<()> {
        carrier.capture_state(&mut self.app, group, name, description)?;
        Ok(())
    }
}

fn main() -> Result<()> {
    if std::env::var_os("CROWN_CONFIG_DIR").is_none() {
        bail!("run through capture-app.sh: the dock saves dock.ron into CROWN_CONFIG_DIR");
    }
    let design_dir = std::env::var_os("CROWN_DESIGN_DIR")
        .map(PathBuf::from)
        .context("CROWN_DESIGN_DIR is not set")?;
    let fixtures = Fixtures::new(&design_dir.join(".fixtures"))?;
    let mut carrier = Carrier::new(
        "Dock",
        "crowndock: the bottom-edge dock layer surface (1920 wide) with pinned and running apps, \
         window marks, pointer, drag-and-drop, autohide and fallback states, captured headlessly \
         from the current source.",
    );
    let all = PINNED.len();
    let shown = fixtures.settings(all, false);

    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        dock.capture(
            &mut carrier,
            DOCK,
            "Pinned apps",
            "Nine pinned apps at rest on the glass, none running",
        )?;
    }
    {
        let mut dock = fixtures.open(&fixtures.settings(4, false), Glass::Composited)?;
        dock.pump();
        dock.capture(
            &mut carrier,
            DOCK,
            "Few apps",
            "The glass springs to fit a short row of four",
        )?;
    }

    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        dock.windows("crownos-files", 1, false)
            .windows("terminal", 2, false)
            .windows("crownsettings", 5, false)
            .windows("firefox", 3, true);
        dock.capture(&mut carrier, RUNNING, "Window marks", "A dot per window up to three (Files 1, Terminal 2, Settings 5) and the accent pill on focused Firefox")?;
        dock.windows("code", 1, false).windows("telegram", 2, false);
        dock.capture(
            &mut carrier,
            RUNNING,
            "Unpinned running apps",
            "Apps with windows that are not pinned follow the pinned ones",
        )?;
        dock.windows("firefox", 3, false).windows("code", 1, true);
        dock.capture(
            &mut carrier,
            RUNNING,
            "Focus on unpinned app",
            "Focus moved to Visual Studio Code: Firefox falls back to dots",
        )?;
    }
    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        dock.windows("firefox", 1, true);
        dock.app.lift_app_icon("firefox");
        dock.pump();
        dock.capture(&mut carrier, RUNNING, "Icon carried by a window", "The compositor carries Firefox's icon through a window opening or minimizing; its place stays undrawn")?;
    }

    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        let target = dock.icon(2, all);
        dock.app.pointer_move(target.x, target.y);
        dock.pump();
        dock.capture(
            &mut carrier,
            POINTER,
            "Hovered icon",
            "Firefox grows from its foot under the pointer",
        )?;
        dock.app.pointer_down(target.x, target.y);
        dock.pump();
        dock.capture(
            &mut carrier,
            POINTER,
            "Pressed icon",
            "Firefox sinks while the button is held",
        )?;
    }

    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        let (x, y) = (dock.icon(4, all + 1).x, dock.surface.height / 2.0);
        dock.app
            .drag_enter(x, y, &[URI_LIST], DndActions::COPY_MOVE);
        dock.app.drag_over(x, y);
        dock.pump();
        dock.capture(
            &mut carrier,
            DRAG,
            "Drop target",
            "An app dragged in from the launcher opens a gap where it would be pinned",
        )?;
    }
    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.pump();
        let start = dock.icon(3, all);
        dock.app.pointer_down(start.x, start.y);
        for step in 1..=8 {
            dock.app
                .pointer_move(start.x + step as f32 * 6.0, start.y - step as f32 * 12.0);
        }
        dock.pump();
        dock.capture(
            &mut carrier,
            DRAG,
            "Icon lifted out",
            "Terminal dragged off the dock: a gap holds its place until it lands or is unpinned",
        )?;
        let to = dock.icon(6, all);
        dock.app.drag_enter(
            to.x,
            dock.surface.height / 2.0,
            &[URI_LIST],
            DndActions::COPY_MOVE,
        );
        dock.app.drag_over(to.x, dock.surface.height / 2.0);
        dock.pump();
        dock.capture(
            &mut carrier,
            DRAG,
            "Reordering",
            "Terminal carried back over the dock: the gap follows the pointer",
        )?;
    }

    let hiding = fixtures.settings(all, true);
    {
        let mut dock = fixtures.open(&hiding, Glass::Composited)?;
        dock.pump();
        dock.capture(&mut carrier, AUTOHIDE, "Hidden", "Autohide: the dock is below the screen edge, nothing is drawn but the 2 px trigger strip")?;
        dock.app
            .pointer_move(OUTPUT_WIDTH / 2.0, dock.surface.height - 1.0);
        dock.pump();
        dock.capture(
            &mut carrier,
            AUTOHIDE,
            "Revealed",
            "The pointer touched the bottom edge and the dock slid up",
        )?;
    }

    {
        let mut dock = fixtures.open(&shown, Glass::Composited)?;
        dock.app.settle();
        dock.capture(
            &mut carrier,
            FALLBACK,
            "Icons loading",
            "Placeholder squircles until the icons are decoded",
        )?;
    }
    {
        let mut dock = fixtures.open(&fixtures.settings(0, false), Glass::Composited)?;
        dock.pump();
        dock.capture(
            &mut carrier,
            FALLBACK,
            "Empty dock",
            "Nothing pinned or running: the glass keeps room for one icon as a drop target",
        )?;
    }
    {
        let mut dock = fixtures.open(&shown, Glass::Solid)?;
        dock.pump();
        dock.windows("firefox", 1, true)
            .windows("terminal", 2, false);
        dock.capture(
            &mut carrier,
            FALLBACK,
            "No compositor blur",
            "Without the compositor's glass the dock paints a nearly solid tint",
        )?;
    }

    let path = carrier.write(None)?;
    println!("{}", path.display());
    capture::finish()
}

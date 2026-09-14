//! Dragging, persistence, and file reloads without a window.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sysy_core::{Design, Layout};
use sysy_layout::Point;

pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Compute absolute positions from the layout at the start of a drag.
#[must_use]
pub fn dragged_positions(design: &Design, positions: &Layout, id: &str, delta: Point) -> Layout {
    let mut ids = BTreeSet::new();
    if design.containers.iter().any(|container| container.id == id) {
        ids.insert(id.to_owned());
        loop {
            let before = ids.len();
            for container in &design.containers {
                if container.parent.as_ref().is_some_and(|id| ids.contains(id)) {
                    ids.insert(container.id.clone());
                }
            }
            if ids.len() == before {
                break;
            }
        }
        for node in &design.nodes {
            if node.container.as_ref().is_some_and(|id| ids.contains(id)) {
                ids.insert(node.id.clone());
            }
        }
    } else if design.nodes.iter().any(|node| node.id == id) {
        ids.insert(id.to_owned());
    }
    ids.into_iter()
        .filter_map(|id| {
            let mut entry = *positions.get(&id)?;
            entry.x += delta.x;
            entry.y += delta.y;
            Some((id, entry))
        })
        .collect()
}

fn position_ids(design: &Design) -> BTreeSet<&str> {
    design
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .chain(
            design
                .containers
                .iter()
                .map(|container| container.id.as_str()),
        )
        .chain(design.notes.iter().map(|note| note.id.as_str()))
        .collect()
}

/// Keep the freshly loaded elements and unrelated layout entries, even if an
/// element was deleted while it was being dragged.
pub fn merge_positions(design: &mut Design, changed: &Layout) {
    let ids = position_ids(design);
    let changed: Layout = changed
        .iter()
        .filter(|(id, _)| ids.contains(id.as_str()))
        .map(|(id, entry)| {
            let mut entry = *entry;
            if !design
                .containers
                .iter()
                .any(|container| container.id == *id)
            {
                entry.size = None;
            }
            (id.clone(), entry)
        })
        .collect();
    design.layout.extend(changed);
}

/// Reload immediately before applying only the dragged entries and saving atomically.
///
/// # Errors
/// Returns the load, validation, or atomic save error without overwriting an invalid file.
pub fn save_positions(path: &Path, changed: &Layout) -> Result<Design, sysy_core::Error> {
    let mut design = sysy_core::load(path)?;
    merge_positions(&mut design, changed);
    sysy_core::save(path, &design)?;
    Ok(design)
}

/// The watcher covers one directory, so every path in a rename event must be
/// checked. Access events from our own reads must not trigger another reload.
#[must_use]
pub fn event_matches(event: &Event, path: &Path) -> bool {
    !matches!(event.kind, EventKind::Access(_))
        && path.file_name().is_some()
        && event
            .paths
            .iter()
            .any(|changed| changed.file_name() == path.file_name())
}

#[derive(Default)]
pub struct Debounce {
    last_event: Option<Instant>,
}

impl Debounce {
    pub fn record(&mut self, now: Instant) {
        self.last_event = Some(self.last_event.map_or(now, |last| last.max(now)));
    }

    pub fn take_ready(&mut self, now: Instant) -> bool {
        if self
            .last_event
            .is_some_and(|last| now.saturating_duration_since(last) >= DEBOUNCE)
        {
            self.last_event = None;
            true
        } else {
            false
        }
    }
}

struct Drag {
    start: Point,
    original: Layout,
    changed: Layout,
}

pub struct ViewerState {
    pub design: Design,
    pub positions: Layout,
    pub selected: Option<String>,
    pub last_reload: SystemTime,
    pub error: Option<String>,
    drag: Option<Drag>,
}

impl ViewerState {
    #[must_use]
    pub fn new(design: Design) -> Self {
        Self {
            positions: sysy_layout::layout(&design),
            design,
            selected: None,
            last_reload: SystemTime::now(),
            error: None,
            drag: None,
        }
    }

    fn replace(&mut self, design: Design) {
        // Keep transient positions separate from saved pins. Only new elements
        // need automatic placement; a reload must not rearrange existing ones.
        let ids = position_ids(&design);
        let mut positions = self.positions.clone();
        positions.retain(|id, _| ids.contains(id.as_str()));
        for (id, entry) in &design.layout {
            if self.design.layout.get(id) != Some(entry) || !positions.contains_key(id) {
                positions.insert(id.clone(), *entry);
            }
        }
        if ids.iter().any(|id| !positions.contains_key(*id)) {
            let mut input = design.clone();
            input.layout = positions.clone();
            for (id, entry) in sysy_layout::layout(&input) {
                positions.entry(id).or_insert(entry);
            }
        }
        if self
            .selected
            .as_deref()
            .is_some_and(|id| !ids.contains(id) && !design.edges.iter().any(|edge| edge.id == id))
        {
            self.selected = None;
        }
        self.positions = positions;
        self.design = design;
        self.last_reload = SystemTime::now();
        self.error = None;
    }

    pub fn reload(&mut self, path: &Path) {
        match sysy_core::load(path) {
            Ok(design) => self.replace(design),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn begin_drag(&mut self, id: &str, cursor: Point) {
        let original = dragged_positions(&self.design, &self.positions, id, Point::default());
        self.drag = (!original.is_empty()).then_some(Drag {
            start: cursor,
            original,
            changed: Layout::new(),
        });
    }

    pub fn drag_to(&mut self, cursor: Point) {
        if let Some(drag) = &mut self.drag {
            let delta = Point {
                x: cursor.x - drag.start.x,
                y: cursor.y - drag.start.y,
            };
            drag.changed = drag
                .original
                .iter()
                .map(|(id, entry)| {
                    let mut entry = *entry;
                    entry.x += delta.x;
                    entry.y += delta.y;
                    (id.clone(), entry)
                })
                .collect();
            self.positions.extend(drag.changed.clone());
        }
    }

    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            self.positions.extend(drag.original);
        }
    }

    pub fn finish_drag(&mut self, path: &Path) {
        let Some(drag) = self.drag.take() else { return };
        if drag.changed.is_empty() || drag.changed == drag.original {
            return;
        }
        match save_positions(path, &drag.changed) {
            Ok(design) => self.replace(design),
            Err(error) => {
                self.positions.extend(drag.original);
                self.error = Some(error.to_string());
            }
        }
    }

    #[must_use]
    pub fn status(&self) -> String {
        let seconds = self
            .last_reload
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            % 86400;
        let time = format!(
            "Last reload {:02}:{:02}:{:02} UTC",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        );
        match &self.error {
            Some(error) => format!("{time}\n{error}"),
            None => time,
        }
    }
}

/// Owns the directory watch for the same lifetime as the viewer.
pub struct LiveDesign {
    pub state: ViewerState,
    path: PathBuf,
    _watcher: RecommendedWatcher,
    events: Receiver<(Instant, notify::Result<Event>)>,
    debounce: Debounce,
}

impl LiveDesign {
    /// Register the watch before loading, so changes during startup are queued.
    ///
    /// # Errors
    /// Returns an error if the watch cannot be registered or the initial file is invalid.
    pub fn open(path: &Path) -> Result<Self, crate::Error> {
        let path = std::env::current_dir()
            .map_err(sysy_core::Error::from)?
            .join(path);
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send((Instant::now(), event));
        })?;
        watcher.watch(
            path.parent().unwrap_or_else(|| Path::new(".")),
            RecursiveMode::NonRecursive,
        )?;
        let state = ViewerState::new(sysy_core::load(&path)?);
        Ok(Self {
            state,
            path,
            _watcher: watcher,
            events,
            debounce: Debounce::default(),
        })
    }

    /// Poll from the UI executor or a test. Reloads wait until the drag ends so
    /// a disk update cannot replace the geometry under the cursor.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.state.is_dragging() {
            return false;
        }
        let mut updated = false;
        for (time, event) in self.events.try_iter() {
            match event {
                Ok(event) if event.need_rescan() || event_matches(&event, &self.path) => {
                    self.debounce.record(time);
                }
                Err(error) => {
                    self.state.error = Some(format!("File watch failed: {error}"));
                    updated = true;
                }
                Ok(_) => {}
            }
        }
        if self.debounce.take_ready(now) {
            self.state.reload(&self.path);
            updated = true;
        }
        updated
    }

    pub fn finish_drag(&mut self) {
        self.state.finish_drag(&self.path);
    }
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    App, Application, BorderStyle, Bounds, ContentMask, Context, Div, FocusHandle, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Path, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, SharedString, Stateful, TextAlign, TextRun, Window, WindowBounds,
    WindowOptions, canvas, div, fill, point, prelude::*, px, quad, rgb, size, transparent_black,
};
use sysy_core::{Design, NodeKind};
use sysy_layout::{Point as WorldPoint, Rect, Size};

use crate::scene::{Camera, Label, Primitive, Scene, Shape, ShapeKind, detail_text, highlighted};

#[derive(Clone, Copy, Default)]
struct Shared {
    camera: Camera,
    origin: Point<Pixels>,
    fit_pending: bool,
}

struct Viewer {
    design: Design,
    scene: Rc<Scene>,
    shared: Rc<Cell<Shared>>,
    selected: Option<String>,
    hovered: Option<String>,
    pan_start: Option<Point<Pixels>>,
    focus: FocusHandle,
}

impl Viewer {
    fn new(design: Design, cx: &mut Context<Self>) -> Self {
        let scene = Rc::new(Scene::build(&design, &sysy_layout::layout(&design)));
        Self {
            design,
            scene,
            shared: Rc::new(Cell::new(Shared {
                fit_pending: true,
                ..Shared::default()
            })),
            selected: None,
            hovered: None,
            pan_start: None,
            focus: cx.focus_handle(),
        }
    }

    fn local(&self, position: Point<Pixels>) -> WorldPoint {
        let origin = self.shared.get().origin;
        WorldPoint {
            x: f64::from(f32::from(position.x - origin.x)),
            y: f64::from(f32::from(position.y - origin.y)),
        }
    }

    fn hit(&self, position: Point<Pixels>) -> Option<String> {
        let camera = self.shared.get().camera;
        self.scene
            .hit_test(
                camera.screen_to_world(self.local(position)),
                5.0 / camera.scale,
            )
            .map(str::to_owned)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.selected = self.hit(event.position);
        self.pan_start = self.selected.is_none().then_some(event.position);
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.pan_start = None;
        }
        if let Some(last) = self.pan_start {
            let mut shared = self.shared.get();
            shared.camera.pan_by(WorldPoint {
                x: f64::from(f32::from(event.position.x - last.x)),
                y: f64::from(f32::from(event.position.y - last.y)),
            });
            self.shared.set(shared);
            self.pan_start = Some(event.position);
            cx.notify();
        }
        let hovered = self.hit(event.position);
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let mut shared = self.shared.get();
        match event.delta {
            ScrollDelta::Lines(delta) => shared.camera.zoom_at(
                self.local(event.position),
                1.12_f64.powf(f64::from(delta.y)),
            ),
            ScrollDelta::Pixels(delta) if event.modifiers.control || event.modifiers.platform => {
                shared.camera.zoom_at(
                    self.local(event.position),
                    (f64::from(f32::from(delta.y)) / 160.0).exp(),
                );
            }
            ScrollDelta::Pixels(delta) => shared.camera.pan_by(WorldPoint {
                x: f64::from(f32::from(delta.x)),
                y: f64::from(f32::from(delta.y)),
            }),
        }
        self.shared.set(shared);
        self.hovered = self.hit(event.position);
        cx.notify();
    }

    fn graph(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let scene = Rc::clone(&self.scene);
        let shared = Rc::clone(&self.shared);
        let bounds = scene.bounds;
        let hovered = self.hovered.clone();
        let selected = self.selected.clone();
        div()
            .id("canvas")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.pan_start = None),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.pan_start = None),
            )
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                if !over {
                    this.hovered = None;
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(
                canvas(
                    move |area, _, _| {
                        let mut state = shared.get();
                        state.origin = area.origin;
                        if state.fit_pending
                            && area.size.width > px(0.0)
                            && area.size.height > px(0.0)
                        {
                            state.camera = Camera::fit(
                                bounds,
                                Size {
                                    width: f64::from(f32::from(area.size.width)),
                                    height: f64::from(f32::from(area.size.height)),
                                },
                                36.0,
                            );
                            state.fit_pending = false;
                        }
                        shared.set(state);
                        state.camera
                    },
                    move |area, camera, window, cx| {
                        window.with_content_mask(Some(ContentMask { bounds: area }), |window| {
                            window.paint_quad(fill(area, rgb(0x00fa_faf7)));
                            let transform = Transform {
                                camera,
                                origin: area.origin,
                            };
                            for shape in &scene.shapes {
                                let active = highlighted(shape, hovered.as_deref())
                                    || selected.as_deref() == Some(shape.id.as_str());
                                paint_shape(shape, transform, active, window, cx);
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    fn panel(&self) -> Stateful<Div> {
        let fields = self
            .selected
            .as_deref()
            .and_then(|id| detail_text(&self.design, id));
        let content = if let Some(fields) = fields {
            div()
                .flex()
                .flex_col()
                .gap_4()
                .children(fields.into_iter().map(|(heading, text)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_xs().text_color(rgb(0x0064_748b)).child(heading))
                        .child(div().text_sm().child(text))
                }))
        } else {
            div()
                .text_sm()
                .child("Click an element to see its details.")
        };
        div()
            .id("details")
            .w(px(300.0))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .p_4()
            .bg(rgb(0x00ff_ffff))
            .border_l_1()
            .border_color(rgb(0x00cb_d5e1))
            .child(div().text_lg().mb_4().child(self.design.title.clone()))
            .child(content)
            .child(div().mt_6().text_xs().text_color(rgb(0x0064_748b)).child(
                "Drag empty canvas or scroll to pan. Wheel or Ctrl-scroll to zoom. Press f to fit.",
            ))
    }
}

impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        div()
            .flex()
            .size_full()
            .text_color(rgb(0x001e_293b))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "f"
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.platform
                {
                    let mut shared = this.shared.get();
                    shared.fit_pending = true;
                    this.shared.set(shared);
                    cx.notify();
                }
            }))
            .child(self.graph(cx))
            .child(self.panel())
    }
}

#[derive(Clone, Copy)]
struct Transform {
    camera: Camera,
    origin: Point<Pixels>,
}

// GPUI uses f32 pixels; world geometry stays f64 until this rendering boundary.
#[allow(clippy::cast_possible_truncation)]
fn pixels(value: f64) -> Pixels {
    px(value as f32)
}

impl Transform {
    fn point(self, p: WorldPoint) -> Point<Pixels> {
        let p = self.camera.world_to_screen(p);
        point(self.origin.x + pixels(p.x), self.origin.y + pixels(p.y))
    }
    fn rect(self, rect: Rect) -> Bounds<Pixels> {
        Bounds {
            origin: self.point(rect.origin),
            size: size(
                pixels(rect.size.width * self.camera.scale),
                pixels(rect.size.height * self.camera.scale),
            ),
        }
    }
}

fn fill_color(kind: ShapeKind) -> u32 {
    match kind {
        ShapeKind::Container => 0x00f1_f5f9,
        ShapeKind::Note => 0x00fe_f3c7,
        ShapeKind::Node(NodeKind::Database | NodeKind::Storage) => 0x00dc_fce7,
        ShapeKind::Node(NodeKind::Queue | NodeKind::Cache) => 0x00ff_edd5,
        ShapeKind::Node(NodeKind::External) => 0x00f3_e8ff,
        ShapeKind::Node(_) => 0x00e0_f2fe,
        ShapeKind::Edge(_) => 0x00fa_faf7,
    }
}

fn paint_shape(
    shape: &Shape,
    transform: Transform,
    active: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let outline = rgb(if active { 0x0002_84c7 } else { 0x0064_748b });
    let background = rgb(fill_color(shape.kind));
    let scale = transform.camera.scale;
    for primitive in &shape.primitives {
        match primitive {
            Primitive::Box {
                rect,
                radius,
                filled,
                border,
            } => window.paint_quad(quad(
                transform.rect(*rect),
                pixels(radius * scale),
                if *filled {
                    background.into()
                } else {
                    transparent_black()
                },
                pixels(if *border {
                    (if active { 2.5 } else { 1.2 }) * scale
                } else {
                    0.0
                }),
                outline,
                BorderStyle::default(),
            )),
            Primitive::Stroke { points, width } => {
                for pair in points.windows(2) {
                    let a = pair[0];
                    let b = pair[1];
                    let length = (b.x - a.x).hypot(b.y - a.y);
                    if length < 0.001 {
                        continue;
                    }
                    let half = (width + if active { 1.0 } else { 0.0 }) / 2.0;
                    let nx = -(b.y - a.y) / length * half;
                    let ny = (b.x - a.x) / length * half;
                    polygon(
                        &[
                            WorldPoint {
                                x: a.x + nx,
                                y: a.y + ny,
                            },
                            WorldPoint {
                                x: b.x + nx,
                                y: b.y + ny,
                            },
                            WorldPoint {
                                x: b.x - nx,
                                y: b.y - ny,
                            },
                            WorldPoint {
                                x: a.x - nx,
                                y: a.y - ny,
                            },
                        ],
                        transform,
                        outline,
                        window,
                    );
                }
            }
            Primitive::Polygon(points) => polygon(
                points,
                transform,
                if matches!(shape.kind, ShapeKind::Edge(_)) {
                    outline
                } else {
                    background
                },
                window,
            ),
        }
    }
    for label in &shape.labels {
        paint_label(label, transform, window, cx);
    }
}

fn polygon(points: &[WorldPoint], transform: Transform, color: gpui::Rgba, window: &mut Window) {
    if let Some(first) = points.first() {
        let mut path = Path::new(transform.point(*first));
        for p in &points[1..] {
            path.line_to(transform.point(*p));
        }
        window.paint_path(path, color);
    }
}

fn text_lines(
    text: &str,
    font_size: Pixels,
    width: Pixels,
    window: &Window,
) -> Vec<gpui::WrappedLine> {
    let text = SharedString::from(text.to_owned());
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: rgb(0x001e_293b).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    match window
        .text_system()
        .shape_text(text, font_size, &[run], Some(width), None)
    {
        Ok(lines) => lines.into_vec(),
        Err(_) => Vec::new(),
    }
}

fn paint_label(label: &Label, transform: Transform, window: &mut Window, cx: &mut App) {
    let area = transform.rect(label.rect);
    let font_size = pixels(label.font_size * transform.camera.scale);
    if font_size < px(2.0) {
        return;
    }
    let line_height = font_size * 1.25;
    let fits = |lines: &[gpui::WrappedLine]| {
        lines
            .iter()
            .map(|line| line.size(line_height).height)
            .fold(px(0.0), |total, height| total + height)
            <= area.size.height
    };
    let mut lines = text_lines(&label.text, font_size, area.size.width, window);
    if !fits(&lines) {
        let ends: Vec<_> = label
            .text
            .char_indices()
            .map(|(i, _)| i)
            .chain([label.text.len()])
            .collect();
        let (mut low, mut high) = (0, ends.len() - 1);
        while low < high {
            let mid = (low + high).div_ceil(2);
            let candidate = format!("{}…", &label.text[..ends[mid]]);
            if fits(&text_lines(&candidate, font_size, area.size.width, window)) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        lines = text_lines(
            &format!("{}…", &label.text[..ends[low]]),
            font_size,
            area.size.width,
            window,
        );
    }
    let height = lines
        .iter()
        .map(|line| line.size(line_height).height)
        .fold(px(0.0), |total, height| total + height);
    let mut origin = area.origin;
    if label.centered {
        origin.y += (area.size.height - height) / 2.0;
    }
    window.with_content_mask(Some(ContentMask { bounds: area }), |window| {
        for line in lines {
            let align = if label.centered {
                TextAlign::Center
            } else {
                TextAlign::Left
            };
            let _ = line.paint(origin, line_height, align, Some(area), window, cx);
            origin.y += line.size(line_height).height;
        }
    });
}

pub(crate) fn run(design: Design) -> Result<(), crate::Error> {
    let error = Rc::new(RefCell::new(None));
    let open_error = Rc::clone(&error);
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(820.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(format!("sysy — {}", design.title).into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| Viewer::new(design, cx)),
        );
        if let Err(failure) = result {
            *open_error.borrow_mut() = Some(crate::Error::Window(failure.to_string()));
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
    error.take().map_or(Ok(()), Err)
}

use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{app_state::RenderImage, i18n::Language, widgets};

use std::marker::PhantomData;
use std::sync::Arc;

use conrod_core::*;

pub struct Viewport3D;

pub struct Viewport2D;

pub trait ViewportType {
    fn renderer_type() -> RendererType;
}

impl ViewportType for Viewport3D {
    fn renderer_type() -> RendererType {
        RendererType::Renderer3D
    }
}

impl ViewportType for Viewport2D {
    fn renderer_type() -> RendererType {
        RendererType::Renderer2D
    }
}

#[derive(WidgetCommon)]
pub struct Viewport<'a, V, B: crate::gpu::Backend> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    renderer: &'a mut crate::gpu::ui::Renderer<B>,
    image_map: &'a mut image::Map<crate::gpu::ui::Image<B>>,
    monitor_resolution: (u32, u32),
    event_buffer: Option<&'a [Arc<Lang>]>,
    style: Style,
    _viewport_type: PhantomData<V>,
}

impl<'a, B> Viewport<'a, Viewport3D, B>
where
    B: crate::gpu::Backend,
{
    pub fn new_3d(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        renderer: &'a mut crate::gpu::ui::Renderer<B>,
        image_map: &'a mut image::Map<crate::gpu::ui::Image<B>>,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            renderer,
            image_map,
            monitor_resolution: (1920, 1080),
            event_buffer: None,
            style: Style::default(),
            _viewport_type: PhantomData,
        }
    }
}

impl<'a, B> Viewport<'a, Viewport2D, B>
where
    B: crate::gpu::Backend,
{
    pub fn new_2d(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        renderer: &'a mut crate::gpu::ui::Renderer<B>,
        image_map: &'a mut image::Map<crate::gpu::ui::Image<B>>,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            renderer,
            image_map,
            monitor_resolution: (1920, 1080),
            event_buffer: None,
            style: Style::default(),
            _viewport_type: PhantomData,
        }
    }
}

impl<'a, V, B> Viewport<'a, V, B>
where
    B: crate::gpu::Backend,
{
    builder_methods! {
        pub event_buffer { event_buffer = Some(&'a [Arc<Lang>]) }
        pub icon_font { style.icon_font = Some(text::font::Id) }
        pub monitor_resolution { monitor_resolution = (u32, u32) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        inner,
        modal,
        parameters,
    }
}

pub struct State {
    ids: Ids,
    modal: bool,
    parameters: Option<ParamBoxDescription<RenderField>>,
    render_image: RenderImage,
}

impl<'a, V, B> Widget for Viewport<'a, V, B>
where
    V: ViewportType,
    B: crate::gpu::Backend,
{
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            modal: false,
            parameters: None,
            render_image: RenderImage::None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(mut self, args: widget::UpdateArgs<Self>) -> Self::Event {
        use widgets::render_view;

        let widget::UpdateArgs {
            state,
            ui,
            id,
            style,
            ..
        } = args;

        let renderer_id = state.ids.inner.index() as u64;

        if let Some(ev_buf) = self.event_buffer {
            for ev in ev_buf {
                self.handle_event(ui, state, renderer_id, ev);
            }
        }

        // If there is a known render image, create a render view for it
        match state.render_image {
            RenderImage::Image(render_image) => {
                let rv = render_view::RenderView::new(render_image, self.monitor_resolution)
                    .parent(id)
                    .wh_of(id)
                    .middle()
                    .set(state.ids.inner, ui);

                // The widget itself does not communicate with the backend. Process
                // events here
                match rv {
                    Some(render_view::Event::Resized(w, h)) => self
                        .sender
                        .send(Lang::UIEvent(UIEvent::RendererResize(renderer_id, w, h)))
                        .unwrap(),
                    Some(render_view::Event::Rotate(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Rotate(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(render_view::Event::Pan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Pan(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(render_view::Event::LightPan(x, y)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::LightMove(
                            renderer_id,
                            x,
                            y,
                        )))
                        .unwrap(),
                    Some(render_view::Event::Zoom(delta)) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::Zoom(
                            renderer_id,
                            delta,
                        )))
                        .unwrap(),
                    Some(render_view::Event::OpenModal) => {
                        state.update(|state| state.modal = true);
                    }
                    Some(render_view::Event::CenterCamera) => self
                        .sender
                        .send(Lang::UserRenderEvent(UserRenderEvent::CenterCamera(
                            renderer_id,
                        )))
                        .unwrap(),
                    None => {}
                }
            }
            RenderImage::None => {
                // Otherwise create one by notifying the render component
                let [w, h] = ui.wh_of(args.id).unwrap();
                self.sender
                    .send(Lang::UIEvent(UIEvent::RendererRequested(
                        renderer_id,
                        (self.monitor_resolution.0, self.monitor_resolution.1),
                        (w as u32, h as u32),
                        V::renderer_type(),
                    )))
                    .expect("Error contacting renderer backend");
                state.update(|state| state.render_image = RenderImage::Requested);
            }
            RenderImage::Requested => {}
        }

        if state.modal && state.parameters.is_some() {
            use widgets::modal;
            use widgets::param_box;

            match modal::Modal::canvas()
                .wh_of(id)
                .middle_of(id)
                .graphics_for(id)
                .set(state.ids.modal, ui)
            {
                modal::Event::ChildEvent((_, id)) => state.update(|state| {
                    for ev in param_box::ParamBox::new(
                        state.parameters.as_mut().unwrap(),
                        &renderer_id,
                        &self.language,
                    )
                    .parent(id)
                    .w_of(id)
                    .mid_top()
                    .icon_font(style.icon_font(&ui.theme))
                    .text_size(10)
                    .text_color(color::WHITE)
                    .set(state.ids.parameters, ui)
                    {
                        if let param_box::Event::ChangeParameter(lang) = ev {
                            self.sender.send(lang).unwrap()
                        }
                    }
                }),
                modal::Event::Hide => {
                    state.update(|state| state.modal = false);
                }
            }
        }
    }
}

impl<'a, V, B> Viewport<'a, V, B>
where
    B: crate::gpu::Backend,
{
    fn handle_event(
        &mut self,
        ui: &mut UiCell,
        state: &mut widget::State<State>,
        renderer_id: u64,
        event: &Lang,
    ) {
        match event {
            Lang::RenderEvent(RenderEvent::RendererAdded(id, view, pbox)) if *id == renderer_id => {
                if let Some(view) = view.clone().to::<B>() {
                    if let Some(img) = self.renderer.create_image(
                        view,
                        self.monitor_resolution.0,
                        self.monitor_resolution.1,
                    ) {
                        let id = self.image_map.insert(img);
                        state.update(|state| {
                            state.render_image = RenderImage::Image(id);
                            state.parameters = Some(pbox.clone());
                        });
                    }
                }
            }
            Lang::RenderEvent(RenderEvent::RendererRedrawn(id)) if *id == renderer_id => {
                ui.needs_redraw();
            }
            Lang::RenderEvent(RenderEvent::RendererRemoved(id)) if *id == renderer_id => state
                .update(|state| {
                    state.render_image = RenderImage::None;
                }),
            Lang::RenderEvent(RenderEvent::SettingsUpdated(id, pbox)) if *id == renderer_id => {
                state.update(|state| {
                    state.parameters = Some(pbox.clone());
                })
            }
            _ => {}
        }
    }
}

use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{app_state::RenderImage, i18n::Language, widgets};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct Viewport<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    render_image: &'a mut RenderImage,
    monitor_resolution: (u32, u32),
    style: Style,
}

impl<'a> Viewport<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        render_image: &'a mut RenderImage,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            render_image,
            monitor_resolution: (1920, 1080),
            style: Style::default(),
        }
    }

    pub fn icon_font(mut self, font_id: text::font::Id) -> Self {
        self.style.icon_font = Some(Some(font_id));
        self
    }

    pub fn monitor_resolution(mut self, resolution: (u32, u32)) -> Self {
        self.monitor_resolution = resolution;
        self
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id")]
    icon_font: Option<Option<text::font::Id>>,
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
    parameters: ParamBoxDescription<RenderField>,
}

impl<'a> Widget for Viewport<'a> {
    type State = State;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            modal: false,
            parameters: ParamBoxDescription::render_parameters(),
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        use widgets::render_view;

        let renderer_id = args.state.ids.inner.index() as u64;

        // If there is a known render image, create a render view for it
        match self.render_image {
            RenderImage::Image(render_image) => {
                let rv = render_view::RenderView::new(*render_image, self.monitor_resolution)
                    .parent(args.id)
                    .wh_of(args.id)
                    .middle()
                    .set(args.state.ids.inner, args.ui);

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
                        args.state.update(|state| state.modal = true);
                    }
                    _ => {}
                }
            }
            RenderImage::None => {
                // Otherwise create one by notifying the render component
                let [w, h] = args.ui.wh_of(args.id).unwrap();
                self.sender
                    .send(Lang::UIEvent(UIEvent::RendererRequested(
                        renderer_id,
                        (self.monitor_resolution.0, self.monitor_resolution.1),
                        (w as u32, h as u32),
                        RendererType::Renderer3D,
                    )))
                    .expect("Error contacting renderer backend");
                *self.render_image = RenderImage::Requested;
            }
            RenderImage::Requested => {}
        }

        if args.state.modal {
            use widgets::modal;
            use widgets::param_box;

            let ui = args.ui;

            match modal::Modal::canvas()
                .wh_of(args.id)
                .middle_of(args.id)
                .graphics_for(args.id)
                .set(args.state.ids.modal, ui)
            {
                modal::Event::ChildEvent((_, id)) => args.state.update(|state| {
                    for ev in param_box::ParamBox::new(
                        &mut state.parameters,
                        &renderer_id,
                        &self.language,
                    )
                    .parent(id)
                    .w_of(id)
                    .mid_top()
                    .icon_font(self.style.icon_font.unwrap().unwrap())
                    .set(state.ids.parameters, ui)
                    {
                        if let param_box::Event::ChangeParameter(lang) = ev {
                            self.sender.send(lang).unwrap()
                        }
                    }
                }),
                modal::Event::Hide => {
                    args.state.update(|state| state.modal = false);
                }
            }
        }
    }
}

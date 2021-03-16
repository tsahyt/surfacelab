use crate::broker::BrokerSender;
use crate::lang::*;
use crate::ui::{app_state, i18n::Language, widgets};

use conrod_core::*;

#[derive(WidgetCommon)]
pub struct GraphSection<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    language: &'a Language,
    sender: &'a BrokerSender<Lang>,
    graphs: &'a mut app_state::NodeCollections,
    style: Style,
}

impl<'a> GraphSection<'a> {
    pub fn new(
        language: &'a Language,
        sender: &'a BrokerSender<Lang>,
        graphs: &'a mut app_state::NodeCollections,
    ) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            language,
            sender,
            graphs,
            style: Style::default(),
        }
    }

    builder_methods! {
        pub icon_font { style.icon_font = Some(text::font::Id) }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {
    #[conrod(default = "theme.font_id.unwrap()")]
    icon_font: Option<text::font::Id>,
}

widget_ids! {
    pub struct Ids {
        param_box,
        layer_convert,
        exposed_param_title,
        exposed_param_list,
    }
}

impl<'a> Widget for GraphSection<'a> {
    type State = Ids;
    type Style = Style;
    type Event = ();

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        Ids::new(id_gen)
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        use widgets::exposed_param_row;
        use widgets::param_box;

        let active_graph = self.graphs.get_active().clone();

        let mut offset = 0.0;

        if self
            .graphs
            .get_active_collection_mut()
            .as_layers_mut()
            .is_some()
        {
            offset = 32.0;

            for _click in widget::Button::new()
                .label(&self.language.get_message("convert-to-graph"))
                .label_font_size(10)
                .parent(args.id)
                .padded_w_of(args.id, 16.0)
                .h(16.0)
                .mid_top_with_margin(16.0)
                .set(args.state.layer_convert, args.ui)
            {
                self.sender
                    .send(Lang::UserLayersEvent(UserLayersEvent::Convert(
                        active_graph.clone(),
                    )))
                    .unwrap();
            }
        }

        for ev in param_box::ParamBox::new(
            self.graphs.get_collection_parameters_mut(),
            &active_graph,
            &self.language,
        )
        .parent(args.id)
        .w_of(args.id)
        .mid_top_with_margin(32.0)
        .text_color(color::WHITE)
        .text_size(10)
        .set(args.state.param_box, args.ui)
        {
            if let param_box::Event::ChangeParameter(event) = ev {
                self.sender.send(event).unwrap()
            }
        }

        widget::Text::new(&self.language.get_message("exposed-parameters"))
            .parent(args.id)
            .color(color::WHITE)
            .font_size(12)
            .mid_top_with_margin(96.0 + offset)
            .set(args.state.exposed_param_title, args.ui);

        let exposed_params = self.graphs.get_exposed_parameters_mut();

        let (mut rows, scrollbar) = widget::List::flow_down(exposed_params.len())
            .parent(args.id)
            .padded_w_of(args.id, 8.0)
            .item_size(160.0)
            .h(320.0)
            .mid_top_with_margin(112.0 + offset)
            .scrollbar_on_top()
            .set(args.state.exposed_param_list, args.ui);

        while let Some(row) = rows.next(args.ui) {
            let widget = exposed_param_row::ExposedParamRow::new(
                &mut exposed_params[row.i].1,
                &self.language,
            )
            .icon_font(args.style.icon_font(&args.ui.theme))
            .icon_size(12)
            .text_size(10)
            .ctrl_size(16);

            if let Some(ev) = row.set(widget, args.ui) {
                match ev {
                    exposed_param_row::Event::ConcealParameter => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::ConcealParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateTitle => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RetitleParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.title.to_owned(),
                            )))
                            .unwrap();
                    }
                    exposed_param_row::Event::UpdateField => {
                        self.sender
                            .send(Lang::UserGraphEvent(UserGraphEvent::RefieldParameter(
                                active_graph.clone(),
                                exposed_params[row.i].0.clone(),
                                exposed_params[row.i].1.graph_field.to_owned(),
                            )))
                            .unwrap();
                    }
                }
            }
        }

        if let Some(s) = scrollbar {
            s.set(args.ui);
        }
    }
}

use crate::lang::{ChannelSpec, ExportSpec, ImageChannel, Resource, Socket};
use conrod_core::*;

pub struct RegisteredSocket {
    pub spec: ChannelSpec,
    formatted: String,
}

impl AsRef<str> for RegisteredSocket {
    fn as_ref(&self) -> &str {
        &self.formatted
    }
}

impl RegisteredSocket {
    pub fn new(spec: ChannelSpec) -> Self {
        Self {
            spec: spec.clone(),
            formatted: format!(
                "{}#{}",
                spec.0,
                match spec.1 {
                    ImageChannel::R => "R",
                    ImageChannel::G => "G",
                    ImageChannel::B => "B",
                    ImageChannel::A => "A",
                },
            ),
        }
    }
}

#[derive(WidgetCommon)]
pub struct ExportRow<'a> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    spec: &'a ExportSpec,
    sockets: &'a [RegisteredSocket],
    style: Style,
}

impl<'a> ExportRow<'a> {
    pub fn new(spec: &'a ExportSpec, sockets: &'a [RegisteredSocket]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            spec,
            sockets,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        image_type_label,
        image_type_selector,
        channel_r_label,
        channel_g_label,
        channel_b_label,
        channel_a_label,
        channel_r_selector,
        channel_g_selector,
        channel_b_selector,
        channel_a_selector,
    }
}

pub struct State {
    ids: Ids,
    r_selection: Option<(ChannelSpec, widget::drop_down_list::Idx)>,
    g_selection: Option<(ChannelSpec, widget::drop_down_list::Idx)>,
    b_selection: Option<(ChannelSpec, widget::drop_down_list::Idx)>,
    a_selection: Option<(ChannelSpec, widget::drop_down_list::Idx)>,
}

pub enum Event {
    ChangeToRGB,
    ChangeToRGBA,
    ChangeToGrayscale,
    SetChannelR(ChannelSpec),
    SetChannelG(ChannelSpec),
    SetChannelB(ChannelSpec),
    SetChannelA(ChannelSpec),
}

impl<'a> Widget for ExportRow<'a> {
    type State = State;
    type Style = Style;
    type Event = Option<Event>;

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
            r_selection: None,
            g_selection: None,
            b_selection: None,
            a_selection: None,
        }
    }

    fn style(&self) -> Self::Style {
        self.style
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut res = None;

        widget::Text::new("Type")
            .parent(args.id)
            .color(color::WHITE)
            .font_size(10)
            .top_left_with_margins(0., 16.0)
            .set(args.state.ids.image_type_label, args.ui);
        for new_selection in widget::DropDownList::new(
            &["RGB", "RGBA", "Grayscale"],
            Some(match self.spec {
                ExportSpec::RGBA(_) => 1,
                ExportSpec::RGB(_) => 0,
                ExportSpec::Grayscale(_) => 2,
            }),
        )
        .parent(args.id)
        .label_font_size(10)
        .padded_w_of(args.id, 16.0)
        .h(16.0)
        .set(args.state.ids.image_type_selector, args.ui)
        {
            match new_selection {
                0 => res = Some(Event::ChangeToRGB),
                1 => res = Some(Event::ChangeToRGBA),
                2 => res = Some(Event::ChangeToGrayscale),
                _ => unreachable!(),
            }
        }

        let (gb, a) = match self.spec {
            ExportSpec::RGBA(_) => (true, true),
            ExportSpec::RGB(_) => (true, false),
            ExportSpec::Grayscale(_) => (false, false),
        };

        if let Some(idx) = channel_widgets(
            args.id,
            "Channel R",
            args.state.ids.channel_r_label,
            args.state.ids.channel_r_selector,
            &self.sockets,
            args.state.r_selection.clone().map(|x| x.1),
            args.ui,
        ) {
            args.state
                .update(|state| state.r_selection = Some((self.sockets[idx].spec.clone(), idx)));
            res = Some(Event::SetChannelR(self.sockets[idx].spec.clone()));
        }

        if gb {
            if let Some(idx) = channel_widgets(
                args.id,
                "Channel G",
                args.state.ids.channel_g_label,
                args.state.ids.channel_g_selector,
                &self.sockets,
                args.state.g_selection.clone().map(|x| x.1),
                args.ui,
            ) {
                args.state.update(|state| {
                    state.g_selection = Some((self.sockets[idx].spec.clone(), idx))
                });
                res = Some(Event::SetChannelG(self.sockets[idx].spec.clone()))
            }
            if let Some(idx) = channel_widgets(
                args.id,
                "Channel B",
                args.state.ids.channel_b_label,
                args.state.ids.channel_b_selector,
                &self.sockets,
                args.state.b_selection.clone().map(|x| x.1),
                args.ui,
            ) {
                args.state.update(|state| {
                    state.b_selection = Some((self.sockets[idx].spec.clone(), idx))
                });
                res = Some(Event::SetChannelB(self.sockets[idx].spec.clone()))
            }
        }

        if a {
            if let Some(idx) = channel_widgets(
                args.id,
                "Channel A",
                args.state.ids.channel_a_label,
                args.state.ids.channel_a_selector,
                &self.sockets,
                args.state.a_selection.clone().map(|x| x.1),
                args.ui,
            ) {
                args.state.update(|state| {
                    state.a_selection = Some((self.sockets[idx].spec.clone(), idx))
                });
                res = Some(Event::SetChannelA(self.sockets[idx].spec.clone()))
            }
        }

        res
    }
}

fn channel_widgets(
    parent_id: widget::Id,
    chan_name: &str,
    label_id: widget::Id,
    selector_id: widget::Id,
    sockets: &[RegisteredSocket],
    selection: Option<widget::drop_down_list::Idx>,
    ui: &mut UiCell,
) -> Option<widget::drop_down_list::Idx> {
    widget::Text::new(chan_name)
        .parent(parent_id)
        .color(color::WHITE)
        .font_size(10)
        .set(label_id, ui);

    widget::DropDownList::new(&sockets, selection)
        .parent(parent_id)
        .label_font_size(10)
        .padded_w_of(parent_id, 16.0)
        .h(16.0)
        .set(selector_id, ui)
}

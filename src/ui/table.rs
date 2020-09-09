use conrod_core::*;
use std::any::*;

#[derive(Debug, WidgetCommon)]
pub struct Table<'a, R> {
    #[conrod(common_builder)]
    common: widget::CommonBuilder,
    style: Style,
    rows: &'a mut [R],
}

impl<'a, R> Table<'a, R> {
    pub fn new(rows: &'a mut [R]) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            rows,
        }
    }
}

#[derive(Copy, Clone, Default, Debug, WidgetStyle, PartialEq)]
pub struct Style {}

widget_ids! {
    pub struct Ids {
        rows
    }
}

impl<'a, R> Widget for Table<'a, R> where R: TableRow {
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
        let (mut items, scrollbar) = widget::List::flow_down(self.rows.len())
            .parent(args.id)
            .wh_of(args.id)
            .middle()
            .set(args.state.rows, args.ui);

        while let Some(item) = items.next(args.ui) {
            let i = item.i;
            let row = &mut self.rows[i];
            let n = row.row_length();

            let (mut cells, _) = widget::List::flow_left(n)
                .set(item.widget_id, args.ui);
            while let Some(cell) = cells.next(args.ui) {
                row.set_row_widget(cell.i, cell.widget_id, args.ui, item.widget_id);
            }
        }
    }
}

pub trait TableRow {
    fn row_length(&self) -> usize;
    fn set_row_widget(&mut self, i: usize, id: widget::Id, ui: &mut UiCell, parent: widget::Id);
}

use conrod_core::*;

pub trait Expandable {
    /// Query the current expansion state
    fn expanded(&self) -> bool;
}

fn visible_tree_items<T: Expandable>(tree: &id_tree::Tree<T>) -> usize {
    let mut stack: Vec<&id_tree::NodeId> = Vec::with_capacity(tree.height());

    if let Some(root) = tree.root_node_id() {
        let mut ns: usize = 0;
        stack.push(root);

        while !stack.is_empty() {
            let current = stack.pop().unwrap();
            if tree
                .get(current)
                .expect("Invalid node ID in tree")
                .data()
                .expanded()
            {
                ns += 1;
                stack.extend(tree.children_ids(current).unwrap());
            }
        }

        ns
    } else {
        0
    }
}

#[derive(Debug, WidgetCommon)]
pub struct Tree<'a, T: Expandable> {
    #[conrod(common_builder)]
    /// Common widget building params for the `Tree`.
    pub common: widget::CommonBuilder,
    /// Unique styling for the `Tree`.
    pub style: Style,
    tree: &'a mut id_tree::Tree<T>,
}

/// If the `List` is scrollable, this describes how th `Scrollbar` should be positioned.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ScrollbarPosition {
    /// To the right of the items (reduces the item width to fit).
    NextTo,
    /// On top of the right edge of the items with auto_hide activated.
    OnTop,
}

/// Unique styling for the `Tree`.
#[derive(Copy, Clone, Debug, Default, PartialEq, WidgetStyle)]
pub struct Style {
    /// The width of the scrollbar if it is visible.
    #[conrod(default = "None")]
    pub scrollbar_thickness: Option<Option<Scalar>>,
    /// The color of the scrollbar if it is visible.
    #[conrod(default = "theme.border_color")]
    pub scrollbar_color: Option<Color>,
    /// The location of the `Tree`'s scrollbar.
    #[conrod(default = "None")]
    pub scrollbar_position: Option<Option<ScrollbarPosition>>,
}

widget_ids! {
    struct Ids {
        list,
        expanders[],
    }
}

pub struct State {
    ids: Ids,
}

impl<'a, T> Tree<'a, T>
where
    T: Expandable,
{
    pub fn new(tree: &'a mut id_tree::Tree<T>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tree,
        }
    }

    /// Specifies that the `Tree` should be scrollable and should provide a `Scrollbar` to the
    /// right of the items.
    pub fn scrollbar_next_to(mut self) -> Self {
        self.style.scrollbar_position = Some(Some(ScrollbarPosition::NextTo));
        self
    }

    /// Specifies that the `Tree` should be scrollable and should provide a `Scrollbar` that hovers
    /// above the right edge of the items and automatically hides when the user is not scrolling.
    pub fn scrollbar_on_top(mut self) -> Self {
        self.style.scrollbar_position = Some(Some(ScrollbarPosition::OnTop));
        self
    }

    /// The width of the `Scrollbar`.
    pub fn scrollbar_thickness(mut self, w: Scalar) -> Self {
        self.style.scrollbar_thickness = Some(Some(w));
        self
    }

    /// The color of the `Scrollbar`.
    pub fn scrollbar_color(mut self, color: Color) -> Self {
        self.style.scrollbar_color = Some(color);
        self
    }
}

pub struct Items<'a, T: Expandable> {
    stack: Vec<(&'a id_tree::NodeId, usize)>,
    tree: &'a id_tree::Tree<T>,
    items: widget::list::Items<widget::list::Down, widget::list::Dynamic>,
}

impl<'a, T> Items<'a, T>
where
    T: Expandable,
{
    pub fn next(&mut self, ui: &Ui) -> Option<Item<'a, T>> {
        if let Some((current, level)) = self.stack.pop() {
            let node = self.tree.get(current).unwrap();

            if node.data().expanded() {
                let list_item = self.items.next(ui).unwrap();
                self.stack.extend(self.tree.children_ids(current).unwrap().map(|x| (x, level + 1)));
                Some(Item {
                    data: node.data(),
                    item: list_item,
                    level,
                })
            } else {
                self.next(ui)
            }
        } else {
            None
        }
    }
}

pub struct Item<'a, T: Expandable> {
    pub data: &'a T,
    pub item: widget::list::Item<widget::list::Down, widget::list::Dynamic>,
    pub level: usize,
}

impl<'a, T> Widget for Tree<'a, T>
where
    T: Expandable,
{
    type State = State;
    type Style = Style;
    type Event = (
        Items<'a, T>,
        Option<widget::list::Scrollbar<widget::scroll::Y>>,
    );

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut list = widget::list::List::flow_down(visible_tree_items(self.tree))
            .parent(args.id)
            .middle_of(args.id)
            .wh_of(args.id);

        match self.style.scrollbar_position {
            Some(Some(ScrollbarPosition::NextTo)) => list = list.scrollbar_next_to(),
            Some(Some(ScrollbarPosition::OnTop)) => list = list.scrollbar_on_top(),
            _ => {}
        }

        if let Some(Some(w)) = self.style.scrollbar_thickness {
            list = list.scrollbar_thickness(w);
        }

        if let Some(c) = self.style.scrollbar_color {
            list = list.scrollbar_color(c)
        }

        let (list_items, scrollbar) = list.set(args.state.ids.list, args.ui);

        // Prepare iterator
        let mut stack = Vec::new();
        if let Some(root) = self.tree.root_node_id() {
            stack.push((root, 0));
        }

        let items = Items {
            stack,
            tree: self.tree,
            items: list_items,
        };

        (items, scrollbar)
    }
}

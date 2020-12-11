use conrod_core::*;
use std::collections::VecDeque;

pub trait Expandable {
    /// Query the current expansion state
    fn expanded(&self) -> bool;
}

fn visible_tree_items<T: Expandable>(tree: &id_tree::Tree<T>, skip_root: bool) -> usize {
    visible_tree_items_queue(tree, skip_root).len()
}

fn visible_tree_items_queue<T: Expandable>(
    tree: &id_tree::Tree<T>,
    skip_root: bool,
) -> VecDeque<(id_tree::NodeId, usize)> {
    let mut stack: Vec<(id_tree::NodeId, usize)> = Vec::with_capacity(tree.height());
    let mut queue: VecDeque<(id_tree::NodeId, usize)> = VecDeque::new();

    if let Some(root) = tree.root_node_id() {
        stack.push((root.clone(), 0));

        while !stack.is_empty() {
            let (current, level) = stack.pop().unwrap();
            if !skip_root || level > 0 {
                queue.push_back((current.clone(), level));
            }
            if tree
                .get(&current)
                .expect("Invalid node ID in tree")
                .data()
                .expanded()
            {
                stack.extend(
                    tree.children_ids(&current)
                        .unwrap()
                        .cloned()
                        .map(|n| (n, level + 1)),
                );
            }
        }
    }

    queue
}

#[derive(Debug, WidgetCommon)]
pub struct Tree<'a, T: Expandable> {
    #[conrod(common_builder)]
    /// Common widget building params for the `Tree`.
    pub common: widget::CommonBuilder,
    /// Unique styling for the `Tree`.
    pub style: Style,
    tree: &'a id_tree::Tree<T>,
    skip_root: bool,
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
    pub fn new(tree: &'a id_tree::Tree<T>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tree,
            skip_root: false,
        }
    }

    /// Construct a `Tree` without a root node, e.g. for displaying forests.
    pub fn without_root(tree: &'a id_tree::Tree<T>) -> Self {
        Self {
            common: widget::CommonBuilder::default(),
            style: Style::default(),
            tree,
            skip_root: true,
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

pub struct Items {
    queue: VecDeque<(id_tree::NodeId, usize)>,
    items: widget::list::Items<widget::list::Down, widget::list::Dynamic>,
}

pub struct Item {
    pub node_id: id_tree::NodeId,
    pub item: widget::list::Item<widget::list::Down, widget::list::Dynamic>,
    pub level: usize,
}

impl Items {
    pub fn next(&mut self, ui: &Ui) -> Option<Item> {
        self.queue.pop_front().map(|(node_id, level)| {
            let item = self.items.next(ui).unwrap();
            Item {
                node_id,
                item,
                level,
            }
        })
    }

    pub fn new<T: Expandable>(
        tree: &id_tree::Tree<T>,
        items: widget::list::Items<widget::list::Down, widget::list::Dynamic>,
        skip_root: bool,
    ) -> Self {
        Self {
            queue: visible_tree_items_queue(tree, skip_root),
            items,
        }
    }
}

impl<'a, T> Widget for Tree<'a, T>
where
    T: Expandable,
{
    type State = State;
    type Style = Style;
    type Event = (Items, Option<widget::list::Scrollbar<widget::scroll::Y>>);

    fn init_state(&self, id_gen: widget::id::Generator) -> Self::State {
        State {
            ids: Ids::new(id_gen),
        }
    }

    fn style(&self) -> Self::Style {
        self.style.clone()
    }

    fn update(self, args: widget::UpdateArgs<Self>) -> Self::Event {
        let mut list = widget::list::List::flow_down(visible_tree_items(self.tree, self.skip_root))
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
        let items = Items::new(self.tree, list_items, self.skip_root);

        (items, scrollbar)
    }
}

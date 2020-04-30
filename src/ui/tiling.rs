use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug, Clone)]
enum BSPLayout {
    Branch {
        splitter: gtk::Paned,
        left: Box<BSPLayout>,
        right: Box<BSPLayout>,
    },
    Leaf {
        tbox: TilingBox,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum SplitOrientation {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
enum MergeKeep {
    First,
    Second,
}

impl std::ops::Not for MergeKeep {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::First => Self::Second,
            Self::Second => Self::First,
        }
    }
}

impl Default for BSPLayout {
    fn default() -> Self {
        Self::Leaf {
            tbox: TilingBox::new(
                gtk::Label::new(Some("Placeholder")).upcast(),
                None,
                "Tiling Box",
            ),
        }
    }
}

impl BSPLayout {
    fn get_root_widget(&self) -> gtk::Widget {
        match self {
            Self::Branch { splitter, .. } => splitter.clone().upcast(),
            Self::Leaf { tbox } => tbox.clone().upcast(),
        }
    }

    fn find_tbox_parent(&mut self, box_: &TilingBox) -> Option<(&mut Self, MergeKeep)> {
        match self {
            Self::Branch {
                left: box Self::Leaf { tbox },
                ..
            } if *tbox == *box_ => Some((self, MergeKeep::Second)),
            Self::Branch {
                right: box Self::Leaf { tbox },
                ..
            } if *tbox == *box_ => Some((self, MergeKeep::First)),
            Self::Leaf { .. } => None,
            Self::Branch { left, right, .. } => left
                .find_tbox_parent(box_)
                .or_else(move || right.find_tbox_parent(box_)),
        }
    }

    fn find_tbox(&mut self, box_: &TilingBox) -> Option<&mut Self> {
        match self {
            Self::Branch { left, right, .. } => {
                left.find_tbox(box_).or_else(move || right.find_tbox(box_))
            }
            Self::Leaf { tbox } if *tbox == *box_ => Some(self),
            _ => None,
        }
    }

    /// Rotate a branch's orientation. If given a leaf, nothing will happen.
    fn rotate_branch(&self) {
        match self {
            Self::Branch { splitter, .. } => {
                splitter.set_orientation(match splitter.get_orientation() {
                    gtk::Orientation::Vertical => gtk::Orientation::Horizontal,
                    _ => gtk::Orientation::Vertical,
                })
            }
            Self::Leaf { .. } => {}
        }
    }

    /// Split the layout, retaining the given leaf on the given side of the new
    /// split. When given a branch, it will recurse down to leftmost leaf child.
    fn split(&mut self, new: BSPLayout, orientation: SplitOrientation) {
        match self {
            Self::Leaf { tbox } => {
                let (left, right) = match orientation {
                    SplitOrientation::Left | SplitOrientation::Up => {
                        (Box::new(Self::Leaf { tbox: tbox.clone() }), Box::new(new))
                    }
                    SplitOrientation::Right | SplitOrientation::Down => {
                        (Box::new(new), Box::new(Self::Leaf { tbox: tbox.clone() }))
                    }
                };

                let lwidget = left.get_root_widget();
                let rwidget = right.get_root_widget();

                if let Some(parent) = lwidget.get_parent() {
                    parent
                        .downcast::<gtk::Container>()
                        .expect("Left layout child parented to non-container")
                        .remove(&lwidget);
                }
                if let Some(parent) = rwidget.get_parent() {
                    parent
                        .downcast::<gtk::Container>()
                        .expect("Right layout child parented to non-container")
                        .remove(&rwidget);
                }

                *self = Self::Branch {
                    splitter: {
                        let paned = gtk::Paned::new(match orientation {
                            SplitOrientation::Left | SplitOrientation::Right => {
                                gtk::Orientation::Horizontal
                            }
                            SplitOrientation::Up | SplitOrientation::Down => {
                                gtk::Orientation::Vertical
                            }
                        });
                        paned.add1(&lwidget);
                        paned.add2(&rwidget);
                        paned
                    },
                    left,
                    right,
                }
            }
            Self::Branch { left, .. } => left.split(new, orientation),
        }
    }

    /// Merge a branch, keeping the specified child. Returns the new root widget
    /// of the resulting branch
    fn merge(&mut self, keep: MergeKeep) {
        match self {
            Self::Leaf { .. } => {}
            Self::Branch { left, right, .. } => match keep {
                MergeKeep::First => *self = (**left).clone(),
                MergeKeep::Second => *self = (**right).clone(),
            },
        }
    }

    /// Create a layout from a layout description.
    fn from_layout_description(description: LayoutDescription) -> Self {
        match description {
            LayoutDescription::Branch {
                orientation: o,
                left: l,
                right: r,
            } => {
                let paned = gtk::Paned::new(o);
                let left_child = Self::from_layout_description(*l);
                let right_child = Self::from_layout_description(*r);
                paned.add1(&left_child.get_root_widget());
                paned.add2(&right_child.get_root_widget());
                Self::Branch {
                    splitter: paned,
                    left: Box::new(left_child),
                    right: Box::new(right_child),
                }
            }
            LayoutDescription::Leaf(tbox) => Self::Leaf { tbox },
        }
    }

    /// Rebuilds all layouting widgets, discarding the old ones. Returns the
    /// rebuilt root widget
    fn rebuild_layout(&mut self) -> gtk::Widget {
        match self {
            Self::Branch {
                left,
                right,
                splitter,
            } => {
                let left_child = left.rebuild_layout();
                let right_child = right.rebuild_layout();
                let orientation = splitter.get_orientation();

                let new_paned = gtk::Paned::new(orientation);
                new_paned.add1(&left_child);
                new_paned.add2(&right_child);

                *splitter = new_paned.clone();
                new_paned.upcast()
            }
            Self::Leaf { tbox } => {
                if let Some(parent) = tbox.get_parent() {
                    parent
                        .downcast::<gtk::Container>()
                        .expect("TilingBox parented to non-container")
                        .remove(tbox);
                }
                tbox.clone().upcast()
            }
        }
    }

    /// Restores the parent relationships for all widgets in the layout
    fn reparent_all(&self) {
        if let Self::Branch {
            splitter,
            left,
            right,
        } = self
        {
            let lwidget = left.get_root_widget();
            let rwidget = right.get_root_widget();

            left.reparent_all();
            right.reparent_all();

            if let Some(parent) = lwidget.get_parent() {
                parent
                    .downcast::<gtk::Container>()
                    .expect("Left layout child parented to non-container")
                    .remove(&lwidget);
            }
            if let Some(parent) = rwidget.get_parent() {
                parent
                    .downcast::<gtk::Container>()
                    .expect("Right layout child parented to non-container")
                    .remove(&rwidget);
            }

            splitter.add1(&lwidget);
            splitter.add2(&rwidget);
        }
    }

    fn iter(&self) -> BSPIter {
        BSPIter::new(self)
    }
}

struct BSPIter<'a> {
    stack: Vec<&'a BSPLayout>,
}

impl<'a> BSPIter<'a> {
    fn new(root: &'a BSPLayout) -> Self {
        let mut stack = Vec::with_capacity(32);
        stack.push(root);
        BSPIter { stack }
    }
}

impl<'a> Iterator for BSPIter<'a> {
    type Item = &'a TilingBox;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.stack.pop() {
            match item {
                BSPLayout::Leaf { tbox } => Some(tbox),
                BSPLayout::Branch { left, right, .. } => {
                    self.stack.push(&*left);
                    self.stack.push(&*right);
                    self.next()
                }
            }
        } else {
            None
        }
    }
}

pub enum LayoutDescription {
    Branch {
        orientation: gtk::Orientation,
        left: Box<LayoutDescription>,
        right: Box<LayoutDescription>,
    },
    Leaf(TilingBox),
}

pub struct TilingAreaPrivate {
    layout: Rc<RefCell<BSPLayout>>,
    maximized: Rc<Cell<bool>>,
}

impl ObjectSubclass for TilingAreaPrivate {
    const NAME: &'static str = "TilingArea";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    //fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {}

    fn new() -> Self {
        Self {
            layout: Rc::new(RefCell::new(BSPLayout::default())),
            maximized: Rc::new(Cell::new(false)),
        }
    }
}

impl ObjectImpl for TilingAreaPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {}
}

impl WidgetImpl for TilingAreaPrivate {}

impl ContainerImpl for TilingAreaPrivate {}

impl BoxImpl for TilingAreaPrivate {}

impl TilingAreaPrivate {
    fn connect_tbox(
        box_: &gtk::Box,
        tbox: &TilingBox,
        layout: Rc<RefCell<BSPLayout>>,
        maximized: Rc<Cell<bool>>,
    ) {
        tbox.connect_maximize_clicked(clone!(
            @strong box_,
            @strong maximized,
            @strong layout => move |t| {
                let max = maximized.get();
                if max {
                    layout.borrow().reparent_all();
                    let root = layout.borrow().get_root_widget();
                    root.show_all();
                    Self::swap_widget(&box_, &root);
                } else {
                    if let Some(parent) = t.get_parent() {
                        parent.downcast::<gtk::Container>().expect("Tiling Box parented to non-container").remove(t);
                    }
                    Self::swap_widget(&box_, &t.clone().upcast());
                }
                maximized.set(!max);
        }));

        tbox.connect_close_clicked(clone!(@strong box_, @strong layout => move |t| {
            let mut layout_m = layout.borrow_mut();
            if let Some((parent, keep)) = layout_m.find_tbox_parent(t) {
                parent.merge(keep);
                let new_root = layout_m.rebuild_layout();
                new_root.show_all();
                Self::swap_widget(&box_, &new_root);
            }
        }));

        tbox.connect_split_clicked(
            clone!(@strong box_, @strong layout, @strong maximized => move |t, dir| {
                let mut layout_m = layout.borrow_mut();
                if let Some(tbox) = layout_m.find_tbox(t) {
                    // create new tbox
                    let new_tbox = TilingBox::new(gtk::Label::new(Some("foo")).upcast(), None, "Foobar");
                    Self::connect_tbox(&box_, &new_tbox, layout.clone(), maximized.clone());
                    let new = BSPLayout::Leaf {
                        tbox: new_tbox
                    };

                    // split old tbox
                    tbox.split(new, dir);
                    let new_root = layout_m.rebuild_layout();
                    new_root.show_all();
                    Self::swap_widget(&box_, &new_root);
                }
            }
            ),
        );

        tbox.connect_rotate_clicked(clone!(@strong box_, @strong layout => move |t| {
            let mut layout_m = layout.borrow_mut();
            if let Some((tbox, _)) = layout_m.find_tbox_parent(t) {
                tbox.rotate_branch();
            }
        }));
    }

    fn from_layout_description(&self, box_: &gtk::Box, description: LayoutDescription) {
        let new_layout = BSPLayout::from_layout_description(description);

        for tbox in new_layout.iter() {
            Self::connect_tbox(box_, tbox, self.layout.clone(), self.maximized.clone());
        }

        self.layout.replace(new_layout);
        let w = self.layout.borrow().get_root_widget();

        Self::swap_widget(box_, &w);
    }

    fn swap_widget(box_: &gtk::Box, widget: &gtk::Widget) {
        // This box should only ever have one child
        for c in box_.get_children() {
            box_.remove(&c);
        }

        box_.pack_start(widget, true, true, 0);
    }
}

glib_wrapper! {
    pub struct TilingArea(
        Object<subclass::simple::InstanceStruct<TilingAreaPrivate>,
        subclass::simple::ClassStruct<TilingAreaPrivate>,
        TilingAreaClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || TilingAreaPrivate::get_type().to_glib(),
    }
}

impl TilingArea {
    pub fn new() -> Self {
        glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap()
    }

    pub fn new_from_layout_description(description: LayoutDescription) -> Self {
        let obj = Self::new();
        let imp = TilingAreaPrivate::from_instance(&obj);
        imp.from_layout_description(&obj.clone().upcast(), description);
        obj
    }
}

impl Default for TilingArea {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TilingBoxPrivate {
    title_box: gtk::Box,
    title_label: gtk::Label,
    title_menubutton: gtk::MenuButton,
    title_popover: gtk::PopoverMenu,
    close_button: gtk::Button,
    maximize_button: gtk::Button,
}

// TilingBox Signals
pub const MAXIMIZE_CLICKED: &str = "maximize-clicked";
pub const CLOSE_CLICKED: &str = "close-clicked";
pub const SPLIT_CLICKED: &str = "split-clicked";
pub const ROTATE_CLICKED: &str = "rotate-clicked";

impl ObjectSubclass for TilingBoxPrivate {
    const NAME: &'static str = "TilingBox";

    type ParentType = gtk::Box;
    type Instance = subclass::simple::InstanceStruct<Self>;
    type Class = subclass::simple::ClassStruct<Self>;

    glib_object_subclass!();

    // Called right before the first time an instance of the new
    // type is created. Here class specific settings can be performed,
    // including installation of properties and registration of signals
    // for the new type.
    fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {
        class.add_signal(
            MAXIMIZE_CLICKED,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
        class.add_signal(
            CLOSE_CLICKED,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
        class.add_signal(
            ROTATE_CLICKED,
            glib::SignalFlags::empty(),
            &[],
            glib::types::Type::Unit,
        );
        class.add_signal(
            SPLIT_CLICKED,
            glib::SignalFlags::empty(),
            &[glib::types::Type::U8],
            glib::types::Type::Unit,
        );
    }

    fn new() -> Self {
        let title_menubutton = gtk::MenuButtonBuilder::new()
            .relief(gtk::ReliefStyle::None)
            .focus_on_click(false)
            .build();
        let title_popover = gtk::PopoverMenu::new();
        title_menubutton.set_popover(Some(&title_popover));

        Self {
            title_box: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            title_label: gtk::Label::new(Some("Tiling Box")),
            title_menubutton,
            title_popover,
            close_button: gtk::Button::new_from_icon_name(
                Some("window-close-symbolic"),
                gtk::IconSize::Menu,
            ),
            maximize_button: gtk::Button::new_from_icon_name(
                Some("window-maximize-symbolic"),
                gtk::IconSize::Menu,
            ),
        }
    }
}

impl ObjectImpl for TilingBoxPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let box_ = obj.clone().downcast::<gtk::Box>().unwrap();
        box_.set_orientation(gtk::Orientation::Vertical);

        self.title_box.set_vexpand(false);

        let title_label_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        title_label_box.add(&self.title_label);
        title_label_box.add(&gtk::Image::new_from_icon_name(
            Some("pan-down-symbolic"),
            gtk::IconSize::Menu,
        ));
        self.title_menubutton.add(&title_label_box);
        self.title_box
            .pack_start(&self.title_menubutton, false, false, 0);

        // Popover
        let main_menu = gtk::Box::new(gtk::Orientation::Vertical, 8);
        self.title_popover.add(&main_menu);

        // {
        //     let split_left_button =
        //         gtk::Button::new_from_icon_name(Some("go-previous-symbolic"), gtk::IconSize::Menu);
        //     split_left_button.connect_clicked(clone!(@strong obj, @strong self.title_popover as popover => move |_| {
        //         popover.open_submenu("new-split");
        //         //obj.emit(SPLIT_CLICKED, &[&(3 as u8)]).unwrap();
        //     }));

        main_menu.add(
            &gtk::ModelButtonBuilder::new()
                .text("Split View")
                .menu_name("new-split")
                .build(),
        );
        main_menu.add(&{
            let btn = gtk::ModelButtonBuilder::new().text("Rotate View").build();
            btn.connect_clicked(clone!(@strong obj => move |_| {
               obj.emit(ROTATE_CLICKED, &[]).unwrap();
            }));
            btn
        });
        main_menu.show_all();

        let new_split_menu = gtk::Box::new(gtk::Orientation::Vertical, 2);
        new_split_menu.add(&gtk::ModelButtonBuilder::new().text("2D View").build());
        new_split_menu.add(&gtk::ModelButtonBuilder::new().text("3D View").build());
        new_split_menu.add(&gtk::ModelButtonBuilder::new().text("Node Area").build());

        self.title_popover.add(&new_split_menu);
        self.title_popover
            .set_child_submenu(&new_split_menu, Some("new-split"));
        new_split_menu.show_all();

        // Close Button
        self.close_button.set_tooltip_text(Some("Close"));
        self.close_button.set_relief(gtk::ReliefStyle::None);
        self.close_button.set_focus_on_click(false);
        self.close_button
            .connect_clicked(clone!(@strong obj => move |_| {
                obj.emit(CLOSE_CLICKED, &[]).unwrap();
            }));
        self.title_box.pack_end(&self.close_button, false, false, 0);

        // Maximize Button
        self.maximize_button.set_tooltip_text(Some("Maximize"));
        self.maximize_button.set_relief(gtk::ReliefStyle::None);
        self.maximize_button.set_focus_on_click(false);
        self.maximize_button
            .connect_clicked(clone!(@strong obj => move |_| {
                obj.emit(MAXIMIZE_CLICKED, &[]).unwrap();
            }));
        self.title_box
            .pack_end(&self.maximize_button, false, false, 0);

        box_.add(&self.title_box);
    }
}

impl WidgetImpl for TilingBoxPrivate {}

impl ContainerImpl for TilingBoxPrivate {}

impl BoxImpl for TilingBoxPrivate {}

impl TilingBoxPrivate {
    fn set_menu(&self, menu: Option<gio::Menu>) {
        let m = match menu {
            Some(m) => m,
            _ => gio::Menu::new(),
        };
        let gtkmenu = gtk::MenuBar::new_from_model(&m);
    }

    fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }
}

glib_wrapper! {
    pub struct TilingBox(
        Object<subclass::simple::InstanceStruct<TilingBoxPrivate>,
        subclass::simple::ClassStruct<TilingBoxPrivate>,
        TilingBoxClass>)
        @extends gtk::Widget, gtk::Container, gtk::Box;

    match fn {
        get_type => || TilingBoxPrivate::get_type().to_glib(),
    }
}

impl TilingBox {
    pub fn new(inner: gtk::Widget, menu: Option<gio::Menu>, title: &str) -> Self {
        let tbox: Self = glib::Object::new(Self::static_type(), &[])
            .unwrap()
            .downcast()
            .unwrap();
        tbox.set_menu(menu);
        tbox.set_title(title);
        tbox.pack_end(&inner, true, true, 0);
        tbox
    }

    pub fn set_menu(&self, menu: Option<gio::Menu>) {
        let imp = TilingBoxPrivate::from_instance(self);
        imp.set_menu(menu)
    }

    pub fn set_title(&self, title: &str) {
        let imp = TilingBoxPrivate::from_instance(self);
        imp.set_title(title);
    }

    pub fn connect_close_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local(CLOSE_CLICKED, true, move |w| {
            let tbox = w[0].clone().downcast::<TilingBox>().unwrap().get().unwrap();
            f(&tbox);
            None
        })
        .unwrap()
    }

    pub fn connect_rotate_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local(ROTATE_CLICKED, true, move |w| {
            let tbox = w[0].clone().downcast::<TilingBox>().unwrap().get().unwrap();
            f(&tbox);
            None
        })
        .unwrap()
    }

    pub fn connect_maximize_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local(MAXIMIZE_CLICKED, true, move |w| {
            let tbox = w[0].clone().downcast::<TilingBox>().unwrap().get().unwrap();
            f(&tbox);
            None
        })
        .unwrap()
    }

    pub fn connect_split_clicked<F: Fn(&Self, SplitOrientation) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local(SPLIT_CLICKED, true, move |w| {
            let tbox = w[0].clone().downcast::<TilingBox>().unwrap().get().unwrap();
            let dir = match w[1].get_some::<u8>().unwrap() {
                0 => SplitOrientation::Up,
                1 => SplitOrientation::Down,
                2 => SplitOrientation::Left,
                3 => SplitOrientation::Right,
                _ => panic!("Invalid SplitOrientation in signal handler"),
            };
            f(&tbox, dir);
            None
        })
        .unwrap()
    }

    pub fn contains(&self, widget: &gtk::Widget) -> bool {
        &self.get_children()[1] == widget
    }
}

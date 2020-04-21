use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
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

enum SplitOrientation {
    Up,
    Down,
    Left,
    Right,
}

enum MergeKeep {
    First,
    Second,
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
            } if tbox == box_ => Some((self, MergeKeep::First)),
            Self::Branch {
                right: box Self::Leaf { tbox },
                ..
            } if tbox == box_ => Some((self, MergeKeep::Second)),
            Self::Leaf { .. } => None,
            Self::Branch { left, right, .. } => left
                .find_tbox_parent(box_)
                .or_else(move || right.find_tbox_parent(box_)),
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
    fn split(self, new: BSPLayout, orientation: SplitOrientation) -> Self {
        match self {
            Self::Leaf { tbox } => {
                let (left, right) = match orientation {
                    SplitOrientation::Left | SplitOrientation::Up => {
                        (Box::new(Self::Leaf { tbox }), Box::new(new))
                    }
                    SplitOrientation::Right | SplitOrientation::Down => {
                        (Box::new(new), Box::new(Self::Leaf { tbox }))
                    }
                };
                Self::Branch {
                    splitter: {
                        let paned = gtk::Paned::new(match orientation {
                            SplitOrientation::Left | SplitOrientation::Right => {
                                gtk::Orientation::Horizontal
                            }
                            SplitOrientation::Up | SplitOrientation::Down => {
                                gtk::Orientation::Vertical
                            }
                        });
                        paned.add1(&left.get_root_widget());
                        paned.add2(&right.get_root_widget());
                        paned
                    },
                    left,
                    right,
                }
            }
            Self::Branch { left, .. } => left.split(new, orientation),
        }
    }

    fn merge(self, keep: MergeKeep) -> Self {
        match self {
            Self::Leaf { .. } => self,
            Self::Branch { left, right, .. } => match keep {
                MergeKeep::First => *left,
                MergeKeep::Second => *right,
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

            left.reparent_all();
            right.reparent_all();
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
    fn from_layout_description(&self, box_: &gtk::Box, description: LayoutDescription) {
        let new_layout = BSPLayout::from_layout_description(description);

        for tbox in new_layout.iter() {
            tbox.connect_maximize_clicked(clone!(
                @strong box_,
                @strong self.maximized as maximized,
                @strong self.layout as layout => move |t| {
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

            tbox.connect_close_clicked(clone!(@strong self.layout as layout => move |t| {
                let mut layout_m = layout.borrow_mut();
                if let Some((parent, keep)) = layout_m.find_tbox_parent(t) {
                    let new_parent = parent.clone().merge(keep);
                    *parent = new_parent;
                }
                layout_m.reparent_all();
            }));
        }

        self.layout.replace(new_layout);
        let w = self.layout.borrow().get_root_widget();

        Self::swap_widget(box_, &w);
    }

    fn swap_widget(box_: &gtk::Box, widget: &gtk::Widget) {
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

pub struct TilingBoxPrivate {
    title_box: gtk::Box,
    title_label: gtk::Label,
    title_menubutton: gtk::MenuButton,
    close_button: gtk::Button,
    maximize_button: gtk::Button,
}

// TilingBox Signals
pub const MAXIMIZE_CLICKED: &str = "maximize-clicked";
pub const CLOSE_CLICKED: &str = "close-clicked";

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
    }

    fn new() -> Self {
        Self {
            title_box: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            title_label: gtk::Label::new(Some("Tiling Box")),
            title_menubutton: gtk::MenuButtonBuilder::new()
                .relief(gtk::ReliefStyle::None)
                .focus_on_click(false)
                .build(),
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
    fn prepend_tiling(menu: &gio::Menu) {
        menu.prepend(Some("Split Horizontally"), None);
        menu.prepend(Some("Split Vertically"), None);
    }

    fn set_menu(&self, menu: Option<gio::Menu>) {
        let m = match menu {
            Some(m) => m,
            _ => gio::Menu::new(),
        };
        Self::prepend_tiling(&m);
        let popover = gtk::Popover::new_from_model(Some(&self.title_menubutton), &m);
        self.title_menubutton.set_popover(Some(&popover))
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

    pub fn connect_maximize_clicked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local(MAXIMIZE_CLICKED, true, move |w| {
            let tbox = w[0].clone().downcast::<TilingBox>().unwrap().get().unwrap();
            f(&tbox);
            None
        })
        .unwrap()
    }

    pub fn contains(&self, widget: &gtk::Widget) -> bool {
        if &self.get_children()[1] == widget {
            true
        } else {
            false
        }
    }
}

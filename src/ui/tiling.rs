use crate::lang::*;

use glib::subclass;
use glib::subclass::prelude::*;
use glib::translate::*;
use glib::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use std::cell::RefCell;

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

impl BSPLayout {
    fn get_root_widget(&self) -> gtk::Widget {
        match self {
            Self::Branch { splitter, .. } => splitter.clone().upcast(),
            Self::Leaf { tbox } => tbox.clone().upcast(),
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
    layout: RefCell<BSPLayout>,
}

impl ObjectSubclass for TilingAreaPrivate {
    const NAME: &'static str = "TilingArea";

    type ParentType = gtk::Stack;
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
            layout: RefCell::new(BSPLayout::Leaf {
                tbox: TilingBox::new(
                    gtk::Label::new(Some("Placeholder")).upcast(),
                    None,
                    "Tiling Box",
                ),
            }),
        }
    }
}

impl ObjectImpl for TilingAreaPrivate {
    glib_object_impl!();

    fn constructed(&self, obj: &glib::Object) {
        let stack = obj.clone().downcast::<gtk::Stack>().unwrap();
    }
}

impl WidgetImpl for TilingAreaPrivate {}

impl ContainerImpl for TilingAreaPrivate {}

impl StackImpl for TilingAreaPrivate {}

impl TilingAreaPrivate {
    fn from_layout_description(&self, stack: &gtk::Stack, description: LayoutDescription) {
        let new_layout = BSPLayout::from_layout_description(description);
        self.layout.replace(new_layout);
        let w = self.layout.borrow().get_root_widget();
        stack.add(&w);
        stack.set_visible_child(&w);
    }
}

glib_wrapper! {
    pub struct TilingArea(
        Object<subclass::simple::InstanceStruct<TilingAreaPrivate>,
        subclass::simple::ClassStruct<TilingAreaPrivate>,
        TilingAreaClass>)
        @extends gtk::Widget, gtk::Container, gtk::Stack;

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
    //fn class_init(class: &mut subclass::simple::ClassStruct<Self>) {}

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
        self.title_box.pack_end(&self.close_button, false, false, 0);

        // Maximize Button
        self.maximize_button.set_tooltip_text(Some("Maximize"));
        self.maximize_button.set_relief(gtk::ReliefStyle::None);
        self.maximize_button.set_focus_on_click(false);
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
}

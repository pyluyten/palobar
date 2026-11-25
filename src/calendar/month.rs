use chrono::{Datelike, Days, Local, Months, NaiveDate};
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

struct Month {
    grid: gtk::Grid,
    title: gtk::Label,
    shown: Cell<NaiveDate>,
    selected: Rc<Cell<NaiveDate>>,
}

impl Month {
    fn draw(&self) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        let month = self.shown.get();
        self.title.set_text(&month.format("%B %Y").to_string());
        for (column, day) in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
            .iter()
            .enumerate()
        {
            let label = gtk::Label::new(Some(day));
            label.add_css_class("date-muted");
            self.grid.attach(&label, column as i32, 0, 1, 1);
        }
        let first = month
            .checked_sub_days(Days::new(month.weekday().num_days_from_sunday().into()))
            .unwrap_or(month);
        let today = Local::now().date_naive();
        let mut first_button = None;
        for offset in 0..42 {
            let Some(day) = first.checked_add_days(Days::new(offset)) else {
                break;
            };
            let button = gtk::ToggleButton::builder()
                .label(day.day().to_string())
                .tooltip_text(day.format("%A, %B %-d, %Y").to_string())
                .build();
            if let Some(leader) = &first_button {
                button.set_group(Some(leader));
            } else {
                first_button = Some(button.clone());
            }
            button.add_css_class("date-grid-day");
            if day.month() != month.month() {
                button.add_css_class("adjacent");
            }
            if day == today {
                button.add_css_class("today");
            }
            button.set_active(day == self.selected.get());
            let selected = self.selected.clone();
            button.connect_toggled(move |button| {
                if button.is_active() {
                    selected.set(day);
                }
            });
            self.grid
                .attach(&button, (offset % 7) as i32, (offset / 7 + 1) as i32, 1, 1);
        }
    }
}

pub(super) fn widget() -> gtk::Box {
    let today = Local::now().date_naive();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let previous = super::icon_button("pan-start-symbolic", "Previous Month");
    let next = super::icon_button("pan-end-symbolic", "Next Month");
    let title = gtk::Label::builder().hexpand(true).build();
    title.add_css_class("date-section-title");
    navigation.append(&previous);
    navigation.append(&title);
    navigation.append(&next);
    box_.append(&navigation);
    let grid = gtk::Grid::builder()
        .column_homogeneous(true)
        .column_spacing(2)
        .row_spacing(2)
        .build();
    box_.append(&grid);
    let state = Rc::new(Month {
        grid,
        title,
        shown: Cell::new(today.with_day(1).unwrap_or(today)),
        selected: Rc::new(Cell::new(today)),
    });
    state.draw();
    for (button, next) in [(previous, false), (next, true)] {
        let state = state.clone();
        button.connect_clicked(move |_| {
            let month = state.shown.get();
            let target = if next {
                month.checked_add_months(Months::new(1))
            } else {
                month.checked_sub_months(Months::new(1))
            };
            if let Some(target) = target {
                state.shown.set(target);
                state.draw();
            }
        });
    }
    box_
}

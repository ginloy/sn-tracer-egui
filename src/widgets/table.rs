use eframe::egui;
use egui::*;
use egui_extras::*;
use tracing::*;

pub struct CustomTable<'a> {
    headers: &'a [&'a str],
    columns: &'a [Vec<String>],
}

impl<'a> CustomTable<'a> {
    pub fn new(headers: &'a [&str], columns: &'a [Vec<String>]) -> Self {
        Self { headers, columns }
    }

    fn num_cols(&self) -> usize {
        std::cmp::max(self.headers.len(), self.columns.len())
    }

    fn num_rows(&self) -> usize {
        self.columns.iter().map(Vec::len).max().unwrap_or_default()
    }

    fn header_cell(&self, ui: &mut Ui, col_idx: usize) -> Response {
        let contents = *self.headers.get(col_idx).unwrap_or(&"");
        let text = RichText::new(contents).strong();
        let label = Label::new(text).wrap(false);
        ui.add(label)
    }

    fn header(&self, mut header: TableRow, response: &mut Response) {
        (0..self.num_cols()).map(|i| {
            header.col(|ui| {
                self.header_cell(ui, i);
            }).1
        })
        .for_each(|r| {
            *response = response.union(r);
        })
    }

    fn body_cell(&self, ui: &mut Ui, row_idx: usize, col_idx: usize) -> Response {
        ui.add(
            Label::new(
                self.columns
                    .get(col_idx)
                    .and_then(|col| col.get(row_idx).map(String::clone))
                    .unwrap_or_default(),
            )
            .wrap(false),
        )
    }

    fn body_row(&self, mut row: TableRow, row_idx: usize, response: &mut Response) {
        (0..self.num_cols()).map(|i| {
            row.col(|ui| {
                self.body_cell(ui, row_idx, i);
            }).1
        })
        .for_each(|r| {
            *response = response.union(r);
        })
    }

    fn body(&self, body: TableBody, response: &mut Response, row_height: f32) {
        body.rows(row_height, self.num_rows(), |row_idx, row| {
            self.body_row(row, row_idx, response);
        })
    }

    fn table(&self, ui: &mut Ui) -> Response {
        let mut res = ui.allocate_response(Vec2 { x: 0.0, y: 0.0 }, Sense::hover());
        let num_cols = self.num_cols();
        let row_height = ui.text_style_height(&TextStyle::Body);
        let mean_width = ui.available_width() / num_cols as f32;
        let width_range = Rangef::new(0.75 * mean_width, 1.25 * mean_width);
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .columns(Column::auto().range(width_range), num_cols - 1)
            .column(Column::remainder().at_least(width_range.min))
            .stick_to_bottom(true)
            .header(1.2 * row_height, |header| {
               self.header(header, &mut res);
            })
            .body(|body| {
                self.body(body, &mut res, row_height);
            });
        res
    }
}

impl Widget for CustomTable<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ScrollArea::horizontal()
            .auto_shrink(false)
            .show(ui, |ui| self.table(ui))
            .inner
    }
}

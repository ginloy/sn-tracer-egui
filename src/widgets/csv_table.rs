use anyhow::*;
use eframe::egui;
use egui::*;
use egui_extras::*;
use tracing::*;

#[derive(Debug, Clone)]
pub struct CsvTable<'a> {
    contents: &'a str,
}

fn union_response(a: Option<Response>, b: Response) -> Response {
    match a {
        Some(a) => a.union(b),
        None => b,
    }
}

impl<'a> CsvTable<'a> {
    #[instrument]
    pub fn new(contents: &'a str) -> Result<Self> {
        if contents.is_empty() {
            warn!("Empty contents to table");
            bail!("Cannot create CsvTable with empty contents");
        }
        info!("Created new CsvTable Widget");
        Ok(Self { contents })
    }

    #[instrument(level = "trace", ret)]
    fn num_cols(&self) -> usize {
        self.contents
            .lines()
            .map(|line| line.split(',').count())
            .max()
            .unwrap_or(0)
    }

    #[instrument(level = "trace", ret)]
    fn get_matrix(&self) -> Vec<Vec<String>> {
        let cols = self.num_cols();
        let res = self
            .contents
            .lines()
            .map(|line| {
                let mut cells = line.split(',');
                (0..cols)
                    .map(|_| cells.next().unwrap_or_default().into())
                    .collect()
            })
            .collect();
        res
    }

    #[instrument(level = "trace", skip(self, ui), ret)]
    fn draw_table(&self, ui: &mut Ui) -> Response {
        let mut res = None;
        let num_cols = self.num_cols();
        let row_height = ui.text_style_height(&TextStyle::Body);
        let data = self.get_matrix();
        TableBuilder::new(ui)
            .striped(true)
            // .resizable(true)
            .columns(Column::remainder(), num_cols)
            .stick_to_bottom(true)
            .header(1.2 * row_height, |mut header| {
                let headers = &data[0];
                headers.iter().for_each(|s| {
                    header.col(|ui| {
                        let response = ui.add(Label::new(RichText::new(s).strong()).wrap(true));
                        let mut temp = None;
                        std::mem::swap(&mut res, &mut temp);
                        res = Some(union_response(temp, response));
                    });
                })
            })
            .body(|mut body| {
                (1..data.len()).for_each(|i| {
                    body.row(row_height, |mut row| {
                        data[i].iter().for_each(|s| {
                            row.col(|ui| {
                                let response = ui.add(Label::new(RichText::new(s)).wrap(true));
                                let mut temp = None;
                                std::mem::swap(&mut res, &mut temp);
                                res = Some(union_response(temp, response));
                            });
                        })
                    })
                });
                // body.rows(row_height, data.len() - 1, |i, mut row| {
                //     let row_data = &data[i + 1];
                //     row_data.iter().for_each(|s| {
                //         row.col(|ui| {
                //             let response = ui.add(Label::new(RichText::new(s)).wrap(false));
                //             let mut temp = None;
                //             std::mem::swap(&mut res, &mut temp);
                //             res = Some(union_response(temp, response));
                //         });
                //     })
                // })
            });
        res.expect("This should not happen")
    }
}

impl Widget for CsvTable<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let mut res = None;
        ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
            let response = self.draw_table(ui);
            res = Some(response);
        });
        res.expect("This should not happen")
    }
}

/// What `GET /quotations` filters on. `status` is one of the three list
/// views (`active`, `expired`, `cancelled`) and is validated by `service`;
/// `today` is `YYYY-MM-DD` and is what separates active from expired.
#[derive(Debug)]
pub(crate) struct ListFilter {
    pub search: Option<String>,
    pub status: Option<String>,
    pub today: String,
}

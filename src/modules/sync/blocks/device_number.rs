/// A number taken for a document, with the prefix it must be rendered under.
#[derive(Debug)]
pub(crate) struct DeviceNumber {
    pub prefix: String,
    pub number: i64,
}
